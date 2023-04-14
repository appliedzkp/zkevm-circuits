//! The Super Circuit is a circuit that contains all the circuits of the
//! zkEVM in order to achieve two things:
//! - Check the correct integration between circuits via the shared lookup tables, to verify that
//!   the table layouts match.
//! - Allow having a single circuit setup for which a proof can be generated that would be verified
//!   under a single aggregation circuit for the first milestone.
//!
//! The current implementation contains the following circuits:
//!
//! - [x] EVM Circuit
//! - [ ] State Circuit
//! - [x] Tx Circuit
//! - [x] Bytecode Circuit
//! - [x] Copy Circuit
//! - [x] Exponentiation Circuit
//! - [ ] Keccak Circuit
//! - [ ] MPT Circuit
//! - [x] PublicInputs Circuit
//!
//! And the following shared tables, with the circuits that use them:
//!
//! - [x] Copy Table
//!   - [x] Copy Circuit
//!   - [x] EVM Circuit
//! - [x] Exponentiation Table
//!   - [x] EVM Circuit
//! - [ ] Rw Table
//!   - [ ] State Circuit
//!   - [ ] EVM Circuit
//!   - [ ] Copy Circuit
//! - [x] Tx Table
//!   - [x] Tx Circuit
//!   - [x] EVM Circuit
//!   - [x] Copy Circuit
//!   - [x] PublicInputs Circuit
//! - [x] Bytecode Table
//!   - [x] Bytecode Circuit
//!   - [x] EVM Circuit
//!   - [x] Copy Circuit
//! - [ ] Block Table
//!   - [ ] EVM Circuit
//!   - [x] PublicInputs Circuit
//! - [ ] MPT Table
//!   - [ ] MPT Circuit
//!   - [ ] State Circuit
//! - [x] Keccak Table
//!   - [ ] Keccak Circuit
//!   - [ ] EVM Circuit
//!   - [x] Bytecode Circuit
//!   - [x] Tx Circuit
//!   - [ ] MPT Circuit

#[cfg(any(feature = "test", test))]
pub(crate) mod test;

use crate::{
    bytecode_circuit::circuit::{
        BytecodeCircuit, BytecodeCircuitConfig, BytecodeCircuitConfigArgs,
    },
    copy_circuit::{CopyCircuit, CopyCircuitConfig, CopyCircuitConfigArgs},
    evm_circuit::{EvmCircuit, EvmCircuitConfig, EvmCircuitConfigArgs},
    exp_circuit::{ExpCircuit, ExpCircuitConfig},
    keccak_circuit::{KeccakCircuit, KeccakCircuitConfig, KeccakCircuitConfigArgs},
    pi_circuit::{PiCircuit, PiCircuitConfig, PiCircuitConfigArgs},
    state_circuit::{StateCircuit, StateCircuitConfig, StateCircuitConfigArgs},
    table::{
        BlockTable, BytecodeTable, CopyTable, ExpTable, KeccakTable, MptTable, RwTable, TxTable,
    },
    tx_circuit::{TxCircuit, TxCircuitConfig, TxCircuitConfigArgs},
    util::{log2_ceil, Challenges, SubCircuit, SubCircuitConfig},
    witness::{block_convert, Block, MptUpdates},
};
use bus_mapping::{
    circuit_input_builder::{CircuitInputBuilder, CircuitsParams},
    mock::BlockData,
};
use eth_types::{geth_types::GethData, Field};
use halo2_proofs::{
    circuit::{Layouter, SimpleFloorPlanner, Value},
    plonk::{Circuit, ConstraintSystem, Error, Expression},
};

use std::array;

/// Configuration of the Super Circuit
#[derive(Clone)]
pub struct SuperCircuitConfig<F: Field> {
    block_table: Option<BlockTable>,
    mpt_table: Option<MptTable>,
    evm_circuit: Option<EvmCircuitConfig<F>>,
    state_circuit: Option<StateCircuitConfig<F>>,
    tx_circuit: Option<TxCircuitConfig<F>>,
    bytecode_circuit: Option<BytecodeCircuitConfig<F>>,
    copy_circuit: Option<CopyCircuitConfig<F>>,
    keccak_circuit: Option<KeccakCircuitConfig<F>>,
    pi_circuit: Option<PiCircuitConfig<F>>,
    exp_circuit: Option<ExpCircuitConfig<F>>,
}

