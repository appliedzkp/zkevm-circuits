//! The bytecode circuit implementation.

/// Bytecode unroller
pub mod bytecode_unroller;
/// Bytecode circuit
pub mod circuit;
#[cfg(any(feature = "test", test, feature = "test-circuits"))]
mod dev;
pub(crate) mod param;
/// Bytecode circuit tester
#[cfg(any(feature = "test", test))]
mod test;
#[cfg(any(feature = "test", test, feature = "test-circuits"))]
pub use dev::BytecodeCircuit as TestBytecodeCircuit;
