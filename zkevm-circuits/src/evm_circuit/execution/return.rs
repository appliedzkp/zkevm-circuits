use crate::evm_circuit::util::memory_gadget::MemoryAddressGadget;
use crate::{
    evm_circuit::{
        execution::ExecutionGadget,
        param::N_BYTES_MEMORY_ADDRESS,
        step::ExecutionState,
        util::{
            common_gadget::RestoreContextGadget, constraint_builder::ConstraintBuilder,
            math_gadget::MinMaxGadget, not, CachedRegion, Cell,
        },
        witness::{Block, Call, ExecStep, Transaction},
    },
    table::CallContextFieldTag,
    util::Expr,
};
use bus_mapping::{circuit_input_builder::CopyDataType, evm::OpcodeId};
use eth_types::{Field, ToScalar, Word};
use halo2_proofs::{circuit::Value, plonk::Error};

#[derive(Clone, Debug)]
pub(crate) struct ReturnGadget<F> {
    opcode: Cell<F>,

    range: MemoryAddressGadget<F>,

    is_root: Cell<F>,
    is_create: Cell<F>,
    is_success: Cell<F>,
    restore_context: RestoreContextGadget<F>,

    nonroot_copy_length: MinMaxGadget<F, N_BYTES_MEMORY_ADDRESS>,

    caller_id: Cell<F>, // can you get this out of restore_context?
    return_data_offset: Cell<F>,
    return_data_length: Cell<F>,
}

// TODO: rename this is reflect the fact that is handles REVERT as well.
impl<F: Field> ExecutionGadget<F> for ReturnGadget<F> {
    const NAME: &'static str = "RETURN";

    const EXECUTION_STATE: ExecutionState = ExecutionState::RETURN;

    fn configure(cb: &mut ConstraintBuilder<F>) -> Self {
        let opcode = cb.query_cell();
        cb.opcode_lookup(opcode.expr(), 1.expr());

        let offset = cb.query_cell();
        let length = cb.query_rlc();
        cb.stack_pop(offset.expr()); // how is this passing?????
        cb.stack_pop(length.expr());
        let range = MemoryAddressGadget::construct(cb, offset, length);

        let [is_root, is_create, is_success, caller_id, return_data_offset, return_data_length] = [
            CallContextFieldTag::IsRoot,
            CallContextFieldTag::IsCreate,
            CallContextFieldTag::IsSuccess,
            CallContextFieldTag::CallerId,
            CallContextFieldTag::ReturnDataOffset,
            CallContextFieldTag::ReturnDataLength,
        ]
        .map(|field_tag| cb.call_context(None, field_tag));

        cb.require_equal(
            "if is_success, opcode is RETURN. if not, opcode is REVERT",
            opcode.expr(),
            is_success.expr() * OpcodeId::RETURN.expr()
                + not::expr(is_success.expr()) * OpcodeId::REVERT.expr(),
        );

        cb.condition(is_root.expr(), |cb| {
            cb.require_next_state(ExecutionState::EndTx);
        });

        let nonroot_copy_length =
            MinMaxGadget::construct(cb, return_data_length.expr(), range.length());
        let copy_length = not::expr(is_root.expr()) * nonroot_copy_length.min();

        let restore_context = cb.condition(not::expr(is_root.expr()), |cb| {
            cb.require_next_state_not(ExecutionState::EndTx);
            RestoreContextGadget::construct(
                cb,
                is_success.expr(),
                copy_length.clone() + copy_length.clone(),
                range.offset(),
                range.length(),
            )
        });

        // cb.condition(
        //     not::expr(is_create.expr()) * not::expr(is_root.expr()) *
        // range.has_length(),     |cb| {
        //         let source_id = cb.curr.state.call_id.expr();
        //         let source_tag = CopyDataType::Memory.expr();
        //         let destination_id = caller_id.expr();
        //         let destination_tag = CopyDataType::Memory.expr();
        //         let source_address_start = range.offset();
        //         let source_address_end = range.offset() + copy_length.clone();
        //         let destination_address_start = return_data_offset.expr();
        //         let rlc_acc = 0.expr();
        //         let rw_counter_start =
        //             cb.curr.state.rw_counter.expr() + cb.rw_counter_offset().expr();
        //         cb.copy_table_lookup(
        //             source_id,
        //             source_tag,
        //             destination_id,
        //             destination_tag,
        //             source_address_start,
        //             source_address_end,
        //             destination_address_start,
        //             copy_length.clone(),
        //             rlc_acc,
        //             rw_counter_start,
        //             copy_length.clone() + copy_length,
        //         );
        //     },
        // );

        cb.condition(
            is_create.expr() * is_success.expr() * range.has_length(),
            |_cb| {
                // TODO: copy_table_lookup for contract creation
            },
        );

        Self {
            opcode,
            range,
            is_root,
            is_create,
            is_success,
            caller_id,
            nonroot_copy_length,
            return_data_offset,
            return_data_length,
            restore_context,
        }
    }