/// Circuit configuration arguments
pub struct SuperCircuitConfigArgs {
    /// Max txs
    pub max_txs: usize,
    /// Max calldata
    pub max_calldata: usize,
    /// Mock randomness
    pub mock_randomness: u64,
    /// Configuration flags
    pub flags: SuperCircuitFlag,
}

impl<F: Field> SubCircuitConfig<F> for SuperCircuitConfig<F> {
    type ConfigArgs = SuperCircuitConfigArgs;

    /// Configure SuperCircuitConfig
    fn new(
        meta: &mut ConstraintSystem<F>,
        Self::ConfigArgs {
            max_txs,
            max_calldata,
            mock_randomness,
            flags,
        }: Self::ConfigArgs,
    ) -> Self {
        let tx_table = {
            if is_enabled(
                flags,
                SUPER_CIRCUIT_FLAG_PI
                    | SUPER_CIRCUIT_FLAG_TX
                    | SUPER_CIRCUIT_FLAG_COPY
                    | SUPER_CIRCUIT_FLAG_EVM,
            ) {
                Some(TxTable::construct(meta))
            } else {
                None
            }
        };
        let rw_table = {
            if is_enabled(
                flags,
                SUPER_CIRCUIT_FLAG_COPY | SUPER_CIRCUIT_FLAG_STATE | SUPER_CIRCUIT_FLAG_EVM,
            ) {
                Some(RwTable::construct(meta))
            } else {
                None
            }
        };
        let mpt_table = {
            if is_enabled(
                flags,
                SUPER_CIRCUIT_FLAG_MPT_TABLE | SUPER_CIRCUIT_FLAG_STATE,
            ) {
                Some(MptTable::construct(meta))
            } else {
                None
            }
        };
        let bytecode_table = {
            if is_enabled(
                flags,
                SUPER_CIRCUIT_FLAG_BYTECODE | SUPER_CIRCUIT_FLAG_COPY | SUPER_CIRCUIT_FLAG_EVM,
            ) {
                Some(BytecodeTable::construct(meta))
            } else {
                None
            }
        };
        let block_table = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_PI | SUPER_CIRCUIT_FLAG_EVM) {
                Some(BlockTable::construct(meta))
            } else {
                None
            }
        };
        let (q_copy_table, copy_table) = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_COPY | SUPER_CIRCUIT_FLAG_EVM) {
                let q_copy_table = meta.fixed_column();
                let copy_table = CopyTable::construct(meta, q_copy_table);
                (Some(q_copy_table), Some(copy_table))
            } else {
                (None, None)
            }
        };
        let exp_table = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_EXP | SUPER_CIRCUIT_FLAG_EVM) {
                Some(ExpTable::construct(meta))
            } else {
                None
            }
        };
        let keccak_table = {
            if is_enabled(
                flags,
                SUPER_CIRCUIT_FLAG_KECCAK
                    | SUPER_CIRCUIT_FLAG_TX
                    | SUPER_CIRCUIT_FLAG_BYTECODE
                    | SUPER_CIRCUIT_FLAG_EVM,
            ) {
                Some(KeccakTable::construct(meta))
            } else {
                None
            }
        };

        // Use a mock randomness instead of the randomness derived from the challange
        // (either from mock or real prover) to help debugging assignments.
        let power_of_randomness: [Expression<F>; 31] = array::from_fn(|i| {
            Expression::Constant(F::from(mock_randomness).pow(&[1 + i as u64, 0, 0, 0]))
        });

        let challenges = Challenges::mock(
            power_of_randomness[0].clone(),
            power_of_randomness[0].clone(),
            power_of_randomness[0].clone(),
        );

        let keccak_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_KECCAK) {
                Some(KeccakCircuitConfig::new(
                    meta,
                    KeccakCircuitConfigArgs {
                        keccak_table: keccak_table.clone().unwrap(),
                        challenges: challenges.clone(),
                    },
                ))
            } else {
                None
            }
        };

        let pi_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_PI) {
                Some(PiCircuitConfig::new(
                    meta,
                    PiCircuitConfigArgs {
                        max_txs,
                        max_calldata,
                        block_table: block_table.clone().unwrap(),
                        tx_table: tx_table.clone().unwrap(),
                    },
                ))
            } else {
                None
            }
        };

        let tx_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_TX) {
                Some(TxCircuitConfig::new(
                    meta,
                    TxCircuitConfigArgs {
                        tx_table: tx_table.clone().unwrap(),
                        keccak_table: keccak_table.clone().unwrap(),
                        challenges: challenges.clone(),
                    },
                ))
            } else {
                None
            }
        };

        let bytecode_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_BYTECODE) {
                Some(BytecodeCircuitConfig::new(
                    meta,
                    BytecodeCircuitConfigArgs {
                        bytecode_table: bytecode_table.clone().unwrap(),
                        keccak_table: keccak_table.clone().unwrap(),
                        challenges: challenges.clone(),
                    },
                ))
            } else {
                None
            }
        };

        let copy_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_COPY) {
                Some(CopyCircuitConfig::new(
                    meta,
                    CopyCircuitConfigArgs {
                        tx_table: tx_table.clone().unwrap(),
                        rw_table: rw_table.unwrap(),
                        bytecode_table: bytecode_table.clone().unwrap(),
                        copy_table: copy_table.unwrap(),
                        q_enable: q_copy_table.unwrap(),
                        challenges: challenges.clone(),
                    },
                ))
            } else {
                None
            }
        };

        let state_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_STATE) {
                Some(StateCircuitConfig::new(
                    meta,
                    StateCircuitConfigArgs {
                        rw_table: rw_table.unwrap(),
                        mpt_table: mpt_table.unwrap(),
                        challenges: challenges.clone(),
                    },
                ))
            } else {
                None
            }
        };

        let exp_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_EXP) {
                Some(ExpCircuitConfig::new(meta, exp_table.unwrap()))
            } else {
                None
            }
        };

        let evm_circuit = {
            if is_enabled(flags, SUPER_CIRCUIT_FLAG_EVM) {
                Some(EvmCircuitConfig::new(
                    meta,
                    EvmCircuitConfigArgs {
                        challenges,
                        tx_table: tx_table.unwrap(),
                        rw_table: rw_table.unwrap(),
                        bytecode_table: bytecode_table.unwrap(),
                        block_table: block_table.clone().unwrap(),
                        copy_table: copy_table.unwrap(),
                        keccak_table: keccak_table.unwrap(),
                        exp_table: exp_table.unwrap(),
                    },
                ))
            } else {
                None
            }
        };

        Self {
            block_table,
            mpt_table,
            evm_circuit,
            state_circuit,
            copy_circuit,
            tx_circuit,
            bytecode_circuit,
            keccak_circuit,
            pi_circuit,
            exp_circuit,
        }
    }
}

