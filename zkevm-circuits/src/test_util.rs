//! Testing utilities

use crate::{
    evm_circuit::EvmCircuit,
    state_circuit::StateCircuit,
    util::SubCircuit,
    witness::{Block, Rw},
};
use bus_mapping::{circuit_input_builder::CircuitsParams, mock::BlockData};
use eth_types::geth_types::GethData;

use halo2_proofs::dev::MockProver;
use halo2_proofs::halo2curves::bn256::Fr;
use mock::TestContext;

#[cfg(test)]
#[ctor::ctor]
fn init_env_logger() {
    // Enable RUST_LOG during tests
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("error")).init();
}

/// Bytecode circuit test configuration
#[derive(Debug, Clone)]
pub struct BytecodeTestConfig {
    /// Test EVM circuit
    pub enable_evm_circuit_test: bool,
    /// Test state circuit
    pub enable_state_circuit_test: bool,
    /// Gas limit
    pub gas_limit: u64,
}

impl Default for BytecodeTestConfig {
    fn default() -> Self {
        Self {
            enable_evm_circuit_test: true,
            enable_state_circuit_test: true,
            gas_limit: 1_000_000u64,
        }
    }
}

/// Struct used to easily generate tests for EVM &| State circuits being able to
/// customize all of the steps involved in the testing itself.
///
/// By default, the tests run through `prover.assert_satisfied_par()` but the
/// builder pattern provides functions that allow to pass different functions
/// that the prover should execute when verifying the CTB correctness.
///
/// The CTB also includes a mechanism to recieve calls that will modify the
/// block produced from the [`TestContext`] and apply them before starting to
/// compute the proof.
///
/// ## Example:
/// ```rust, no_run
/// use eth_types::geth_types::Account;
/// use eth_types::{address, bytecode, Address, Bytecode, ToWord, Word, U256, word};
/// use mock::{TestContext, MOCK_ACCOUNTS, gwei, eth};
/// use zkevm_circuits::test_util::CircuitTestBuilder;
///     let code = bytecode! {
/// // [ADDRESS, STOP]
///     PUSH32(word!("
/// 3000000000000000000000000000000000000000000000000000000000000000"))
///     PUSH1(0)
///     MSTORE
///
///     PUSH1(2)
///     PUSH1(0)
///     RETURN
/// };
/// let ctx = TestContext::<1, 1>::new(
///     None,
///     |accs| {
///         accs[0].address(MOCK_ACCOUNTS[0]).balance(eth(20));
///     },
///     |mut txs, _accs| {
///         txs[0]
///             .from(MOCK_ACCOUNTS[0])
///             .gas_price(gwei(2))
///             .gas(Word::from(0x10000))
///             .value(eth(2))
///             .input(code.into());
///     },
///     |block, _tx| block.number(0xcafeu64),
/// )
/// .unwrap();
///
/// CircuitTestBuilder::empty()
///     .test_ctx(ctx)
///     .block_modifier(Box::new(|block| block.evm_circuit_pad_to = (1 << 18) - 100))
///     .state_checks(Box::new(|prover| assert!(prover.verify_par().is_err())))
///     .run();
/// ```
pub struct CircuitTestBuilder<const NACC: usize, const NTX: usize> {
    test_ctx: Option<TestContext<NACC, NTX>>,
    bytecode_config: Option<BytecodeTestConfig>,
    circuit_params: Option<CircuitsParams>,
    block: Option<Block<Fr>>,
    evm_checks: Option<Box<dyn Fn(MockProver<Fr>)>>,
    state_checks: Option<Box<dyn Fn(MockProver<Fr>)>>,
    block_modifiers: Vec<Box<dyn Fn(&mut Block<Fr>)>>,
}

impl<const NACC: usize, const NTX: usize> CircuitTestBuilder<NACC, NTX> {
    /// Generates an empty/set to default `CircuitTestBuilder`.
    pub fn empty() -> Self {
        CircuitTestBuilder {
            test_ctx: None,
            bytecode_config: None,
            circuit_params: None,
            block: None,
            evm_checks: Some(Box::new(|prover| prover.assert_satisfied_par())),
            state_checks: Some(Box::new(|prover| prover.assert_satisfied_par())),
            block_modifiers: vec![],
        }
    }

    /// Allows to procide a [`TestContext`] which will serve as the generator of
    /// the Block.
    pub fn test_ctx(mut self, ctx: TestContext<NACC, NTX>) -> Self {
        self.test_ctx = Some(ctx);
        self
    }

