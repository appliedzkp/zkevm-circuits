use super::{
    common_gadget::UpdateBalanceGadget,
    math_gadget::{
        ConstantDivisionGadget, IsEqualGadget, IsEqualWordGadget, IsZeroGadget, IsZeroWordGadget,
        LtGadget, LtWordGadget, MinMaxGadget,
    },
    rlc, AccountAddress, CachedRegion, CellType, MemoryAddress, StoredExpression, U64Cell,
};
use crate::{
    evm_circuit::{
        param::STACK_CAPACITY,
        step::{ExecutionState, Step},
        table::{FixedTableTag, Lookup, RwValues, Table},
        util::{Cell, RandomLinearCombination},
    },
    table::{
        AccountFieldTag, BytecodeFieldTag, CallContextFieldTag, TxContextFieldTag, TxLogFieldTag,
        TxReceiptFieldTag,
    },
    util::{
        build_tx_log_expression, query_expression,
        word::{Word32, Word32Cell, WordExpr, WordLoHi, WordLoHiCell},
        Challenges, Expr,
    },
};
use bus_mapping::{
    circuit_input_builder::FeatureConfig, operation::Target, state_db::EMPTY_CODE_HASH_LE,
};
use eth_types::{Field, OpsIdentity};
use gadgets::util::{not, sum};
use halo2_proofs::{
    circuit::Value,
    plonk::{
        ConstraintSystem, Error,
        Expression::{self, Constant},
        VirtualCells,
    },
};

// Max degree allowed in all expressions passing through the ConstraintBuilder.
// It aims to cap `extended_k` to 2, which allows constraint degree to 2^2+1,
// but each ExecutionGadget has implicit selector degree 3, so here it only
// allows 2^2+1-3 = 2.
const MAX_DEGREE: usize = 5;
const IMPLICIT_DEGREE: usize = 3;

pub(crate) enum Transition<T> {
    Same,
    Delta(T),
    To(T),
    Any,
}

impl<F> Default for Transition<F> {
    fn default() -> Self {
        Self::Same
    }
}

#[derive(Default)]
pub(crate) struct StepStateTransition<F: Field> {
    pub(crate) rw_counter: Transition<Expression<F>>,
    pub(crate) call_id: Transition<Expression<F>>,
    pub(crate) is_root: Transition<Expression<F>>,
    pub(crate) is_create: Transition<Expression<F>>,
    pub(crate) code_hash: Transition<WordLoHi<Expression<F>>>,
    pub(crate) program_counter: Transition<Expression<F>>,
    pub(crate) stack_pointer: Transition<Expression<F>>,
    pub(crate) gas_left: Transition<Expression<F>>,
    pub(crate) memory_word_size: Transition<Expression<F>>,
    pub(crate) reversible_write_counter: Transition<Expression<F>>,
    pub(crate) log_id: Transition<Expression<F>>,
}

impl<F: Field> StepStateTransition<F> {
    pub(crate) fn new_context() -> Self {
        Self {
            program_counter: Transition::To(0.expr()),
            stack_pointer: Transition::To(STACK_CAPACITY.expr()),
            memory_word_size: Transition::To(0.expr()),
            ..Default::default()
        }
    }

    pub(crate) fn any() -> Self {
        Self {
            rw_counter: Transition::Any,
            call_id: Transition::Any,
            is_root: Transition::Any,
            is_create: Transition::Any,
            code_hash: Transition::Any,
            program_counter: Transition::Any,
            stack_pointer: Transition::Any,
            gas_left: Transition::Any,
            memory_word_size: Transition::Any,
            reversible_write_counter: Transition::Any,
            log_id: Transition::Any,
        }
    }
}

/// ReversionInfo counts `rw_counter` of reversion for gadgets, by tracking how
/// many reversions that have been used. Gadgets should call
/// [`EVMConstraintBuilder::reversion_info`] to get [`ReversionInfo`] with
/// `reversible_write_counter` initialized at current tracking one if no
/// `call_id` is specified, then pass it as mutable reference when doing state
/// write.
#[derive(Clone, Debug)]
pub(crate) struct ReversionInfo<F> {
    /// Field [`CallContextFieldTag::RwCounterEndOfReversion`] read from call
    /// context.
    rw_counter_end_of_reversion: Cell<F>,
    /// Field [`CallContextFieldTag::IsPersistent`] read from call context.
    is_persistent: Cell<F>,
    /// Current cumulative reversible_write_counter.
    reversible_write_counter: Expression<F>,
}

impl<F: Field> ReversionInfo<F> {
    pub(crate) fn rw_counter_end_of_reversion(&self) -> Expression<F> {
        self.rw_counter_end_of_reversion.expr()
    }

    pub(crate) fn is_persistent(&self) -> Expression<F> {
        self.is_persistent.expr()
    }

    /// Returns `rw_counter_end_of_reversion - reversible_write_counter` and
    /// increases `reversible_write_counter` by `1` when `inc_selector` is
    /// enabled.
    pub(crate) fn rw_counter_of_reversion(&mut self, inc_selector: Expression<F>) -> Expression<F> {
        let rw_counter_of_reversion =
            self.rw_counter_end_of_reversion.expr() - self.reversible_write_counter.clone();
        self.reversible_write_counter =
            self.reversible_write_counter.clone() + inc_selector * 1.expr();
        rw_counter_of_reversion
    }

    pub(crate) fn assign(
        &self,
        region: &mut CachedRegion<'_, '_, F>,
        offset: usize,
        rw_counter_end_of_reversion: usize,
        is_persistent: bool,
    ) -> Result<(), Error> {
        self.rw_counter_end_of_reversion.assign(
            region,
            offset,
            Value::known(F::from(rw_counter_end_of_reversion as u64)),
        )?;
        self.is_persistent
            .assign(region, offset, Value::known(F::from(is_persistent as u64)))?;
        Ok(())
    }

    pub(crate) fn rw_delta(&self) -> Expression<F> {
        // From definition, rws include:
        // Field [`CallContextFieldTag::RwCounterEndOfReversion`] read from call context.
        // Field [`CallContextFieldTag::IsPersistent`] read from call context.
        2.expr()
    }
}