/// The type used for SuperCircuit configuration.
pub type SuperCircuitFlag = u64;
/// Does not configure any sub circuits nor tables.
pub const SUPER_CIRCUIT_FLAG_NONE: SuperCircuitFlag = 0;
/// Enable the `EVMCircuit`.
pub const SUPER_CIRCUIT_FLAG_EVM: SuperCircuitFlag = 1 << 0;
/// Enable the `StateCircuit`.
pub const SUPER_CIRCUIT_FLAG_STATE: SuperCircuitFlag = 1 << 1;
/// Enable the `TxCircuit`.
pub const SUPER_CIRCUIT_FLAG_TX: SuperCircuitFlag = 1 << 2;
/// Enable the `PiCircuit`.
pub const SUPER_CIRCUIT_FLAG_PI: SuperCircuitFlag = 1 << 3;
/// Enable the `BytecodeCircuit`.
pub const SUPER_CIRCUIT_FLAG_BYTECODE: SuperCircuitFlag = 1 << 4;
/// Enable the `CopyCircuit`.
pub const SUPER_CIRCUIT_FLAG_COPY: SuperCircuitFlag = 1 << 5;
/// Enable the `ExpCircuit`.
pub const SUPER_CIRCUIT_FLAG_EXP: SuperCircuitFlag = 1 << 6;
/// Enable the `KeccakCircuit`.
pub const SUPER_CIRCUIT_FLAG_KECCAK: SuperCircuitFlag = 1 << 7;
/// Load the `BlockTable`.
pub const SUPER_CIRCUIT_FLAG_BLOCK_TABLE: SuperCircuitFlag = 1 << 8;
/// Load the `MptTable`.
pub const SUPER_CIRCUIT_FLAG_MPT_TABLE: SuperCircuitFlag = 1 << 9;
/// Enable all sub circuits and tables.
pub const SUPER_CIRCUIT_FLAG_DEFAULT: SuperCircuitFlag = SUPER_CIRCUIT_FLAG_EVM
    | SUPER_CIRCUIT_FLAG_STATE
    | SUPER_CIRCUIT_FLAG_TX
    | SUPER_CIRCUIT_FLAG_PI
    | SUPER_CIRCUIT_FLAG_BYTECODE
    | SUPER_CIRCUIT_FLAG_COPY
    | SUPER_CIRCUIT_FLAG_EXP
    | SUPER_CIRCUIT_FLAG_KECCAK
    | SUPER_CIRCUIT_FLAG_BLOCK_TABLE
    | SUPER_CIRCUIT_FLAG_MPT_TABLE;

