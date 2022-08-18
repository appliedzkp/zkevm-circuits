//! Execution step related module.

use crate::{
    circuit_input_builder::CallContext, error::ExecError, exec_trace::OperationRef,
    operation::RWCounter, operation::RW,
};
use eth_types::{
    evm_types::{Gas, GasCost, OpcodeId, ProgramCounter},
    GethExecStep, H256,
};
use gadgets::impl_expr;
use halo2_proofs::plonk::Expression;
use strum_macros::EnumIter;

/// An execution step of the EVM.
#[derive(Clone, Debug)]
pub struct ExecStep {
    /// Execution state
    pub exec_state: ExecState,
    /// Program Counter
    pub pc: ProgramCounter,
    /// Stack size
    pub stack_size: usize,
    /// Memory size
    pub memory_size: usize,
    /// Gas left
    pub gas_left: Gas,
    /// Gas cost of the step.  If the error is OutOfGas caused by a "gas uint64
    /// overflow", this value will **not** be the actual Gas cost of the
    /// step.
    pub gas_cost: GasCost,
    /// Accumulated gas refund
    pub gas_refund: Gas,
    /// Call index within the Transaction.
    pub call_index: usize,
    /// The global counter when this step was executed.
    pub rwc: RWCounter,
    /// Reversible Write Counter.  Counter of write operations in the call that
    /// will need to be undone in case of a revert.
    pub reversible_write_counter: usize,
    /// Log index when this step was executed.
    pub log_id: usize,
    /// The list of references to Operations in the container
    pub bus_mapping_instance: Vec<OperationRef>,
    /// Error generated by this step
    pub error: Option<ExecError>,
}

impl ExecStep {
    /// Create a new Self from a `GethExecStep`.
    pub fn new(
        step: &GethExecStep,
        call_ctx: &CallContext,
        rwc: RWCounter,
        reversible_write_counter: usize,
        log_id: usize,
    ) -> Self {
        ExecStep {
            exec_state: ExecState::Op(step.op),
            pc: step.pc,
            stack_size: step.stack.0.len(),
            memory_size: call_ctx.memory.len(),
            gas_left: step.gas,
            gas_cost: step.gas_cost,
            gas_refund: Gas(0),
            call_index: call_ctx.index,
            rwc,
            reversible_write_counter,
            log_id,
            bus_mapping_instance: Vec::new(),
            error: None,
        }
    }

    /// Returns `true` if `error` is oog and stack related..
    pub fn oog_or_stack_error(&self) -> bool {
        matches!(
            self.error,
            Some(ExecError::OutOfGas(_) | ExecError::StackOverflow | ExecError::StackUnderflow)
        )
    }
}

impl Default for ExecStep {
    fn default() -> Self {
        Self {
            exec_state: ExecState::Op(OpcodeId::INVALID(0)),
            pc: ProgramCounter(0),
            stack_size: 0,
            memory_size: 0,
            gas_left: Gas(0),
            gas_cost: GasCost(0),
            gas_refund: Gas(0),
            call_index: 0,
            rwc: RWCounter(0),
            reversible_write_counter: 0,
            log_id: 0,
            bus_mapping_instance: Vec::new(),
            error: None,
        }
    }
}

/// Execution state
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExecState {
    /// EVM Opcode ID
    Op(OpcodeId),
    /// Virtual step Begin Tx
    BeginTx,
    /// Virtual step End Tx
    EndTx,
}

impl ExecState {
    /// Returns `true` if `ExecState` is an opcode and the opcode is a `PUSHn`.
    pub fn is_push(&self) -> bool {
        if let ExecState::Op(op) = self {
            op.is_push()
        } else {
            false
        }
    }

    /// Returns `true` if `ExecState` is an opcode and the opcode is a `DUPn`.
    pub fn is_dup(&self) -> bool {
        if let ExecState::Op(op) = self {
            op.is_dup()
        } else {
            false
        }
    }

    /// Returns `true` if `ExecState` is an opcode and the opcode is a `SWAPn`.
    pub fn is_swap(&self) -> bool {
        if let ExecState::Op(op) = self {
            op.is_swap()
        } else {
            false
        }
    }

    /// Returns `true` if `ExecState` is an opcode and the opcode is a `Logn`.
    pub fn is_log(&self) -> bool {
        if let ExecState::Op(op) = self {
            op.is_log()
        } else {
            false
        }
    }
}

/// Defines the various source/destination types for a copy event.
#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumIter)]
pub enum CopyDataType {
    /// When the source for the copy event is the bytecode table.
    Bytecode = 1,
    /// When the source/destination for the copy event is memory.
    Memory,
    /// When the source for the copy event is tx's calldata.
    TxCalldata,
    /// When the destination for the copy event is tx's log.
    TxLog,
    /// When the destination rows are not directly for copying but for a special
    /// scenario where we wish to accumulate the value (RLC) over all rows.
    /// This is used for Copy Lookup from SHA3 opcode verification.
    RlcAcc,
}

impl From<CopyDataType> for usize {
    fn from(t: CopyDataType) -> Self {
        t as usize
    }
}

impl Default for CopyDataType {
    fn default() -> Self {
        Self::Memory
    }
}

impl_expr!(CopyDataType);

/// Defines a single copy step in a copy event. This type is unified over the
/// source/destination row in the copy table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CopyStep {
    /// Address (source/destination) for the copy step.
    pub addr: u64,
    /// Represents the source/destination's type.
    pub tag: CopyDataType,
    /// Whether this step is a read or write step.
    pub rw: RW,
    /// Byte value copied in this step.
    pub value: u8,
    /// Optional field which is enabled only for the source being `bytecode`,
    /// and represents whether or not the byte is an opcode.
    pub is_code: Option<bool>,
    /// Represents the current RW counter at this copy step.
    pub rwc: RWCounter,
    /// A decrementing value representing the RW counters left in the copy event
    /// including the current step's RW counter.
    pub rwc_inc_left: u64,
}

/// Defines an enum type that can hold either a number or a hash value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NumberOrHash {
    /// Variant to indicate a number value.
    Number(usize),
    /// Variant to indicate a 256-bits hash value.
    Hash(H256),
}

/// Defines a copy event associated with EVM opcodes such as CALLDATACOPY,
/// CODECOPY, CREATE, etc. More information:
/// <https://github.com/privacy-scaling-explorations/zkevm-specs/blob/master/specs/copy-proof.md>.
#[derive(Clone, Debug)]
pub struct CopyEvent {
    /// Represents the start address at the source of the copy event.
    pub src_addr: u64,
    /// Represents the end address at the source of the copy event.
    pub src_addr_end: u64,
    /// Represents the source type.
    pub src_type: CopyDataType,
    /// Represents the relevant ID for source.
    pub src_id: NumberOrHash,
    /// Represents the start address at the destination of the copy event.
    pub dst_addr: u64,
    /// Represents the destination type.
    pub dst_type: CopyDataType,
    /// Represents the relevant ID for destination.
    pub dst_id: NumberOrHash,
    /// An optional field to hold the log ID in case of the destination being
    /// TxLog.
    pub log_id: Option<u64>,
    /// Represents the number of bytes copied as a part of this copy event.
    pub length: u64,
    /// Represents the list of copy steps in this copy event.
    pub steps: Vec<CopyStep>,
}