    fn assign_exec_step(
        &self,
        region: &mut CachedRegion<'_, '_, F>,
        offset: usize,
        block: &Block<F>,
        _: &Transaction,
        call: &Call,
        step: &ExecStep,
    ) -> Result<(), Error> {
        self.opcode.assign(
            region,
            offset,
            Value::known(F::from(step.opcode.unwrap().as_u64())),
        )?;

        let [mut memory_offset, length] =
            [0, 1].map(|i| block.rws[step.rw_indices[i]].stack_value());
        dbg!(memory_offset, length);
        // if length.is_zero() {
        //     memory_offset = Word::zero();
        // }

        // there might be an issue if this is constructed with length = 0, but it does
        // have a have_length, method, so that seems unlikely.
        self.range
            .assign(region, offset, memory_offset, length, block.randomness)?;

        for (cell, value) in [
            (&self.is_root, call.is_root),
            (&self.is_create, call.is_create),
            (&self.is_success, call.is_success),
        ] {
            cell.assign(region, offset, Value::known(value.to_scalar().unwrap()))?;
        }

        for (cell, value) in [
            (
                &self.caller_id,
                F::from(u64::try_from(call.caller_id).unwrap()),
            ),
            (&self.return_data_length, call.return_data_length.into()),
            (&self.return_data_offset, call.return_data_offset.into()),
        ] {
            cell.assign(region, offset, Value::known(value))?;
        }

        if !call.is_root {
            self.restore_context
                .assign(region, offset, block, call, step, 8)?;
        }

        self.nonroot_copy_length.assign(
            region,
            offset,
            F::from(call.return_data_length),
            F::from(length.as_u64()),
        )?;
        let opcode = step.opcode.unwrap();
        self.opcode
            .assign(region, offset, Value::known(F::from(opcode.as_u64())))?;

        Ok(())
    }
}

#[cfg(test)]
mod test {
    use crate::test_util::run_test_circuits;
    use eth_types::{
        address, bytecode, evm_types::OpcodeId, geth_types::Account, Address, Bytecode, ToWord,
        Word,
    };
    use itertools::Itertools;
    use mock::TestContext;

    const CALLEE_ADDRESS: Address = Address::repeat_byte(0xff);
    const CALLER_ADDRESS: Address = Address::repeat_byte(0x34);

    fn callee_bytecode(is_return: bool, offset: u64, length: u64) -> Bytecode {
        let memory_address = 2;
        let memory_value = Word::MAX;
        let mut code = bytecode! {
            PUSH32(memory_value)
            PUSH1(memory_address)
            MSTORE
            PUSH32(length)
            PUSH32(offset)
        };
        code.write_op(if is_return {
            OpcodeId::RETURN
        } else {
            OpcodeId::REVERT
        });
        code
    }

    fn caller_bytecode(return_data_offset: u64, return_data_length: u64) -> Bytecode {
        dbg!(return_data_offset, return_data_length);
        bytecode! {
            PUSH32(return_data_length)
            PUSH32(return_data_offset)
            PUSH32(0) // call data length
            PUSH32(0) // call data offset
            PUSH32(0) // value
            PUSH32(CALLEE_ADDRESS.to_word())
            PUSH32(4000) // gas
            CALL
            STOP
        }
    }

    #[test]
    fn test_root() {
        let offset = 0;
        let length = 10;
        for is_return in [true, false] {
            let code = callee_bytecode(is_return, offset, length);
            assert_eq!(
                run_test_circuits(
                    TestContext::<2, 1>::simple_ctx_with_bytecode(code).unwrap(),
                    None
                ),
                Ok(()),
                "{}",
                dbg!(is_return)
            );
        }
    }

    #[test]
    fn test_return_nonroot() {
        let test_parameters = [
            ((0, 0), (0, 0)),
            ((0, 10), (0, 10)),
            ((0, 10), (0, 20)),
            ((0, 20), (0, 10)),
            ((0, 10), (20, 10)),
            ((0, 10), (1000, 0)),
            ((1000, 0), (0, 10)),
            ((1000, 0), (1000, 0)),
        ];
        for (((callee_offset, callee_length), (caller_offset, caller_length)), is_return) in
            test_parameters.iter().cartesian_product(&[true, false])
        {
            let callee = Account {
                address: CALLEE_ADDRESS,
                code: callee_bytecode(*is_return, *callee_offset, *callee_length).into(),
                nonce: Word::one(),
                ..Default::default()
            };
            let caller = Account {
                address: CALLER_ADDRESS,
                code: caller_bytecode(*caller_offset, *caller_length).into(),
                nonce: Word::one(),
                ..Default::default()
            };

            let test_context = TestContext::<3, 1>::new(
                None,
                |accs| {
                    accs[0]
                        .address(address!("0x000000000000000000000000000000000000cafe"))
                        .balance(Word::from(10u64.pow(19)));
                    accs[1].account(&caller);
                    accs[2].account(&callee);
                },
                |mut txs, accs| {
                    txs[0]
                        .from(accs[0].address)
                        .to(accs[1].address)
                        .gas(100000u64.into());
                },
                |block, _tx| block.number(0xcafeu64),
            )
            .unwrap();

            assert_eq!(
                run_test_circuits(test_context, None),
                Ok(()),
                "(callee_offset, callee_length, caller_offset, caller_length, is_return) = {:?}",
                (
                    *callee_offset,
                    *callee_length,
                    *caller_offset,
                    *caller_length,
                    *is_return
                )
            );
        }
    }
}
