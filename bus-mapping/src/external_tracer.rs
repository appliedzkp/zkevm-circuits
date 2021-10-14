//! This module generates traces by connecting to an external tracer
use crate::util::serialize_field_ext;
use crate::Error;
use crate::{
    bytecode::Bytecode, BlockConstants, ExecutionStep, ExecutionTrace,
};
use geth_utils;
use pasta_curves::arithmetic::FieldExt;
use serde::Serialize;

/// Definition of all of the constants related to an Ethereum transaction.
#[derive(Debug, Clone, Serialize)]
pub struct Transaction<F: FieldExt> {
    #[serde(serialize_with = "serialize_field_ext")]
    origin: F,
    #[serde(serialize_with = "serialize_field_ext")]
    gas_limit: F,
    #[serde(serialize_with = "serialize_field_ext")]
    target: F,
}

impl<F: FieldExt> Default for Transaction<F> {
    fn default() -> Self {
        Transaction {
            origin: F::from_u64(0xc014ba5eu64),
            gas_limit: F::from_u64(1_000_000u64),
            target: F::from_u64(0xc0416ac1u64),
        }
    }
}

/// Definition of all of the data related to an account.
#[derive(Debug, Clone, Serialize)]
pub struct Account<F: FieldExt> {
    #[serde(serialize_with = "serialize_field_ext")]
    address: F,
    #[serde(serialize_with = "serialize_field_ext")]
    balance: F,
    code: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(bound(serialize = "BlockConstants<F>: Serialize",))]
struct GethConfig<F: FieldExt> {
    block_constants: BlockConstants<F>,
    transaction: Transaction<F>,
    accounts: Vec<Account<F>>,
}

/// Creates a trace for the specified config
pub fn trace<F: FieldExt>(
    block_constants: &BlockConstants<F>,
    code: &Bytecode,
) -> Result<Vec<ExecutionStep>, Error> {
    // Some default values for now
    let transaction = Transaction::default();
    let account = Account {
        address: transaction.target,
        balance: F::from_u64(555u64),
        code: hex::encode(code.to_bytes()),
    };

    let geth_config = GethConfig {
        block_constants: block_constants.clone(),
        transaction,
        accounts: vec![account],
    };

    // Get the trace
    let trace =
        geth_utils::trace(&serde_json::to_string(&geth_config).unwrap())
            .map_err(|_| Error::TracingError)?;

    // Generate the execution steps
    ExecutionTrace::<F>::load_trace(trace.as_bytes())
}
