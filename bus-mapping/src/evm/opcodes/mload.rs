use super::Opcode;
use crate::circuit_input_builder::CircuitInputStateRef;
use crate::{operation::RW, Error};
use core::convert::TryInto;
use eth_types::evm_types::MemoryAddress;
use eth_types::{GethExecStep, ToBigEndian, Word};

/// Placeholder structure used to implement [`Opcode`] trait over it
/// corresponding to the [`OpcodeId::MLOAD`](crate::evm::OpcodeId::MLOAD)
/// `OpcodeId`. This is responsible of generating all of the associated
/// [`crate::operation::StackOp`]s and [`crate::operation::MemoryOp`]s and place
/// them inside the trace's [`crate::operation::OperationContainer`].
#[derive(Debug, Copy, Clone)]
pub(crate) struct Mload;

impl Opcode for Mload {
    fn gen_associated_ops(
        state: &mut CircuitInputStateRef,
        steps: &[GethExecStep],
    ) -> Result<(), Error> {
        let step = &steps[0];
        //
        // First stack read
        //
        let stack_value_read = step.stack.last()?;
        let stack_position = step.stack.last_filled();

        // Manage first stack read at latest stack position
        state.push_stack_op(RW::READ, stack_position, stack_value_read);

        // Read the memory
        let mut mem_read_addr: MemoryAddress = stack_value_read.try_into()?;
        // Accesses to memory that hasn't been initialized are valid, and return
        // 0.
        let mem_read_value = steps[1]
            .memory
            .read_word(mem_read_addr)
            .unwrap_or_else(|_| Word::zero());

        //
        // First stack write
        //
        state.push_stack_op(RW::WRITE, stack_position, mem_read_value);

        //
        // First mem read -> 32 MemoryOp generated.
        //
        let bytes = mem_read_value.to_be_bytes();
        bytes.iter().for_each(|value_byte| {
            state.push_memory_op(RW::READ, mem_read_addr, *value_byte);

            // Update mem_read_addr to next byte's one
            mem_read_addr += MemoryAddress::from(1);
        });

        Ok(())
    }
}

#[cfg(test)]
mod mload_tests {
    use super::*;
    use crate::operation::{MemoryOp, StackOp};
    use eth_types::bytecode;
    use eth_types::evm_types::{OpcodeId, StackAddress};
    use eth_types::Word;
    use pretty_assertions::assert_eq;

    #[test]
    fn mload_opcode_impl() {
        let code = bytecode! {
            .setup_state()

            PUSH1(0x40u64)
            MLOAD
            STOP
        };

        // Get the execution steps from the external tracer
        let block = crate::mock::BlockData::new_from_geth_data(
            mock::new_single_tx_trace_code(&code).unwrap(),
        );

        let mut builder = block.new_circuit_input_builder();
        builder
            .handle_block(&block.eth_block, &block.geth_traces)
            .unwrap();

        let step = builder.block.txs()[0]
            .steps()
            .iter()
            .find(|step| step.op == OpcodeId::MLOAD)
            .unwrap();

        assert_eq!(
            [0, 1]
                .map(|idx| &builder.block.container.stack[step.bus_mapping_instance[idx].as_usize()])
                .map(|operation| (operation.rw(), operation.op())),
            [
                (
                    RW::READ,
                    &StackOp::new(1, StackAddress::from(1023), Word::from(0x40))
                ),
                (
                    RW::WRITE,
                    &StackOp::new(1, StackAddress::from(1023), Word::from(0x80))
                )
            ]
        );

        assert_eq!(
            (2..34)
                .map(|idx| &builder.block.container.memory
                    [step.bus_mapping_instance[idx].as_usize()])
                .map(|operation| (operation.rw(), operation.op().clone()))
                .collect::<Vec<_>>(),
            Word::from(0x80)
                .to_be_bytes()
                .into_iter()
                .enumerate()
                .map(|(idx, byte)| (RW::READ, MemoryOp::new(1, MemoryAddress(idx + 0x40), byte)))
                .collect::<Vec<_>>()
        )
    }
}
