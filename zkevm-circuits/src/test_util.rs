use crate::{
    evm_circuit::{table::FixedTableTag, witness::Block},
    state_circuit::StateCircuit,
};
use eth_types::evm_types::Gas;
use halo2_proofs::dev::{MockProver, VerifyFailure};
use pairing::bn256::Fr;

pub enum FixedTableConfig {
    Incomplete,
    Complete,
}

pub fn get_fixed_table(conf: FixedTableConfig) -> Vec<FixedTableTag> {
    match conf {
        FixedTableConfig::Incomplete => {
            vec![
                FixedTableTag::Range5,
                FixedTableTag::Range16,
                FixedTableTag::Range32,
                FixedTableTag::Range256,
                FixedTableTag::Range512,
                FixedTableTag::SignByte,
                FixedTableTag::ResponsibleOpcode,
            ]
        }
        FixedTableConfig::Complete => FixedTableTag::iterator().collect(),
    }
}

pub struct BytecodeTestConfig {
    pub enable_evm_circuit_test: bool,
    pub enable_state_circuit_test: bool,
    pub is_root_call: bool,
    pub call_data_length: usize,
    pub call_data_offset: u64,
    pub gas_limit: u64,
    pub evm_circuit_lookup_tags: Vec<FixedTableTag>,
}

impl Default for BytecodeTestConfig {
    fn default() -> Self {
        Self {
            is_root_call: true,
            enable_evm_circuit_test: true,
            enable_state_circuit_test: true,
            call_data_length: 0,
            call_data_offset: 0,
            gas_limit: 1_000_000u64,
            evm_circuit_lookup_tags: get_fixed_table(FixedTableConfig::Incomplete),
        }
    }
}

impl From<&BytecodeTestConfig> for bus_mapping::circuit_input_builder::TransactionConfig {
    fn from(config: &BytecodeTestConfig) -> Self {
        Self {
            is_root_call: config.is_root_call,
            call_data_length: config.call_data_length,
            call_data_offset: config.call_data_offset,
        }
    }
}

pub fn run_test_circuits(bytecode: eth_types::Bytecode) -> Result<(), Vec<VerifyFailure>> {
    test_circuits_using_bytecode(bytecode, BytecodeTestConfig::default())
}

pub fn test_circuits_using_bytecode(
    bytecode: eth_types::Bytecode,
    config: BytecodeTestConfig,
) -> Result<(), Vec<VerifyFailure>> {
    // execute the bytecode and get trace
    let block_trace = bus_mapping::mock::BlockData::new_from_geth_data(
        mock::new_single_tx_trace_code_gas(&bytecode, Gas(config.gas_limit)).unwrap(),
    );
    let mut builder = block_trace.new_circuit_input_builder();
    builder
        .handle_block(&block_trace.eth_block, &block_trace.geth_traces)
        .unwrap();

    // build a witness block from trace result
    let block = crate::evm_circuit::witness::block_convert(&builder.block, &builder.code_db);

    println!("zkevm - 1 - {block:?}");

    // finish required tests according to config using this witness block
    test_circuits_using_witness_block(block, config)
}

pub fn test_circuits_using_witness_block(
    block: Block<Fr>,
    config: BytecodeTestConfig,
) -> Result<(), Vec<VerifyFailure>> {
    // run evm circuit test
    if config.enable_evm_circuit_test {
        crate::evm_circuit::test::run_test_circuit(block.clone(), config.evm_circuit_lookup_tags)?;
    }

    // run state circuit test
    // TODO:
    //     (1) calculate circuit size(like MEMORY_ROWS_MAX etc) from block
    // rather than hard code  (2) use randomness as one of the circuit
    // public input, since randomness in state circuit and evm
    // circuit must be same
    if config.enable_state_circuit_test {
        let state_circuit =
            StateCircuit::<Fr, true, 2000, 100, 1023, 2000>::new(block.randomness, &block.rws);
        let prover = MockProver::<Fr>::run(12, &state_circuit, vec![]).unwrap();
        prover.verify()?;
    }

    Ok(())
}