    /// Allows to pass a non-default [`BytecodeConfig`] to the builder.
    pub fn config(mut self, config: BytecodeTestConfig) -> Self {
        self.bytecode_config = Some(config);
        self
    }

    /// Allows to pass a non-default [`CircuitParams`] to the builder.
    /// This means that we can increase for example, the `max_rws` or `max_txs`.
    pub fn params(mut self, params: CircuitsParams) -> Self {
        self.circuit_params = Some(params);
        self
    }

    /// Allows to pass a [`Block`] already built to the constructor.
    pub fn block(mut self, block: Block<Fr>) -> Self {
        self.block = Some(block);
        self
    }

    /// Allows to provide checks different than the default ones for the State
    /// Circuit verification.
    pub fn state_checks(mut self, state_checks: Box<dyn Fn(MockProver<Fr>)>) -> Self {
        self.state_checks = Some(state_checks);
        self
    }

    /// Allows to provide checks different than the default ones for the EVM
    /// Circuit verification.
    pub fn evm_checks(mut self, evm_checks: Box<dyn Fn(MockProver<Fr>)>) -> Self {
        self.evm_checks = Some(evm_checks);
        self
    }

    /// Allows to provide modifier functions for the [`Block`] that will be
    /// generated within this builder.
    ///
    /// That prevents to a lot of thests the need to build the block outside of
    /// the builder because they need to modify something particular.
    pub fn block_modifier(mut self, modifier: Box<dyn Fn(&mut Block<Fr>)>) -> Self {
        self.block_modifiers.push(modifier);
        self
    }
}

impl<const NACC: usize, const NTX: usize> CircuitTestBuilder<NACC, NTX> {
    /// Triggers the `CircuitTestBuilder` to convert the [`TestContext`] if any,
    /// into a [`Block`] and apply the default or provided block_modifiers or
    /// circuit checks to the provers generated for the State and EVM circuits.
    pub fn run(self) {
        let block: Block<Fr> = if self.block.is_some() {
            self.block.unwrap()
        } else if self.test_ctx.is_some() {
            let block: GethData = self.test_ctx.unwrap().into();
            let mut builder = BlockData::new_from_geth_data_with_params(
                block.clone(),
                self.circuit_params.unwrap_or_default(),
            )
            .new_circuit_input_builder();
            builder
                .handle_block(&block.eth_block, &block.geth_traces)
                .unwrap();
            // Build a witness block from trace result.
            let mut block =
                crate::witness::block_convert(&builder.block, &builder.code_db).unwrap();

            for modifier_fn in self.block_modifiers {
                modifier_fn.as_ref()(&mut block);
            }
            block
        } else {
            panic!("No attribute to build a block was passed to the CircuitTestBuilder")
        };

        // Fetch Bytecode TestConfig
        let config = self.bytecode_config.unwrap_or_default();

        // Run evm circuit test
        if config.enable_evm_circuit_test {
            let k = block.get_test_degree();

            let (_active_gate_rows, _active_lookup_rows) =
                EvmCircuit::<Fr>::get_active_rows(&block);

            let circuit = EvmCircuit::<Fr>::get_test_cicuit_from_block(block.clone());
            let prover = MockProver::<Fr>::run(k, &circuit, vec![]).unwrap();

            //prover.verify_at_rows_par(active_gate_rows.into_iter(),
            // active_lookup_rows.into_iter())
            self.evm_checks.unwrap().as_ref()(prover)
        }

        // Run state circuit test
        // TODO: use randomness as one of the circuit public input, since randomness in
        // state circuit and evm circuit must be same
        if config.enable_state_circuit_test {
            const N_ROWS: usize = 1 << 16;
            let state_circuit = StateCircuit::<Fr>::new(block.rws, N_ROWS);
            let power_of_randomness = state_circuit.instance();
            let prover = MockProver::<Fr>::run(18, &state_circuit, power_of_randomness).unwrap();
            // Skip verification of Start rows to accelerate testing
            let _non_start_rows_len = state_circuit
                .rows
                .iter()
                .filter(|rw| !matches!(rw, Rw::Start { .. }))
                .count();

            // prover
            //     .verify_at_rows(
            //         N_ROWS - non_start_rows_len..N_ROWS,
            //         N_ROWS - non_start_rows_len..N_ROWS,
            //     )
            //     .unwrap()
            self.state_checks.unwrap().as_ref()(prover);
        }
    }
}
