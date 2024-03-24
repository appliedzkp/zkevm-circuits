//! Witness for all circuits.
//! The `Block<F>` is the witness struct post-processed from geth traces and
//! used to generate witnesses for circuits.

mod block;
///
pub mod chunk;
pub use block::{block_convert, Block, BlockContext};
pub use chunk::{chunk_convert, Chunk};
mod mpt;
pub use mpt::{MptUpdate, MptUpdateRow, MptUpdates};
pub mod rw;
pub use bus_mapping::circuit_input_builder::{Call, ExecStep, Transaction, Withdrawal};
pub use rw::{Rw, RwMap, RwRow};
