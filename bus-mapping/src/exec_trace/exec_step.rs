// Doc this

use super::OperationRef;
use crate::evm::{
    EvmWord, GasCost, GlobalCounter, Memory, ProgramCounter, Stack, Storage,
};
use crate::Gas;
use crate::{
    error::Error,
    evm::{opcodes::Opcode, OpcodeId},
    exec_trace::TraceContext,
};
use std::collections::HashMap;

/// Represents a single step of an [`ExecutionTrace`](super::ExecutionTrace). It
/// contains all of the information relative to this step:
/// - [`Memory`] view at current execution step.
/// - [`Stack`] view at current execution step.
/// - EVM [`Opcode`](self::OpcodeId) executed in this step.
/// - [`ProgramCounter`] relative to this step.
/// - [`GlobalCounter`] assigned to this step by the program.
/// - Bus Mapping instances containing references to all of the
///   [`Operation`](crate::operation::Operation)s generated by this step.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExecutionStep {
    pub(crate) memory: Memory,
    pub(crate) stack: Stack,
    pub(crate) storage: Storage,
    pub(crate) instruction: OpcodeId,
    pub(crate) gas: Gas,
    pub(crate) gas_cost: GasCost,
    pub(crate) depth: u8,
    pub(crate) pc: ProgramCounter,
    pub(crate) gc: GlobalCounter,
    // Holds refs to the container with the related mem ops.
    pub(crate) bus_mapping_instance: Vec<OperationRef>,
}

impl ExecutionStep {
    /// Generate a new `ExecutionStep` from it's fields but with an empty
    /// bus-mapping instance vec.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        memory: Vec<u8>,
        stack: Vec<EvmWord>,
        storage: HashMap<EvmWord, EvmWord>,
        instruction: OpcodeId,
        gas: Gas,
        gas_cost: GasCost,
        depth: u8,
        pc: ProgramCounter,
        gc: GlobalCounter,
    ) -> Self {
        ExecutionStep {
            memory: Memory::from(memory),
            stack: Stack::new(stack),
            storage: Storage::from(storage),
            instruction,
            gas,
            gas_cost,
            depth,
            pc,
            gc,
            bus_mapping_instance: Vec::new(),
        }
    }

    /// Returns the Memory view of this `ExecutionStep`.
    pub const fn memory(&self) -> &Memory {
        &self.memory
    }

    /// Returns the Stack view of this `ExecutionStep`.
    pub const fn stack(&self) -> &Stack {
        &self.stack
    }

    /// Returns the Storage view of this `ExecutionStep`.
    pub const fn storage(&self) -> &Storage {
        &self.storage
    }

    /// Returns the [`OpcodeId`] executed at this step.
    pub const fn instruction(&self) -> &OpcodeId {
        &self.instruction
    }

    /// Returns the [`Gas`] of this step.
    pub const fn gas(&self) -> Gas {
        self.gas
    }

    /// Returns the [`GasCost`] of this step.
    pub const fn gas_cost(&self) -> GasCost {
        self.gas_cost
    }

    /// Returns the call-depth we're operating at this step.
    pub const fn depth(&self) -> u8 {
        self.depth
    }

    /// Returns the [`ProgramCounter`] that corresponds to this step.
    pub const fn pc(&self) -> ProgramCounter {
        self.pc
    }

    /// Returns the [`GlobalCounter`] associated to this step's `Instuction`
    /// execution.
    pub const fn gc(&self) -> GlobalCounter {
        self.gc
    }

    /// Sets the global counter of the instruction execution to the one sent
    /// in the params.
    pub(crate) fn set_gc(&mut self, gc: impl Into<GlobalCounter>) {
        self.gc = gc.into()
    }

    /// Returns a reference to the bus-mapping instance.
    pub const fn bus_mapping_instance(&self) -> &Vec<OperationRef> {
        &self.bus_mapping_instance
    }

    /// Returns a mutable reference to the bus-mapping instance.
    pub(crate) fn bus_mapping_instance_mut(
        &mut self,
    ) -> &mut Vec<OperationRef> {
        &mut self.bus_mapping_instance
    }

    /// Given a mutable reference to an [`OperationContainer`](crate::operation::OperationContainer), generate all of
    /// it's associated Memory, Stack and Storage operations, and register
    /// them in the container.
    ///
    /// This function will not only add the ops to the [`OperationContainer`](crate::operation::OperationContainer) but also get it's
    /// [`OperationRef`]s and add them to the bus-mapping instance of the step.
    ///
    /// ## Returns the #operations added by the
    /// [`OpcodeId`](crate::evm::OpcodeId) into the container.
    pub(crate) fn gen_associated_ops(
        &mut self,
        ctx: &mut TraceContext,
        next_steps: &[ExecutionStep],
    ) -> Result<(), Error> {
        let instruction = *self.instruction();
        instruction.gen_associated_ops(ctx, self, next_steps)
    }
}