fn is_enabled(value: SuperCircuitFlag, flag: SuperCircuitFlag) -> bool {
    (value & flag) != 0
}

/// The Super Circuit contains all the zkEVM circuits
#[derive(Clone, Default, Debug)]
pub struct SuperCircuit<
    F: Field,
    const MAX_TXS: usize,
    const MAX_CALLDATA: usize,
    const MOCK_RANDOMNESS: u64,
    const FLAGS: SuperCircuitFlag,
> {
    /// EVM Circuit
    pub evm_circuit: EvmCircuit<F>,
    /// State Circuit
    pub state_circuit: StateCircuit<F>,
    /// The transaction circuit that will be used in the `synthesize` step.
    pub tx_circuit: TxCircuit<F>,
    /// Public Input Circuit
    pub pi_circuit: PiCircuit<F>,
    /// Bytecode Circuit
    pub bytecode_circuit: BytecodeCircuit<F>,
    /// Copy Circuit
    pub copy_circuit: CopyCircuit<F>,
    /// Exp Circuit
    pub exp_circuit: ExpCircuit<F>,
    /// Keccak Circuit
    pub keccak_circuit: KeccakCircuit<F>,
}

impl<
        F: Field,
        const MAX_TXS: usize,
        const MAX_CALLDATA: usize,
        const MOCK_RANDOMNESS: u64,
        const FLAGS: SuperCircuitFlag,
    > SuperCircuit<F, MAX_TXS, MAX_CALLDATA, MOCK_RANDOMNESS, FLAGS>
{
    /// Return the number of rows required to verify a given block
    pub fn get_num_rows_required(block: &Block<F>) -> usize {
        assert_eq!(block.circuits_params.max_txs, MAX_TXS);

        let mut num_rows_evm_circuit = 0;
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_EVM) {
            num_rows_evm_circuit = EvmCircuit::<F>::get_num_rows_required(block);
        }
        let mut num_rows_tx_circuit = 0;
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_TX) {
            num_rows_tx_circuit =
                TxCircuitConfig::<F>::get_num_rows_required(block.circuits_params.max_txs);
        }

        num_rows_evm_circuit.max(num_rows_tx_circuit)
    }
}