pub(crate) trait ConstrainBuilderCommon<F: Field> {
    fn add_constraint(&mut self, name: &'static str, constraint: Expression<F>);

    fn require_zero(&mut self, name: &'static str, constraint: Expression<F>) {
        self.add_constraint(name, constraint);
    }

    fn require_zero_word(&mut self, name: &'static str, word: WordLoHi<Expression<F>>) {
        self.require_equal_word(name, word, WordLoHi::zero());
    }

    fn require_equal_word(
        &mut self,
        name: &'static str,
        lhs: WordLoHi<Expression<F>>,
        rhs: WordLoHi<Expression<F>>,
    ) {
        let (lhs_lo, lhs_hi) = lhs.to_lo_hi();
        let (rhs_lo, rhs_hi) = rhs.to_lo_hi();
        self.add_constraint(name, lhs_lo - rhs_lo);
        self.add_constraint(name, lhs_hi - rhs_hi);
    }

    fn require_equal(&mut self, name: &'static str, lhs: Expression<F>, rhs: Expression<F>) {
        self.add_constraint(name, lhs - rhs);
    }

    fn require_boolean(&mut self, name: &'static str, value: Expression<F>) {
        self.add_constraint(name, value.clone() * (1.expr() - value));
    }

    fn require_true(&mut self, name: &'static str, value: Expression<F>) {
        self.require_equal(name, value, 1.expr());
    }

    fn require_in_set(
        &mut self,
        name: &'static str,
        value: Expression<F>,
        set: Vec<Expression<F>>,
    ) {
        self.add_constraint(
            name,
            set.iter()
                .fold(1.expr(), |acc, item| acc * (value.clone() - item.clone())),
        );
    }
    /// Under active development
    #[allow(dead_code)]
    fn add_constraints(&mut self, constraints: Vec<(&'static str, Expression<F>)>) {
        for (name, constraint) in constraints {
            self.add_constraint(name, constraint);
        }
    }
}

#[derive(Default)]
pub struct BaseConstraintBuilder<F> {
    pub constraints: Vec<(&'static str, Expression<F>)>,
    pub max_degree: usize,
    pub condition: Option<Expression<F>>,
}

impl<F: Field> ConstrainBuilderCommon<F> for BaseConstraintBuilder<F> {
    fn add_constraint(&mut self, name: &'static str, constraint: Expression<F>) {
        let constraint = match &self.condition {
            Some(condition) => condition.clone() * constraint,
            None => constraint,
        };
        self.validate_degree(constraint.degree(), name);
        self.constraints.push((name, constraint));
    }
}

impl<F: Field> BaseConstraintBuilder<F> {
    pub(crate) fn new(max_degree: usize) -> Self {
        BaseConstraintBuilder {
            constraints: Vec::new(),
            max_degree,
            condition: None,
        }
    }

    pub(crate) fn condition<R>(
        &mut self,
        condition: Expression<F>,
        constraint: impl FnOnce(&mut Self) -> R,
    ) -> R {
        debug_assert!(
            self.condition.is_none(),
            "Nested condition is not supported"
        );
        self.condition = Some(condition);
        let ret = constraint(self);
        self.condition = None;
        ret
    }

    pub(crate) fn validate_degree(&self, degree: usize, name: &'static str) {
        if self.max_degree > 0 {
            debug_assert!(
                degree <= self.max_degree,
                "Expression {} degree too high: {} > {}",
                name,
                degree,
                self.max_degree,
            );
        }
    }

    pub(crate) fn gate(&self, selector: Expression<F>) -> Vec<(&'static str, Expression<F>)> {
        self.constraints
            .clone()
            .into_iter()
            .map(|(name, constraint)| (name, selector.clone() * constraint))
            .filter(|(name, constraint)| {
                self.validate_degree(constraint.degree(), name);
                true
            })
            .collect()
    }
}

/// Internal type to select the location where the constraints are enabled
#[derive(Debug, PartialEq)]
enum ConstraintLocation {
    Step,
    StepFirst,
    NotStepLast,
}

/// Collection of constraints grouped by which selectors will enable them
pub(crate) struct Constraints<F> {
    /// Enabled with q_step
    pub(crate) step: Vec<(&'static str, Expression<F>)>,
    /// Enabled with q_step_first
    pub(crate) step_first: Vec<(&'static str, Expression<F>)>,
    /// Enabled with q_step * q_step_last
    pub(crate) step_last: Vec<(&'static str, Expression<F>)>,
    /// Enabled with q_step * not(q_step_last)
    pub(crate) not_step_last: Vec<(&'static str, Expression<F>)>,
}

pub(crate) struct EVMConstraintBuilder<'a, F: Field> {
    pub(crate) curr: Step<F>,
    pub(crate) next: Step<F>,
    challenges: &'a Challenges<Expression<F>>,
    execution_state: ExecutionState,
    constraints: Constraints<F>,
    rw_counter_offset: Expression<F>,
    program_counter_offset: usize,
    stack_pointer_offset: Expression<F>,
    in_next_step: bool,
    conditions: Vec<Expression<F>>,
    constraints_location: ConstraintLocation,
    stored_expressions: Vec<StoredExpression<F>>,
    pub(crate) debug_expressions: Vec<(String, Expression<F>)>,
    meta: &'a mut ConstraintSystem<F>,
    pub(crate) feature_config: FeatureConfig,
}

impl<'a, F: Field> ConstrainBuilderCommon<F> for EVMConstraintBuilder<'a, F> {
    fn add_constraint(&mut self, name: &'static str, constraint: Expression<F>) {
        let constraint = self.split_expression(
            name,
            constraint * self.condition_expr(),
            MAX_DEGREE - IMPLICIT_DEGREE,
        );

        self.validate_degree(constraint.degree(), name);
        self.push_constraint(name, constraint);
    }
}

pub(crate) type BoxedClosure<'a, F> = Box<dyn FnOnce(&mut EVMConstraintBuilder<F>) + 'a>;

impl<'a, F: Field> EVMConstraintBuilder<'a, F> {
    pub(crate) fn new(
        meta: &'a mut ConstraintSystem<F>,
        curr: Step<F>,
        next: Step<F>,
        challenges: &'a Challenges<Expression<F>>,
        execution_state: ExecutionState,
        feature_config: FeatureConfig,
    ) -> Self {
        Self {
            curr,
            next,
            challenges,
            execution_state,
            constraints: Constraints {
                step: Vec::new(),
                step_first: Vec::new(),
                step_last: Vec::new(),
                not_step_last: Vec::new(),
            },
            rw_counter_offset: 0.expr(),
            program_counter_offset: 0,
            stack_pointer_offset: 0.expr(),
            in_next_step: false,
            conditions: Vec::new(),
            constraints_location: ConstraintLocation::Step,
            stored_expressions: Vec::new(),
            meta,
            debug_expressions: Vec::new(),
            feature_config,
        }
    }

    /// Returns (list of constraints, list of first step constraints, stored
    /// expressions, height used).
    #[allow(clippy::type_complexity)]
    pub(crate) fn build(
        self,
    ) -> (
        Constraints<F>,
        Vec<StoredExpression<F>>,
        usize,
        &'a mut ConstraintSystem<F>,
    ) {
        let exec_state_sel = self.curr.execution_state_selector([self.execution_state]);
        let mul_exec_state_sel = |c: Vec<(&'static str, Expression<F>)>| {
            c.into_iter()
                .map(|(name, constraint)| (name, exec_state_sel.clone() * constraint))
                .collect()
        };
        (
            Constraints {
                step: mul_exec_state_sel(self.constraints.step),
                step_first: mul_exec_state_sel(self.constraints.step_first),
                step_last: mul_exec_state_sel(self.constraints.step_last),
                not_step_last: mul_exec_state_sel(self.constraints.not_step_last),
            },
            self.stored_expressions,
            self.curr.cell_manager.get_height(),
            self.meta,
        )
    }

    pub(crate) fn query_expression<T>(&mut self, f: impl FnMut(&mut VirtualCells<F>) -> T) -> T {
        query_expression(self.meta, f)
    }

    fn condition_expr_opt(&self) -> Option<Expression<F>> {
        let mut iter = self.conditions.iter();
        let first = match iter.next() {
            Some(e) => e,
            None => return None,
        };
        Some(iter.fold(first.clone(), |acc, e| acc * e.clone()))
    }

    pub(crate) fn challenges(&self) -> &Challenges<Expression<F>> {
        self.challenges
    }

    pub(crate) fn execution_state(&self) -> ExecutionState {
        self.execution_state
    }

    pub(crate) fn rw_counter_offset(&self) -> Expression<F> {
        self.rw_counter_offset.clone()
    }

    pub(crate) fn stack_pointer_offset(&self) -> Expression<F> {
        self.stack_pointer_offset.clone()
    }

    // Query

    pub(crate) fn copy<E: Expr<F>>(&mut self, value: E) -> Cell<F> {
        let cell = self.query_cell();
        self.require_equal("Copy value to new cell", cell.expr(), value.expr());
        cell
    }

    pub(crate) fn query_bool(&mut self) -> Cell<F> {
        let cell = self.query_cell();
        self.require_boolean("Constrain cell to be a bool", cell.expr());
        cell
    }

    pub(crate) fn query_byte(&mut self) -> Cell<F> {
        self.query_cell_with_type(CellType::Lookup(Table::U8))
    }

    // default query_word is 2 limbs. Each limb is not guaranteed to be 128 bits.
    pub fn query_word_unchecked(&mut self) -> WordLoHiCell<F> {
        WordLoHi::new(
            self.query_cells(CellType::StoragePhase1, 2)
                .try_into()
                .unwrap(),
        )
    }

    // query_word32 each limb is 8 bits, and any conversion to smaller limbs inherits the type
    // check.
    pub(crate) fn query_word32(&mut self) -> Word32Cell<F> {
        Word32::new(self.query_bytes())
    }

    pub(crate) fn query_keccak_rlc<const N: usize>(&mut self) -> RandomLinearCombination<F, N> {
        RandomLinearCombination::<F, N>::new(self.query_bytes(), self.challenges.keccak_input())
    }

    pub(crate) fn query_u64(&mut self) -> U64Cell<F> {
        U64Cell::new(self.query_bytes())
    }

    pub(crate) fn query_account_address(&mut self) -> AccountAddress<F> {
        AccountAddress::<F>::new(self.query_bytes())
    }

    pub(crate) fn query_memory_address(&mut self) -> MemoryAddress<F> {
        MemoryAddress::<F>::new(self.query_bytes())
    }

    pub(crate) fn query_bytes<const N: usize>(&mut self) -> [Cell<F>; N] {
        self.query_u8_dyn(N).try_into().unwrap()
    }

    pub(crate) fn query_u8_dyn(&mut self, count: usize) -> Vec<Cell<F>> {
        self.query_cells(CellType::Lookup(Table::U8), count)
    }

    pub(crate) fn query_cell(&mut self) -> Cell<F> {
        self.query_cell_with_type(CellType::StoragePhase1)
    }

    pub(crate) fn query_cell_phase2(&mut self) -> Cell<F> {
        self.query_cell_with_type(CellType::StoragePhase2)
    }

    pub(crate) fn query_copy_cell(&mut self) -> Cell<F> {
        self.query_cell_with_type(CellType::StoragePermutation)
    }

    pub(crate) fn query_cell_with_type(&mut self, cell_type: CellType) -> Cell<F> {
        self.query_cells(cell_type, 1).first().unwrap().clone()
    }

    fn query_cells(&mut self, cell_type: CellType, count: usize) -> Vec<Cell<F>> {
        if self.in_next_step {
            &mut self.next
        } else {
            &mut self.curr
        }
        .cell_manager
        .query_cells(self.meta, cell_type, count)
    }

    pub(crate) fn keccak_rlc<const N: usize>(&self, bytes: [Expression<F>; N]) -> Expression<F> {
        rlc::expr(&bytes, self.challenges.keccak_input())
    }

    pub(crate) fn empty_code_hash(&self) -> WordLoHi<Expression<F>> {
        Word32::new(EMPTY_CODE_HASH_LE.map(|byte| byte.expr())).to_word()
    }

    pub(crate) fn require_next_state(&mut self, execution_state: ExecutionState) {
        let next_state = self.next.execution_state_selector([execution_state]);
        self.add_constraint(
            "Constrain next execution state",
            1.expr() - next_state.expr(),
        );
    }

    pub(crate) fn require_step_state_transition(
        &mut self,
        step_state_transition: StepStateTransition<F>,
    ) {
        macro_rules! constrain {
            ($name:tt) => {
                match step_state_transition.$name {
                    Transition::Same => self.require_equal(
                        concat!("State transition (same) constraint of ", stringify!($name)),
                        self.next.state.$name.expr(),
                        self.curr.state.$name.expr(),
                    ),
                    Transition::Delta(delta) => self.require_equal(
                        concat!("State transition (delta) constraint of ", stringify!($name)),
                        self.next.state.$name.expr(),
                        self.curr.state.$name.expr() + delta,
                    ),
                    Transition::To(to) => self.require_equal(
                        concat!("State transition (to) constraint of ", stringify!($name)),
                        self.next.state.$name.expr(),
                        to,
                    ),
                    _ => {}
                }
            };
        }

        macro_rules! constrain_word {
            ($name:tt) => {
                match step_state_transition.$name {
                    Transition::Same => self.require_equal_word(
                        concat!("State transition (same) constraint of ", stringify!($name)),
                        self.next.state.$name.to_word(),
                        self.curr.state.$name.to_word(),
                    ),
                    Transition::To(to) => self.require_equal_word(
                        concat!("State transition (to) constraint of ", stringify!($name)),
                        self.next.state.$name.to_word(),
                        to,
                    ),
                    _ => {}
                }
            };
        }

        constrain!(rw_counter);
        constrain!(call_id);
        constrain!(is_root);
        constrain!(is_create);
        constrain_word!(code_hash);
        constrain!(program_counter);
        constrain!(stack_pointer);
        constrain!(gas_left);
        constrain!(memory_word_size);
        constrain!(reversible_write_counter);
        constrain!(log_id);
    }

    // Math gadgets

    pub(crate) fn is_zero(&mut self, value: Expression<F>) -> IsZeroGadget<F> {
        IsZeroGadget::construct(self, value)
    }

    pub(crate) fn is_zero_word<T: WordExpr<F>>(&mut self, value: &T) -> IsZeroWordGadget<F, T> {
        IsZeroWordGadget::construct(self, value)
    }

    pub(crate) fn is_eq(&mut self, lhs: Expression<F>, rhs: Expression<F>) -> IsEqualGadget<F> {
        IsEqualGadget::construct(self, lhs, rhs)
    }

    pub(crate) fn is_eq_word<T1: WordExpr<F>, T2: WordExpr<F>>(
        &mut self,
        lhs: &T1,
        rhs: &T2,
    ) -> IsEqualWordGadget<F, T1, T2> {
        IsEqualWordGadget::construct(self, lhs, rhs)
    }

    pub(crate) fn is_lt<const N_BYTES: usize>(
        &mut self,
        lhs: Expression<F>,
        rhs: Expression<F>,
    ) -> LtGadget<F, N_BYTES> {
        LtGadget::construct(self, lhs, rhs)
    }

    pub(crate) fn is_lt_word<T: Expr<F> + Clone>(
        &mut self,
        lhs: &WordLoHi<T>,
        rhs: &WordLoHi<T>,
    ) -> LtWordGadget<F> {
        LtWordGadget::construct(self, lhs, rhs)
    }

    pub(crate) fn min_max<const N_BYTES: usize>(
        &mut self,
        lhs: Expression<F>,
        rhs: Expression<F>,
    ) -> MinMaxGadget<F, N_BYTES> {
        MinMaxGadget::construct(self, lhs, rhs)
    }

    pub(crate) fn div_by_const<const N_BYTES: usize>(
        &mut self,
        numerator: Expression<F>,
        denominator: u64,
    ) -> ConstantDivisionGadget<F, N_BYTES> {
        ConstantDivisionGadget::construct(self, numerator, denominator)
    }

    // Common Gadget

    pub(crate) fn increase_balance(
        &mut self,
        address: WordLoHi<Expression<F>>,
        value: Word32Cell<F>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) -> UpdateBalanceGadget<F, 2, true> {
        UpdateBalanceGadget::construct(self, address, &[value], reversion_info)
    }

    pub(crate) fn decrease_balance(
        &mut self,
        address: WordLoHi<Expression<F>>,
        value: Word32Cell<F>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) -> UpdateBalanceGadget<F, 2, false> {
        UpdateBalanceGadget::construct(self, address, &[value], reversion_info)
    }

    // Fixed

    pub(crate) fn range_lookup(&mut self, value: Expression<F>, range: u64) {
        let (name, tag) = match range {
            5 => ("Range5", FixedTableTag::Range5),
            16 => ("Range16", FixedTableTag::Range16),
            32 => ("Range32", FixedTableTag::Range32),
            64 => ("Range64", FixedTableTag::Range64),
            128 => ("Range128", FixedTableTag::Range128),
            256 => ("Range256", FixedTableTag::Range256),
            512 => ("Range512", FixedTableTag::Range512),
            1024 => ("Range1024", FixedTableTag::Range1024),
            _ => unimplemented!(),
        };
        self.add_lookup(
            name,
            Lookup::Fixed {
                tag: tag.expr(),
                values: [value, 0.expr(), 0.expr()],
            },
        );
    }

    // precompiled contract information
    pub(crate) fn precompile_info_lookup(
        &mut self,
        execution_state: Expression<F>,
        address: Expression<F>,
        base_gas_cost: Expression<F>,
    ) {
        self.add_lookup(
            "precompiles info",
            Lookup::Fixed {
                tag: FixedTableTag::PrecompileInfo.expr(),
                values: [execution_state, address, base_gas_cost],
            },
        )
    }

    // constant gas
    pub(crate) fn constant_gas_lookup(&mut self, opcode: Expression<F>, gas: Expression<F>) {
        self.add_lookup(
            "constant gas",
            Lookup::Fixed {
                tag: FixedTableTag::ConstantGasCost.expr(),
                values: [opcode, gas, 0.expr()],
            },
        );
    }

    // Opcode

    pub(crate) fn opcode_lookup(&mut self, opcode: Expression<F>, is_code: Expression<F>) {
        self.opcode_lookup_at(
            self.curr.state.program_counter.expr() + self.program_counter_offset.expr(),
            opcode,
            is_code,
        );
        self.program_counter_offset += 1;
    }

    pub(crate) fn opcode_lookup_at(
        &mut self,
        index: Expression<F>,
        opcode: Expression<F>,
        is_code: Expression<F>,
    ) {
        let is_root_create = self.curr.state.is_root.expr() * self.curr.state.is_create.expr();
        self.add_lookup(
            "Opcode lookup",
            Lookup::Bytecode {
                hash: self.curr.state.code_hash.to_word(),
                tag: BytecodeFieldTag::Byte.expr(),
                index,
                is_code,
                value: opcode,
            }
            .conditional(1.expr() - is_root_create),
        );
    }

    pub(crate) fn bytecode_lookup(
        &mut self,
        code_hash: WordLoHi<Expression<F>>,
        index: Expression<F>,
        is_code: Expression<F>,
        value: Expression<F>,
    ) {
        self.add_lookup(
            "Bytecode (byte) lookup",
            Lookup::Bytecode {
                hash: code_hash,
                tag: BytecodeFieldTag::Byte.expr(),
                index,
                is_code,
                value,
            },
        )
    }

    pub(crate) fn bytecode_length(
        &mut self,
        code_hash: WordLoHi<Expression<F>>,
        value: Expression<F>,
    ) {
        self.add_lookup(
            "Bytecode (length)",
            Lookup::Bytecode {
                hash: code_hash,
                tag: BytecodeFieldTag::Header.expr(),
                index: 0.expr(),
                is_code: 0.expr(),
                value,
            },
        );
    }

    // Tx context

    pub(crate) fn tx_context(
        &mut self,
        id: Expression<F>,
        field_tag: TxContextFieldTag,
        index: Option<Expression<F>>,
    ) -> Cell<F> {
        let cell = self.query_cell();
        // lookup read, unchecked is safe
        self.tx_context_lookup(
            id,
            field_tag,
            index,
            WordLoHi::from_lo_unchecked(cell.expr()),
        );
        cell
    }
    pub(crate) fn tx_context_as_word32(
        &mut self,
        id: Expression<F>,
        field_tag: TxContextFieldTag,
        index: Option<Expression<F>>,
    ) -> Word32Cell<F> {
        let word = self.query_word32();
        self.tx_context_lookup(id, field_tag, index, word.to_word());
        word
    }

    pub(crate) fn tx_context_as_word(
        &mut self,
        id: Expression<F>,
        field_tag: TxContextFieldTag,
        index: Option<Expression<F>>,
    ) -> WordLoHiCell<F> {
        let word = self.query_word_unchecked();
        self.tx_context_lookup(id, field_tag, index, word.to_word());
        word
    }

    pub(crate) fn tx_context_lookup(
        &mut self,
        id: Expression<F>,
        field_tag: TxContextFieldTag,
        index: Option<Expression<F>>,
        value: WordLoHi<Expression<F>>,
    ) {
        self.add_lookup(
            "Tx lookup",
            Lookup::Tx {
                id,
                field_tag: field_tag.expr(),
                index: index.unwrap_or_else(|| 0.expr()),
                value,
            },
        );
    }

    // block
    pub(crate) fn block_lookup(
        &mut self,
        tag: Expression<F>,
        number: Option<Expression<F>>,
        val: WordLoHi<Expression<F>>,
    ) {
        self.add_lookup(
            "Block lookup",
            Lookup::Block {
                field_tag: tag,
                number: number.unwrap_or_else(|| 0.expr()),
                value: val,
            },
        );
    }

    // Rw

    /// Add a Lookup::Rw without increasing the rw_counter_offset, which is
    /// useful for state reversion or dummy lookup.
    fn rw_lookup_with_counter(
        &mut self,
        name: &str,
        counter: Expression<F>,
        is_write: Expression<F>,
        tag: Target,
        values: RwValues<F>,
    ) {
        let name = format!("rw lookup {}", name);
        self.add_lookup(
            &name,
            Lookup::Rw {
                counter,
                is_write,
                tag: tag.expr(),
                values,
            },
        );
    }

    /// Add a Lookup::Rw and increase the rw_counter_offset, useful in normal
    /// cases.
    fn rw_lookup(
        &mut self,
        name: &'static str,
        is_write: Expression<F>,
        tag: Target,
        values: RwValues<F>,
    ) {
        self.rw_lookup_with_counter(
            name,
            self.curr.state.rw_counter.expr() + self.rw_counter_offset.clone(),
            is_write,
            tag,
            values,
        );
        // Manually constant folding is used here, since halo2 cannot do this
        // automatically. Better error message will be printed during circuit
        // debugging.
        self.rw_counter_offset = match self.condition_expr_opt() {
            None => {
                if let Constant(v) = self.rw_counter_offset {
                    Constant(v + F::from(1u64))
                } else {
                    self.rw_counter_offset.clone() + 1i32.expr()
                }
            }
            Some(c) => self.rw_counter_offset.clone() + c,
        };
    }

    fn reversible_write(
        &mut self,
        name: &'static str,
        tag: Target,
        values: RwValues<F>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) {
        debug_assert!(
            tag.is_reversible(),
            "Reversible write requires reversible tag"
        );

        self.rw_lookup(name, true.expr(), tag, values.clone());

        // Revert if is_persistent is 0
        if let Some(reversion_info) = reversion_info {
            let reversible_write_counter_inc_selector = self.condition_expr();
            self.condition(not::expr(reversion_info.is_persistent()), |cb| {
                let name = format!("{} with reversion", name);
                cb.rw_lookup_with_counter(
                    &name,
                    reversion_info.rw_counter_of_reversion(reversible_write_counter_inc_selector),
                    true.expr(),
                    tag,
                    values.revert_value(),
                )
            });
        }
    }

    // Access list
    pub(crate) fn account_access_list_write_unchecked(
        &mut self,
        tx_id: Expression<F>,
        account_address: WordLoHi<Expression<F>>,
        value: Expression<F>,
        value_prev: Expression<F>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) {
        self.reversible_write(
            "TxAccessListAccount write",
            Target::TxAccessListAccount,
            RwValues::new(
                tx_id,
                account_address.compress(),
                0.expr(),
                WordLoHi::zero(),
                WordLoHi::from_lo_unchecked(value),
                WordLoHi::from_lo_unchecked(value_prev),
                WordLoHi::zero(),
            ),
            reversion_info,
        );
    }

    pub(crate) fn account_access_list_read(
        &mut self,
        tx_id: Expression<F>,
        account_address: WordLoHi<Expression<F>>,
        value: Expression<F>,
    ) {
        self.rw_lookup(
            "account access list read",
            false.expr(),
            Target::TxAccessListAccount,
            RwValues::new(
                tx_id,
                account_address.compress(),
                0.expr(),
                WordLoHi::zero(),
                WordLoHi::from_lo_unchecked(value.clone()),
                WordLoHi::from_lo_unchecked(value),
                WordLoHi::zero(),
            ),
        );
    }
    pub(crate) fn account_storage_access_list_write(
        &mut self,
        tx_id: Expression<F>,
        account_address: WordLoHi<Expression<F>>,
        storage_key: WordLoHi<Expression<F>>,
        value: WordLoHi<Expression<F>>,
        value_prev: WordLoHi<Expression<F>>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) {
        self.reversible_write(
            "TxAccessListAccountStorage write",
            Target::TxAccessListAccountStorage,
            RwValues::new(
                tx_id,
                account_address.compress(),
                0.expr(),
                storage_key,
                value,
                value_prev,
                WordLoHi::zero(),
            ),
            reversion_info,
        );
    }

    pub(crate) fn account_storage_access_list_read(
        &mut self,
        tx_id: Expression<F>,
        account_address: WordLoHi<Expression<F>>,
        storage_key: WordLoHi<Expression<F>>,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "TxAccessListAccountStorage read",
            false.expr(),
            Target::TxAccessListAccountStorage,
            RwValues::new(
                tx_id,
                account_address.compress(),
                0.expr(),
                storage_key,
                value.clone(),
                value,
                WordLoHi::zero(),
            ),
        );
    }

    // Tx Refund

    pub(crate) fn tx_refund_read(&mut self, tx_id: Expression<F>, value: WordLoHi<Expression<F>>) {
        self.rw_lookup(
            "TxRefund read",
            false.expr(),
            Target::TxRefund,
            RwValues::new(
                tx_id,
                0.expr(),
                0.expr(),
                WordLoHi::zero(),
                value.clone(),
                value,
                WordLoHi::zero(),
            ),
        );
    }

    pub(crate) fn tx_refund_write(
        &mut self,
        tx_id: Expression<F>,
        value: WordLoHi<Expression<F>>,
        value_prev: WordLoHi<Expression<F>>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) {
        self.reversible_write(
            "TxRefund write",
            Target::TxRefund,
            RwValues::new(
                tx_id,
                0.expr(),
                0.expr(),
                WordLoHi::zero(),
                value,
                value_prev,
                WordLoHi::zero(),
            ),
            reversion_info,
        );
    }

    // Account
    pub(crate) fn account_read(
        &mut self,
        account_address: WordLoHi<Expression<F>>,
        field_tag: AccountFieldTag,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "Account read",
            false.expr(),
            Target::Account,
            RwValues::new(
                0.expr(),
                account_address.compress(),
                field_tag.expr(),
                WordLoHi::zero(),
                value.clone(),
                value,
                WordLoHi::zero(),
            ),
        );
    }

    pub(crate) fn account_write(
        &mut self,
        account_address: WordLoHi<Expression<F>>,
        field_tag: AccountFieldTag,
        value: WordLoHi<Expression<F>>,
        value_prev: WordLoHi<Expression<F>>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) {
        self.reversible_write(
            "Account write",
            Target::Account,
            RwValues::new(
                0.expr(),
                account_address.compress(),
                field_tag.expr(),
                WordLoHi::zero(),
                value,
                value_prev,
                WordLoHi::zero(),
            ),
            reversion_info,
        );
    }

    // Account Storage
    pub(crate) fn account_storage_read(
        &mut self,
        account_address: WordLoHi<Expression<F>>,
        key: WordLoHi<Expression<F>>,
        value: WordLoHi<Expression<F>>,
        tx_id: Expression<F>,
        committed_value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "account_storage_read",
            false.expr(),
            Target::Storage,
            RwValues::new(
                tx_id,
                account_address.compress(),
                0.expr(),
                key,
                value.clone(),
                value,
                committed_value,
            ),
        );
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn account_storage_write(
        &mut self,
        account_address: WordLoHi<Expression<F>>,
        key: WordLoHi<Expression<F>>,
        value: WordLoHi<Expression<F>>,
        value_prev: WordLoHi<Expression<F>>,
        tx_id: Expression<F>,
        committed_value: WordLoHi<Expression<F>>,
        reversion_info: Option<&mut ReversionInfo<F>>,
    ) {
        self.reversible_write(
            "AccountStorage write",
            Target::Storage,
            RwValues::new(
                tx_id,
                account_address.compress(),
                0.expr(),
                key,
                value,
                value_prev,
                committed_value,
            ),
            reversion_info,
        );
    }

    // Call context
    pub(crate) fn call_context(
        &mut self,
        call_id: Option<Expression<F>>,
        field_tag: CallContextFieldTag,
    ) -> Cell<F> {
        let phase = match field_tag {
            CallContextFieldTag::CodeHash => CellType::StoragePhase2,
            _ => CellType::StoragePhase1,
        };
        let cell = self.query_cell_with_type(phase);
        self.call_context_lookup_read(
            call_id,
            field_tag,
            WordLoHi::from_lo_unchecked(cell.expr()), // lookup read, unchecked is safe
        );
        cell
    }

    pub(crate) fn call_context_read_as_word(
        &mut self,
        call_id: Option<Expression<F>>,
        field_tag: CallContextFieldTag,
    ) -> WordLoHi<Cell<F>> {
        let word = self.query_word_unchecked();
        self.call_context_lookup_read(call_id, field_tag, word.to_word());
        word
    }

    pub(crate) fn call_context_lookup_read(
        &mut self,
        call_id: Option<Expression<F>>,
        field_tag: CallContextFieldTag,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "CallContext lookup",
            0.expr(),
            Target::CallContext,
            RwValues::new(
                call_id.unwrap_or_else(|| self.curr.state.call_id.expr()),
                0.expr(),
                field_tag.expr(),
                WordLoHi::zero(),
                value,
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    // same as call_context_lookup_write with bypassing external rwc
    // Note: will not bumping internal rwc
    pub(crate) fn call_context_lookup_write_with_counter(
        &mut self,
        rw_counter: Expression<F>,
        call_id: Option<Expression<F>>,
        field_tag: CallContextFieldTag,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup_with_counter(
            "CallContext lookup",
            rw_counter,
            1.expr(),
            Target::CallContext,
            RwValues::new(
                call_id.unwrap_or_else(|| self.curr.state.call_id.expr()),
                0.expr(),
                field_tag.expr(),
                WordLoHi::zero(),
                value,
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    pub(crate) fn call_context_lookup_write(
        &mut self,
        call_id: Option<Expression<F>>,
        field_tag: CallContextFieldTag,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "CallContext lookup",
            1.expr(),
            Target::CallContext,
            RwValues::new(
                call_id.unwrap_or_else(|| self.curr.state.call_id.expr()),
                0.expr(),
                field_tag.expr(),
                WordLoHi::zero(),
                value,
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    fn reversion_info(
        &mut self,
        call_id: Option<Expression<F>>,
        is_write: bool,
    ) -> ReversionInfo<F> {
        let [rw_counter_end_of_reversion, is_persistent] = [
            CallContextFieldTag::RwCounterEndOfReversion,
            CallContextFieldTag::IsPersistent,
        ]
        .map(|field_tag| {
            let cell = self.query_cell();
            if is_write {
                self.call_context_lookup_write(
                    call_id.clone(),
                    field_tag,
                    WordLoHi::from_lo_unchecked(cell.expr()),
                );
            } else {
                self.call_context_lookup_read(
                    call_id.clone(),
                    field_tag,
                    WordLoHi::from_lo_unchecked(cell.expr()),
                );
            }

            cell
        });

        ReversionInfo {
            rw_counter_end_of_reversion,
            is_persistent,
            reversible_write_counter: if call_id.is_some() {
                0.expr()
            } else {
                self.curr.state.reversible_write_counter.expr()
            },
        }
    }

    pub(crate) fn reversion_info_read(
        &mut self,
        call_id: Option<Expression<F>>,
    ) -> ReversionInfo<F> {
        self.reversion_info(call_id, false)
    }

    pub(crate) fn reversion_info_write_unchecked(
        &mut self,
        call_id: Option<Expression<F>>,
    ) -> ReversionInfo<F> {
        self.reversion_info(call_id, true)
    }

    // Stack
    pub(crate) fn stack_pop(&mut self, value: WordLoHi<Expression<F>>) {
        self.stack_lookup(false.expr(), self.stack_pointer_offset.clone(), value);
        self.stack_pointer_offset = self.stack_pointer_offset.clone() + self.condition_expr();
    }

    pub(crate) fn stack_push(&mut self, value: WordLoHi<Expression<F>>) {
        self.stack_pointer_offset = self.stack_pointer_offset.clone() - self.condition_expr();
        self.stack_lookup(true.expr(), self.stack_pointer_offset.expr(), value);
    }

    pub(crate) fn stack_lookup(
        &mut self,
        is_write: Expression<F>,
        stack_pointer_offset: Expression<F>,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "Stack lookup",
            is_write,
            Target::Stack,
            RwValues::new(
                self.curr.state.call_id.expr(),
                self.curr.state.stack_pointer.expr() + stack_pointer_offset,
                0.expr(),
                WordLoHi::zero(),
                value,
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    // Memory

    pub(crate) fn memory_lookup(
        &mut self,
        is_write: Expression<F>,
        memory_address: Expression<F>,
        byte: Expression<F>,
        call_id: Option<Expression<F>>,
    ) {
        self.rw_lookup(
            "Memory lookup",
            is_write,
            Target::Memory,
            RwValues::new(
                call_id.unwrap_or_else(|| self.curr.state.call_id.expr()),
                memory_address,
                0.expr(),
                WordLoHi::zero(),
                // TODO assure range check since write=true also possible
                WordLoHi::from_lo_unchecked(byte),
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    pub(crate) fn tx_log_lookup(
        &mut self,
        tx_id: Expression<F>,
        log_id: Expression<F>,
        field_tag: TxLogFieldTag,
        index: Expression<F>,
        value: WordLoHi<Expression<F>>,
    ) {
        self.rw_lookup(
            "log data lookup",
            1.expr(),
            Target::TxLog,
            RwValues::new(
                tx_id,
                build_tx_log_expression(index, field_tag.expr(), log_id),
                0.expr(),
                WordLoHi::zero(),
                value,
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    // Tx Receipt
    pub(crate) fn tx_receipt_lookup(
        &mut self,
        is_write: Expression<F>,
        tx_id: Expression<F>,
        tag: TxReceiptFieldTag,
        value: Expression<F>,
    ) {
        self.rw_lookup(
            "tx receipt lookup",
            is_write,
            Target::TxReceipt,
            RwValues::new(
                tx_id,
                0.expr(),
                tag.expr(),
                WordLoHi::zero(),
                // TODO assure range check since write=true also possible
                WordLoHi::from_lo_unchecked(value),
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    // RwTable Padding (Start tag)

    pub(crate) fn rw_table_start_lookup(&mut self, counter: Expression<F>) {
        self.rw_lookup_with_counter(
            "Start lookup",
            counter,
            0.expr(),
            Target::Start,
            RwValues::new(
                0.expr(),
                0.expr(),
                0.expr(),
                WordLoHi::zero(),
                WordLoHi::zero(),
                WordLoHi::zero(),
                WordLoHi::zero(),
            ),
        );
    }

    // Copy Table

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn copy_table_lookup(
        &mut self,
        src_id: WordLoHi<Expression<F>>,
        src_tag: Expression<F>,
        dst_id: WordLoHi<Expression<F>>,
        dst_tag: Expression<F>,
        src_addr: Expression<F>,
        src_addr_end: Expression<F>,
        dst_addr: Expression<F>,
        length: Expression<F>,
        rlc_acc: Expression<F>,
        rwc_inc: Expression<F>,
    ) {
        self.add_lookup(
            "copy lookup",
            Lookup::CopyTable {
                is_first: 1.expr(), // is_first
                src_id,
                src_tag,
                dst_id,
                dst_tag,
                src_addr,
                src_addr_end,
                dst_addr,
                length,
                rlc_acc,
                rw_counter: self.curr.state.rw_counter.expr() + self.rw_counter_offset(),
                rwc_inc: rwc_inc.clone(),
            },
        );
        self.rw_counter_offset = self.rw_counter_offset.clone() + self.condition_expr() * rwc_inc;
    }

    // Exponentiation Table

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn exp_table_lookup(
        &mut self,
        identifier: Expression<F>,
        is_last: Expression<F>,
        base_limbs: [Expression<F>; 4],
        exponent_lo_hi: [Expression<F>; 2],
        exponentiation_lo_hi: [Expression<F>; 2],
    ) {
        self.add_lookup(
            "exponentiation lookup",
            Lookup::ExpTable {
                identifier,
                is_last,
                base_limbs,
                exponent_lo_hi,
                exponentiation_lo_hi,
            },
        );
    }

    /// Sig Table
    pub(crate) fn sig_table_lookup(
        &mut self,
        msg_hash: WordLoHi<Expression<F>>,
        sig_v: Expression<F>,
        sig_r: WordLoHi<Expression<F>>,
        sig_s: WordLoHi<Expression<F>>,
        recovered_addr: Expression<F>,
        is_valid: Expression<F>,
    ) {
        self.add_lookup(
            "sig table",
            Lookup::SigTable {
                msg_hash,
                sig_v,
                sig_r,
                sig_s,
                recovered_addr,
                is_valid,
            },
        );
    }

    /// Keccak Table
    pub(crate) fn keccak_table_lookup(
        &mut self,
        input_rlc: Expression<F>,
        input_len: Expression<F>,
        output: WordLoHi<Expression<F>>,
    ) {
        self.add_lookup(
            "keccak lookup",
            Lookup::KeccakTable {
                input_rlc,
                input_len,
                output,
            },
        );
    }

    // Validation

    pub(crate) fn validate_degree(&self, degree: usize, name: &'static str) {
        // We need to subtract IMPLICIT_DEGREE from MAX_DEGREE because all expressions
        // will be multiplied by state selector and q_step/q_step_first
        // selector.
        debug_assert!(
            degree <= MAX_DEGREE - IMPLICIT_DEGREE,
            "Expression {} degree too high: {} > {}",
            name,
            degree,
            MAX_DEGREE - IMPLICIT_DEGREE,
        );
    }

    // General

    pub(crate) fn condition<R>(
        &mut self,
        condition: Expression<F>,
        constraint: impl FnOnce(&mut Self) -> R,
    ) -> R {
        self.conditions.push(condition);
        let ret = constraint(self);
        self.conditions.pop();
        ret
    }

    /// Constrain the next step, given mutually exclusive conditions to determine the next state
    /// and constrain it using the provided respective constraint. This mechanism is specifically
    /// used for constraining the internal states for precompile calls. Each precompile call
    /// expects a different cell layout, but since the next state can be at the most one precompile
    /// state, we can re-use cells assigned across all those conditions.
    pub(crate) fn constrain_mutually_exclusive_next_step(
        &mut self,
        conditions: Vec<Expression<F>>,
        next_states: Vec<ExecutionState>,
        constraints: Vec<BoxedClosure<F>>,
    ) {
        assert_eq!(conditions.len(), constraints.len());
        assert_eq!(conditions.len(), next_states.len());

        self.require_boolean(
            "at the most one condition is true from mutually exclusive conditions",
            sum::expr(&conditions),
        );

        // TODO: constraining the same cells repeatedly requires a height-resetting mechanism
        // on the cell manager. In this case, since only identity precompile is added
        // this height management maneuver is temporarily left out.
        for ((&next_state, condition), constraint) in next_states
            .iter()
            .zip(conditions.into_iter())
            .zip(constraints.into_iter())
        {
            // constrain the next step.
            self.constrain_next_step(next_state, Some(condition), constraint);
        }
    }

    /// This function needs to be used with extra precaution. You need to make
    /// sure the layout is the same as the gadget for `next_step_state`.
    /// `query_cell` will return cells in the next step in the `constraint`
    /// function.
    pub(crate) fn constrain_next_step<R>(
        &mut self,
        next_step_state: ExecutionState,
        condition: Option<Expression<F>>,
        constraint: impl FnOnce(&mut Self) -> R,
    ) -> R {
        assert!(!self.in_next_step, "Already in the next step");
        self.in_next_step = true;
        let ret = match condition {
            None => {
                self.require_next_state(next_step_state);
                constraint(self)
            }
            Some(cond) => self.condition(cond, |cb| {
                cb.require_next_state(next_step_state);
                constraint(cb)
            }),
        };
        self.in_next_step = false;
        ret
    }

    /// TODO: Doc
    fn constraint_at_location<R>(
        &mut self,
        location: ConstraintLocation,
        constraint: impl FnOnce(&mut Self) -> R,
    ) -> R {
        debug_assert_eq!(
            self.constraints_location,
            ConstraintLocation::Step,
            "ConstraintLocation can't be combined"
        );
        self.constraints_location = location;
        let ret = constraint(self);
        self.constraints_location = ConstraintLocation::Step;
        ret
    }

    /// register constraints to be applied `step_first` selector
    pub(crate) fn step_first<R>(&mut self, constraint: impl FnOnce(&mut Self) -> R) -> R {
        self.constraint_at_location(ConstraintLocation::StepFirst, constraint)
    }

    /// register constraints to be applied on step other than first step
    pub(crate) fn not_step_last<R>(&mut self, constraint: impl FnOnce(&mut Self) -> R) -> R {
        self.constraint_at_location(ConstraintLocation::NotStepLast, constraint)
    }

    /// register constraints to be applied on respective selector later
    fn push_constraint(&mut self, name: &'static str, constraint: Expression<F>) {
        match self.constraints_location {
            ConstraintLocation::Step => self.constraints.step.push((name, constraint)),
            ConstraintLocation::StepFirst => self.constraints.step_first.push((name, constraint)),
            ConstraintLocation::NotStepLast => {
                self.constraints.not_step_last.push((name, constraint))
            }
        }
    }

    pub(crate) fn add_lookup(&mut self, name: &str, lookup: Lookup<F>) {
        debug_assert_eq!(
            self.constraints_location,
            ConstraintLocation::Step,
            "lookup do not support conditional on constraint location other than `ConstraintLocation::Step`"
        );
        let lookup = match self.condition_expr_opt() {
            Some(condition) => lookup.conditional(condition),
            None => lookup,
        };
        let compressed_expr = self.split_expression(
            "Lookup compression",
            rlc::expr(&lookup.input_exprs(), self.challenges.lookup_input()),
            MAX_DEGREE - IMPLICIT_DEGREE,
        );
        self.store_expression(name, compressed_expr, CellType::Lookup(lookup.table()));
    }

    pub(crate) fn store_expression(
        &mut self,
        name: &str,
        expr: Expression<F>,
        cell_type: CellType,
    ) -> Expression<F> {
        // Check if we already stored the expression somewhere
        let stored_expression = self.find_stored_expression(&expr, cell_type);

        match stored_expression {
            Some(stored_expression) => {
                debug_assert!(
                    !matches!(cell_type, CellType::Lookup(_)),
                    "The same lookup is done multiple times",
                );
                stored_expression.cell.expr()
            }
            None => {
                // Even if we're building expressions for the next step,
                // these intermediate values need to be stored in the current step.
                let in_next_step = self.in_next_step;
                self.in_next_step = false;
                let cell = self.query_cell_with_type(cell_type);
                self.in_next_step = in_next_step;

                // Require the stored value to equal the value of the expression
                let name = format!("{} (stored expression)", name);
                self.push_constraint(
                    Box::leak(name.clone().into_boxed_str()),
                    cell.expr() - expr.clone(),
                );

                self.stored_expressions.push(StoredExpression {
                    name,
                    cell: cell.clone(),
                    cell_type,
                    expr_id: expr.identifier(),
                    expr,
                });
                cell.expr()
            }
        }
    }

    pub(crate) fn find_stored_expression(
        &self,
        expr: &Expression<F>,
        cell_type: CellType,
    ) -> Option<&StoredExpression<F>> {
        let expr_id = expr.identifier();
        self.stored_expressions
            .iter()
            .find(|&e| e.cell_type == cell_type && e.expr_id == expr_id)
    }

    fn split_expression(
        &mut self,
        name: &'static str,
        expr: Expression<F>,
        max_degree: usize,
    ) -> Expression<F> {
        if expr.degree() > max_degree {
            match expr {
                Expression::Negated(poly) => {
                    Expression::Negated(Box::new(self.split_expression(name, *poly, max_degree)))
                }
                Expression::Scaled(poly, v) => {
                    Expression::Scaled(Box::new(self.split_expression(name, *poly, max_degree)), v)
                }
                Expression::Sum(a, b) => {
                    let a = self.split_expression(name, *a, max_degree);
                    let b = self.split_expression(name, *b, max_degree);
                    a + b
                }
                Expression::Product(a, b) => {
                    let (mut a, mut b) = (*a, *b);
                    while a.degree() + b.degree() > max_degree {
                        let mut split = |expr: Expression<F>| {
                            if expr.degree() > max_degree {
                                self.split_expression(name, expr, max_degree)
                            } else {
                                let cell_type = CellType::storage_for_expr(&expr);
                                self.store_expression(name, expr, cell_type)
                            }
                        };
                        if a.degree() >= b.degree() {
                            a = split(a);
                        } else {
                            b = split(b);
                        }
                    }
                    a * b
                }
                _ => expr.clone(),
            }
        } else {
            expr.clone()
        }
    }

    fn condition_expr(&self) -> Expression<F> {
        match self.condition_expr_opt() {
            Some(condition) => condition,
            None => 1.expr(),
        }
    }

    pub fn debug_expression<S: Into<String>>(&mut self, name: S, expr: Expression<F>) {
        self.debug_expressions.push((name.into(), expr));
    }
}