// Eventhough the SuperCircuit is not a subcircuit we implement the SubCircuit
// trait for it in order to get the `new_from_block` and `instance` methods that
// allow us to generalize integration tests.
impl<
        F: Field,
        const MAX_TXS: usize,
        const MAX_CALLDATA: usize,
        const MOCK_RANDOMNESS: u64,
        const FLAGS: SuperCircuitFlag,
    > SubCircuit<F> for SuperCircuit<F, MAX_TXS, MAX_CALLDATA, MOCK_RANDOMNESS, FLAGS>
{
    type Config = SuperCircuitConfig<F>;

    fn new_from_block(block: &Block<F>) -> Self {
        let evm_circuit = EvmCircuit::new_from_block(block);
        let state_circuit = StateCircuit::new_from_block(block);
        let tx_circuit = TxCircuit::new_from_block(block);
        let pi_circuit = PiCircuit::new_from_block(block);
        let bytecode_circuit = BytecodeCircuit::new_from_block(block);
        let copy_circuit = CopyCircuit::new_from_block_no_external(block);
        let exp_circuit = ExpCircuit::new_from_block(block);
        let keccak_circuit = KeccakCircuit::new_from_block(block);

        SuperCircuit::<_, MAX_TXS, MAX_CALLDATA, MOCK_RANDOMNESS, FLAGS> {
            evm_circuit,
            state_circuit,
            tx_circuit,
            pi_circuit,
            bytecode_circuit,
            copy_circuit,
            exp_circuit,
            keccak_circuit,
        }
    }

    /// Returns suitable inputs for the SuperCircuit.
    fn instance(&self) -> Vec<Vec<F>> {
        let mut instance = Vec::new();
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_KECCAK) {
            instance.extend_from_slice(&self.keccak_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_PI) {
            instance.extend_from_slice(&self.pi_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_TX) {
            instance.extend_from_slice(&self.tx_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_BYTECODE) {
            instance.extend_from_slice(&self.bytecode_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_COPY) {
            instance.extend_from_slice(&self.copy_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_STATE) {
            instance.extend_from_slice(&self.state_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_EXP) {
            instance.extend_from_slice(&self.exp_circuit.instance());
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_EVM) {
            instance.extend_from_slice(&self.evm_circuit.instance());
        }

        instance
    }

    /// Return the minimum number of rows required to prove the block
    fn min_num_rows_block(block: &Block<F>) -> (usize, usize) {
        let mut rows: Vec<(usize, usize)> = Vec::new();

        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_EVM) {
            rows.push(EvmCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_STATE) {
            rows.push(StateCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_BYTECODE) {
            rows.push(BytecodeCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_COPY) {
            rows.push(CopyCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_KECCAK) {
            rows.push(KeccakCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_TX) {
            rows.push(TxCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_EXP) {
            rows.push(ExpCircuit::min_num_rows_block(block));
        }
        if is_enabled(FLAGS, SUPER_CIRCUIT_FLAG_PI) {
            rows.push(PiCircuit::min_num_rows_block(block));
        }

        let (rows_without_padding, rows_with_padding): (Vec<usize>, Vec<usize>) =
            rows.into_iter().unzip();
        (
            itertools::max(rows_without_padding).unwrap_or_default(),
            itertools::max(rows_with_padding).unwrap_or_default(),
        )
    }

    /// Make the assignments to the SuperCircuit
    fn synthesize_sub(
        &self,
        config: &Self::Config,
        challenges: &Challenges<Value<F>>,
        layouter: &mut impl Layouter<F>,
    ) -> Result<(), Error> {
        if let Some(circuit_config) = &config.keccak_circuit {
            self.keccak_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.bytecode_circuit {
            self.bytecode_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.tx_circuit {
            self.tx_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.state_circuit {
            self.state_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.copy_circuit {
            self.copy_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.exp_circuit {
            self.exp_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.evm_circuit {
            self.evm_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        if let Some(circuit_config) = &config.pi_circuit {
            self.pi_circuit
                .synthesize_sub(circuit_config, challenges, layouter)?;
        }
        Ok(())
    }
}

impl<
        F: Field,
        const MAX_TXS: usize,
        const MAX_CALLDATA: usize,
        const MOCK_RANDOMNESS: u64,
        const FLAGS: SuperCircuitFlag,
    > Circuit<F> for SuperCircuit<F, MAX_TXS, MAX_CALLDATA, MOCK_RANDOMNESS, FLAGS>
{
    type Config = SuperCircuitConfig<F>;
    type FloorPlanner = SimpleFloorPlanner;

    fn without_witnesses(&self) -> Self {
        Self::default()
    }

    fn configure(meta: &mut ConstraintSystem<F>) -> Self::Config {
        Self::Config::new(
            meta,
            SuperCircuitConfigArgs {
                max_txs: MAX_TXS,
                max_calldata: MAX_CALLDATA,
                mock_randomness: MOCK_RANDOMNESS,
                flags: FLAGS,
            },
        )
    }

    fn synthesize(
        &self,
        config: Self::Config,
        mut layouter: impl Layouter<F>,
    ) -> Result<(), Error> {
        let block = self.evm_circuit.block.as_ref().unwrap();
        let challenges = Challenges::mock(
            Value::known(block.randomness),
            Value::known(block.randomness),
            Value::known(block.randomness),
        );
        let rws = &self.state_circuit.rows;

        if let Some(block_table) = &config.block_table {
            block_table.load(
                &mut layouter,
                &block.context,
                Value::known(block.randomness),
            )?;
        }

        if let Some(mpt_table) = &config.mpt_table {
            mpt_table.load(
                &mut layouter,
                &MptUpdates::mock_from(rws),
                Value::known(block.randomness),
            )?;
        }

        self.synthesize_sub(&config, &challenges, &mut layouter)
    }
}

impl<
        F: Field,
        const MAX_TXS: usize,
        const MAX_CALLDATA: usize,
        const MOCK_RANDOMNESS: u64,
        const FLAGS: SuperCircuitFlag,
    > SuperCircuit<F, MAX_TXS, MAX_CALLDATA, MOCK_RANDOMNESS, FLAGS>
{
    /// From the witness data, generate a SuperCircuit instance with all of the
    /// sub-circuits filled with their corresponding witnesses.
    ///
    /// Also, return with it the minimum required SRS degree for the
    /// circuit and the Public Inputs needed.
    #[allow(clippy::type_complexity)]
    pub fn build(
        geth_data: GethData,
        circuits_params: CircuitsParams,
    ) -> Result<(u32, Self, Vec<Vec<F>>, CircuitInputBuilder), bus_mapping::Error> {
        let block_data =
            BlockData::new_from_geth_data_with_params(geth_data.clone(), circuits_params);
        let mut builder = block_data.new_circuit_input_builder();
        builder
            .handle_block(&geth_data.eth_block, &geth_data.geth_traces)
            .expect("could not handle block tx");

        let ret = Self::build_from_circuit_input_builder(&builder)?;
        Ok((ret.0, ret.1, ret.2, builder))
    }

    /// From CircuitInputBuilder, generate a SuperCircuit instance with all of
    /// the sub-circuits filled with their corresponding witnesses.
    ///
    /// Also, return with it the minimum required SRS degree for the circuit and
    /// the Public Inputs needed.
    pub fn build_from_circuit_input_builder(
        builder: &CircuitInputBuilder,
    ) -> Result<(u32, Self, Vec<Vec<F>>), bus_mapping::Error> {
        let mut block = block_convert(&builder.block, &builder.code_db).unwrap();
        block.randomness = F::from(MOCK_RANDOMNESS);
        assert_eq!(block.circuits_params.max_txs, MAX_TXS);
        assert_eq!(block.circuits_params.max_calldata, MAX_CALLDATA);

        const NUM_BLINDING_ROWS: usize = 64;
        let (_, rows_needed) = Self::min_num_rows_block(&block);
        let k = log2_ceil(NUM_BLINDING_ROWS + rows_needed);
        log::debug!("super circuit uses k = {}", k);

        let circuit =
            SuperCircuit::<_, MAX_TXS, MAX_CALLDATA, MOCK_RANDOMNESS, FLAGS>::new_from_block(
                &block,
            );

        let instance = circuit.instance();
        Ok((k, circuit, instance))
    }
}
