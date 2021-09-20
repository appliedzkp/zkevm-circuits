use crate::error::Error;
use core::fmt::Debug;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Opcode enum. One-to-one corresponding to an `u8` value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OpcodeId {
    /// `STOP`
    STOP,
    /// `ADD`
    ADD,
    /// `MUL`
    MUL,
    /// `SUB`
    SUB,
    /// `DIV`
    DIV,
    /// `SDIV`
    SDIV,
    /// `MOD`
    MOD,
    /// `SMOD`
    SMOD,
    /// `ADDMOD`
    ADDMOD,
    /// `MULMOD`
    MULMOD,
    /// `EXP`
    EXP,
    /// `SIGNEXTEND`
    SIGNEXTEND,
    /// `LT`
    LT,
    /// `GT`
    GT,
    /// `SLT`
    SLT,
    /// `SGT`
    SGT,
    /// `EQ`
    EQ,
    /// `ISZERO`
    ISZERO,
    /// `AND`
    AND,
    /// `OR`
    OR,
    /// `XOR`
    XOR,
    /// `NOT`
    NOT,
    /// `BYTE`
    BYTE,

    /// `CALLDATALOAD`
    CALLDATALOAD,
    /// `CALLDATASIZE`
    CALLDATASIZE,
    /// `CALLDATACOPY`
    CALLDATACOPY,
    /// `CODESIZE`
    CODESIZE,
    /// `CODECOPY`
    CODECOPY,

    /// `SHL`
    SHL,
    /// `SHR`
    SHR,
    /// `SAR`
    SAR,

    /// `POP`
    POP,
    /// `MLOAD`
    MLOAD,
    /// `MSTORE`
    MSTORE,
    /// `MSTORE8`
    MSTORE8,
    /// `JUMP`
    JUMP,
    /// `JUMPI`
    JUMPI,
    /// `PC`
    PC,
    /// `MSIZE`
    MSIZE,
    /// `JUMPDEST`
    JUMPDEST,

    // PUSHn
    /// `PUSH1`
    PUSH1,
    /// `PUSH2`
    PUSH2,
    /// `PUSH3`
    PUSH3,
    /// `PUSH4`
    PUSH4,
    /// `PUSH5`
    PUSH5,
    /// `PUSH6`
    PUSH6,
    /// `PUSH7`
    PUSH7,
    /// `PUSH8`
    PUSH8,
    /// `PUSH9`
    PUSH9,
    /// `PUSH10`
    PUSH10,
    /// `PUSH11`
    PUSH11,
    /// `PUSH12`
    PUSH12,
    /// `PUSH13`
    PUSH13,
    /// `PUSH14`
    PUSH14,
    /// `PUSH15`
    PUSH15,
    /// `PUSH16`
    PUSH16,
    /// `PUSH17`
    PUSH17,
    /// `PUSH18`
    PUSH18,
    /// `PUSH19`
    PUSH19,
    /// `PUSH20`
    PUSH20,
    /// `PUSH21`
    PUSH21,
    /// `PUSH22`
    PUSH22,
    /// `PUSH23`
    PUSH23,
    /// `PUSH24`
    PUSH24,
    /// `PUSH25`
    PUSH25,
    /// `PUSH26`
    PUSH26,
    /// `PUSH27`
    PUSH27,
    /// `PUSH28`
    PUSH28,
    /// `PUSH29`
    PUSH29,
    /// `PUSH30`
    PUSH30,
    /// `PUSH31`
    PUSH31,
    /// `PUSH32`
    PUSH32,

    // DUPn
    /// `DUP1`
    DUP1,
    /// `DUP2`
    DUP2,
    /// `DUP3`
    DUP3,
    /// `DUP4`
    DUP4,
    /// `DUP5`
    DUP5,
    /// `DUP6`
    DUP6,
    /// `DUP7`
    DUP7,
    /// `DUP8`
    DUP8,
    /// `DUP9`
    DUP9,
    /// `DUP10`
    DUP10,
    /// `DUP11`
    DUP11,
    /// `DUP12`
    DUP12,
    /// `DUP13`
    DUP13,
    /// `DUP14`
    DUP14,
    /// `DUP15`
    DUP15,
    /// `DUP16`
    DUP16,

    // SWAPn
    /// `SWAP1`
    SWAP1,
    /// `SWAP2`
    SWAP2,
    /// `SWAP3`
    SWAP3,
    /// `SWAP4`
    SWAP4,
    /// `SWAP5`
    SWAP5,
    /// `SWAP6`
    SWAP6,
    /// `SWAP7`
    SWAP7,
    /// `SWAP8`
    SWAP8,
    /// `SWAP9`
    SWAP9,
    /// `SWAP10`
    SWAP10,
    /// `SWAP11`
    SWAP11,
    /// `SWAP12`
    SWAP12,
    /// `SWAP13`
    SWAP13,
    /// `SWAP14`
    SWAP14,
    /// `SWAP15`
    SWAP15,
    /// `SWAP16`
    SWAP16,

    /// `RETURN`
    RETURN,
    /// `REVERT`
    REVERT,

    /// `INVALID`
    INVALID,

    // External opcodes
    /// `SHA3`
    SHA3,
    /// `ADDRESS`
    ADDRESS,
    /// `BALANCE`
    BALANCE,
    /// `ORIGIN`
    ORIGIN,
    /// `CALLER`
    CALLER,
    /// `CALLVALUE`
    CALLVALUE,
    /// `GASPRICE`
    GASPRICE,
    /// `EXTCODESIZE`
    EXTCODESIZE,
    /// `EXTCODECOPY`
    EXTCODECOPY,
    /// `EXTCODEHASH`
    EXTCODEHASH,
    /// `RETURNDATASIZE`
    RETURNDATASIZE,
    /// `RETURNDATACOPY`
    RETURNDATACOPY,
    /// `BLOCKHASH`
    BLOCKHASH,
    /// `COINBASE`
    COINBASE,
    /// `TIMESTAMP`
    TIMESTAMP,
    /// `NUMBER`
    NUMBER,
    /// `DIFFICULTY`
    DIFFICULTY,
    /// `GASLIMIT`
    GASLIMIT,
    /// `CHAINID`
    CHAINID,
    /// `SELFBALANCE`
    SELFBALANCE,
    /// `BASEFEE`
    BASEFEE,
    /// `SLOAD`
    SLOAD,
    /// `SSTORE`
    SSTORE,
    /// `GAS`
    GAS,

    // LOGn
    /// `LOG0`
    LOG0,
    /// `LOG1`
    LOG1,
    /// `LOG2`
    LOG2,
    /// `LOG3`
    LOG3,
    /// `LOG4`
    LOG4,

    /// `CREATE`
    CREATE,
    /// `CREATE2`
    CREATE2,
    /// `CALL`
    CALL,
    /// `CALLCODE`
    CALLCODE,
    /// `DELEGATECALL`
    DELEGATECALL,
    /// `STATICCALL`
    STATICCALL,
    /// `SELFDESTRUCT`
    SELFDESTRUCT,
}

impl OpcodeId {
    /// Returns the `OpcodeId` as a `u8`.
    pub const fn as_u8(&self) -> u8 {
        match self {
            OpcodeId::STOP => 0x00u8,
            OpcodeId::ADD => 0x01u8,
            OpcodeId::MUL => 0x02u8,
            OpcodeId::SUB => 0x03u8,
            OpcodeId::DIV => 0x04u8,
            OpcodeId::SDIV => 0x05u8,
            OpcodeId::MOD => 0x06u8,
            OpcodeId::SMOD => 0x07u8,
            OpcodeId::ADDMOD => 0x08u8,
            OpcodeId::MULMOD => 0x09u8,
            OpcodeId::EXP => 0x0au8,
            OpcodeId::SIGNEXTEND => 0x0bu8,
            OpcodeId::LT => 0x10u8,
            OpcodeId::GT => 0x11u8,
            OpcodeId::SLT => 0x12u8,
            OpcodeId::SGT => 0x13u8,
            OpcodeId::EQ => 0x14u8,
            OpcodeId::ISZERO => 0x15u8,
            OpcodeId::AND => 0x16u8,
            OpcodeId::OR => 0x17u8,
            OpcodeId::XOR => 0x18u8,
            OpcodeId::NOT => 0x19u8,
            OpcodeId::BYTE => 0x1au8,
            OpcodeId::CALLDATALOAD => 0x35u8,
            OpcodeId::CALLDATASIZE => 0x36u8,
            OpcodeId::CALLDATACOPY => 0x37u8,
            OpcodeId::CODESIZE => 0x38u8,
            OpcodeId::CODECOPY => 0x39u8,
            OpcodeId::SHL => 0x1bu8,
            OpcodeId::SHR => 0x1cu8,
            OpcodeId::SAR => 0x1du8,
            OpcodeId::POP => 0x50u8,
            OpcodeId::MLOAD => 0x51u8,
            OpcodeId::MSTORE => 0x52u8,
            OpcodeId::MSTORE8 => 0x53u8,
            OpcodeId::JUMP => 0x56u8,
            OpcodeId::JUMPI => 0x57u8,
            OpcodeId::PC => 0x58u8,
            OpcodeId::MSIZE => 0x59u8,
            OpcodeId::JUMPDEST => 0x5bu8,
            OpcodeId::PUSH1 => 0x60u8,
            OpcodeId::PUSH2 => 0x61u8,
            OpcodeId::PUSH3 => 0x62u8,
            OpcodeId::PUSH4 => 0x63u8,
            OpcodeId::PUSH5 => 0x64u8,
            OpcodeId::PUSH6 => 0x65u8,
            OpcodeId::PUSH7 => 0x66u8,
            OpcodeId::PUSH8 => 0x67u8,
            OpcodeId::PUSH9 => 0x68u8,
            OpcodeId::PUSH10 => 0x69u8,
            OpcodeId::PUSH11 => 0x6au8,
            OpcodeId::PUSH12 => 0x6bu8,
            OpcodeId::PUSH13 => 0x6cu8,
            OpcodeId::PUSH14 => 0x6du8,
            OpcodeId::PUSH15 => 0x6eu8,
            OpcodeId::PUSH16 => 0x6fu8,
            OpcodeId::PUSH17 => 0x70u8,
            OpcodeId::PUSH18 => 0x71u8,
            OpcodeId::PUSH19 => 0x72u8,
            OpcodeId::PUSH20 => 0x73u8,
            OpcodeId::PUSH21 => 0x74u8,
            OpcodeId::PUSH22 => 0x75u8,
            OpcodeId::PUSH23 => 0x76u8,
            OpcodeId::PUSH24 => 0x77u8,
            OpcodeId::PUSH25 => 0x78u8,
            OpcodeId::PUSH26 => 0x79u8,
            OpcodeId::PUSH27 => 0x7au8,
            OpcodeId::PUSH28 => 0x7bu8,
            OpcodeId::PUSH29 => 0x7cu8,
            OpcodeId::PUSH30 => 0x7du8,
            OpcodeId::PUSH31 => 0x7eu8,
            OpcodeId::PUSH32 => 0x7fu8,
            OpcodeId::DUP1 => 0x80u8,
            OpcodeId::DUP2 => 0x81u8,
            OpcodeId::DUP3 => 0x82u8,
            OpcodeId::DUP4 => 0x83u8,
            OpcodeId::DUP5 => 0x84u8,
            OpcodeId::DUP6 => 0x85u8,
            OpcodeId::DUP7 => 0x86u8,
            OpcodeId::DUP8 => 0x87u8,
            OpcodeId::DUP9 => 0x88u8,
            OpcodeId::DUP10 => 0x89u8,
            OpcodeId::DUP11 => 0x8au8,
            OpcodeId::DUP12 => 0x8bu8,
            OpcodeId::DUP13 => 0x8cu8,
            OpcodeId::DUP14 => 0x8du8,
            OpcodeId::DUP15 => 0x8eu8,
            OpcodeId::DUP16 => 0x8fu8,
            OpcodeId::SWAP1 => 0x90u8,
            OpcodeId::SWAP2 => 0x91u8,
            OpcodeId::SWAP3 => 0x92u8,
            OpcodeId::SWAP4 => 0x93u8,
            OpcodeId::SWAP5 => 0x94u8,
            OpcodeId::SWAP6 => 0x95u8,
            OpcodeId::SWAP7 => 0x96u8,
            OpcodeId::SWAP8 => 0x97u8,
            OpcodeId::SWAP9 => 0x98u8,
            OpcodeId::SWAP10 => 0x99u8,
            OpcodeId::SWAP11 => 0x9au8,
            OpcodeId::SWAP12 => 0x9bu8,
            OpcodeId::SWAP13 => 0x9cu8,
            OpcodeId::SWAP14 => 0x9du8,
            OpcodeId::SWAP15 => 0x9eu8,
            OpcodeId::SWAP16 => 0x9fu8,
            OpcodeId::RETURN => 0xf3u8,
            OpcodeId::REVERT => 0xfdu8,
            OpcodeId::INVALID => 0xfeu8,
            OpcodeId::SHA3 => 0x20u8,
            OpcodeId::ADDRESS => 0x30u8,
            OpcodeId::BALANCE => 0x31u8,
            OpcodeId::ORIGIN => 0x32u8,
            OpcodeId::CALLER => 0x33u8,
            OpcodeId::CALLVALUE => 0x34u8,
            OpcodeId::GASPRICE => 0x3au8,
            OpcodeId::EXTCODESIZE => 0x3bu8,
            OpcodeId::EXTCODECOPY => 0x3cu8,
            OpcodeId::EXTCODEHASH => 0x3fu8,
            OpcodeId::RETURNDATASIZE => 0x3du8,
            OpcodeId::RETURNDATACOPY => 0x3eu8,
            OpcodeId::BLOCKHASH => 0x40u8,
            OpcodeId::COINBASE => 0x41u8,
            OpcodeId::TIMESTAMP => 0x42u8,
            OpcodeId::NUMBER => 0x43u8,
            OpcodeId::DIFFICULTY => 0x44u8,
            OpcodeId::GASLIMIT => 0x45u8,
            OpcodeId::CHAINID => 0x46u8,
            OpcodeId::SELFBALANCE => 0x47u8,
            OpcodeId::BASEFEE => 0x48u8,
            OpcodeId::SLOAD => 0x54u8,
            OpcodeId::SSTORE => 0x55u8,
            OpcodeId::GAS => 0x5au8,
            OpcodeId::LOG0 => 0xa0u8,
            OpcodeId::LOG1 => 0xa1u8,
            OpcodeId::LOG2 => 0xa2u8,
            OpcodeId::LOG3 => 0xa3u8,
            OpcodeId::LOG4 => 0xa4u8,
            OpcodeId::CREATE => 0xf0u8,
            OpcodeId::CREATE2 => 0xf5u8,
            OpcodeId::CALL => 0xf1u8,
            OpcodeId::CALLCODE => 0xf2u8,
            OpcodeId::DELEGATECALL => 0xf4u8,
            OpcodeId::STATICCALL => 0xfau8,
            OpcodeId::SELFDESTRUCT => 0xffu8,
        }
    }
}

impl FromStr for OpcodeId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "STOP" => OpcodeId::STOP,
            "ADD" => OpcodeId::ADD,
            "MUL" => OpcodeId::MUL,
            "SUB" => OpcodeId::SUB,
            "DIV" => OpcodeId::DIV,
            "SDIV" => OpcodeId::SDIV,
            "MOD" => OpcodeId::MOD,
            "SMOD" => OpcodeId::SMOD,
            "ADDMOD" => OpcodeId::ADDMOD,
            "MULMOD" => OpcodeId::MULMOD,
            "EXP" => OpcodeId::EXP,
            "SIGNEXTEND" => OpcodeId::SIGNEXTEND,
            "LT" => OpcodeId::LT,
            "GT" => OpcodeId::GT,
            "SLT" => OpcodeId::SLT,
            "SGT" => OpcodeId::SGT,
            "EQ" => OpcodeId::EQ,
            "ISZERO" => OpcodeId::ISZERO,
            "AND" => OpcodeId::AND,
            "OR" => OpcodeId::OR,
            "XOR" => OpcodeId::XOR,
            "NOT" => OpcodeId::NOT,
            "BYTE" => OpcodeId::BYTE,
            "CALLDATALOAD" => OpcodeId::CALLDATALOAD,
            "CALLDATASIZE" => OpcodeId::CALLDATASIZE,
            "CALLDATACOPY" => OpcodeId::CALLDATACOPY,
            "CODESIZE" => OpcodeId::CODESIZE,
            "CODECOPY" => OpcodeId::CODECOPY,
            "SHL" => OpcodeId::SHL,
            "SHR" => OpcodeId::SHR,
            "SAR" => OpcodeId::SAR,
            "POP" => OpcodeId::POP,
            "MLOAD" => OpcodeId::MLOAD,
            "MSTORE" => OpcodeId::MSTORE,
            "MSTORE8" => OpcodeId::MSTORE8,
            "JUMP" => OpcodeId::JUMP,
            "JUMPI" => OpcodeId::JUMPI,
            "PC" => OpcodeId::PC,
            "MSIZE" => OpcodeId::MSIZE,
            "JUMPDEST" => OpcodeId::JUMPDEST,
            "PUSH1" => OpcodeId::PUSH1,
            "PUSH2" => OpcodeId::PUSH2,
            "PUSH3" => OpcodeId::PUSH3,
            "PUSH4" => OpcodeId::PUSH4,
            "PUSH5" => OpcodeId::PUSH5,
            "PUSH6" => OpcodeId::PUSH6,
            "PUSH7" => OpcodeId::PUSH7,
            "PUSH8" => OpcodeId::PUSH8,
            "PUSH9" => OpcodeId::PUSH9,
            "PUSH10" => OpcodeId::PUSH10,
            "PUSH11" => OpcodeId::PUSH11,
            "PUSH12" => OpcodeId::PUSH12,
            "PUSH13" => OpcodeId::PUSH13,
            "PUSH14" => OpcodeId::PUSH14,
            "PUSH15" => OpcodeId::PUSH15,
            "PUSH16" => OpcodeId::PUSH16,
            "PUSH17" => OpcodeId::PUSH17,
            "PUSH18" => OpcodeId::PUSH18,
            "PUSH19" => OpcodeId::PUSH19,
            "PUSH20" => OpcodeId::PUSH20,
            "PUSH21" => OpcodeId::PUSH21,
            "PUSH22" => OpcodeId::PUSH22,
            "PUSH23" => OpcodeId::PUSH23,
            "PUSH24" => OpcodeId::PUSH24,
            "PUSH25" => OpcodeId::PUSH25,
            "PUSH26" => OpcodeId::PUSH26,
            "PUSH27" => OpcodeId::PUSH27,
            "PUSH28" => OpcodeId::PUSH28,
            "PUSH29" => OpcodeId::PUSH29,
            "PUSH30" => OpcodeId::PUSH30,
            "PUSH31" => OpcodeId::PUSH31,
            "PUSH32" => OpcodeId::PUSH32,
            "DUP1" => OpcodeId::DUP1,
            "DUP2" => OpcodeId::DUP2,
            "DUP3" => OpcodeId::DUP3,
            "DUP4" => OpcodeId::DUP4,
            "DUP5" => OpcodeId::DUP5,
            "DUP6" => OpcodeId::DUP6,
            "DUP7" => OpcodeId::DUP7,
            "DUP8" => OpcodeId::DUP8,
            "DUP9" => OpcodeId::DUP9,
            "DUP10" => OpcodeId::DUP10,
            "DUP11" => OpcodeId::DUP11,
            "DUP12" => OpcodeId::DUP12,
            "DUP13" => OpcodeId::DUP13,
            "DUP14" => OpcodeId::DUP14,
            "DUP15" => OpcodeId::DUP15,
            "DUP16" => OpcodeId::DUP16,
            "SWAP1" => OpcodeId::SWAP1,
            "SWAP2" => OpcodeId::SWAP2,
            "SWAP3" => OpcodeId::SWAP3,
            "SWAP4" => OpcodeId::SWAP4,
            "SWAP5" => OpcodeId::SWAP5,
            "SWAP6" => OpcodeId::SWAP6,
            "SWAP7" => OpcodeId::SWAP7,
            "SWAP8" => OpcodeId::SWAP8,
            "SWAP9" => OpcodeId::SWAP9,
            "SWAP10" => OpcodeId::SWAP10,
            "SWAP11" => OpcodeId::SWAP11,
            "SWAP12" => OpcodeId::SWAP12,
            "SWAP13" => OpcodeId::SWAP13,
            "SWAP14" => OpcodeId::SWAP14,
            "SWAP15" => OpcodeId::SWAP15,
            "SWAP16" => OpcodeId::SWAP16,
            "RETURN" => OpcodeId::RETURN,
            "REVERT" => OpcodeId::REVERT,
            "INVALID" => OpcodeId::INVALID,
            "SHA3" => OpcodeId::SHA3,
            "ADDRESS" => OpcodeId::ADDRESS,
            "BALANCE" => OpcodeId::BALANCE,
            "SELFBALANCE" => OpcodeId::SELFBALANCE,
            "ORIGIN" => OpcodeId::ORIGIN,
            "CALLER" => OpcodeId::CALLER,
            "CALLVALUE" => OpcodeId::CALLVALUE,
            "GASPRICE" => OpcodeId::GASPRICE,
            "EXTCODESIZE" => OpcodeId::EXTCODESIZE,
            "EXTCODECOPY" => OpcodeId::EXTCODECOPY,
            "EXTCODEHASH" => OpcodeId::EXTCODEHASH,
            "RETURNDATASIZE" => OpcodeId::RETURNDATASIZE,
            "RETURNDATACOPY" => OpcodeId::RETURNDATACOPY,
            "BLOCKHASH" => OpcodeId::BLOCKHASH,
            "COINBASE" => OpcodeId::COINBASE,
            "TIMESTAMP" => OpcodeId::TIMESTAMP,
            "NUMBER" => OpcodeId::NUMBER,
            "DIFFICULTY" => OpcodeId::DIFFICULTY,
            "GASLIMIT" => OpcodeId::GASLIMIT,
            "SLOAD" => OpcodeId::SLOAD,
            "SSTORE" => OpcodeId::SSTORE,
            "GAS" => OpcodeId::GAS,
            "LOG0" => OpcodeId::LOG0,
            "LOG1" => OpcodeId::LOG1,
            "LOG2" => OpcodeId::LOG2,
            "LOG3" => OpcodeId::LOG3,
            "LOG4" => OpcodeId::LOG4,
            "CREATE" => OpcodeId::CREATE,
            "CREATE2" => OpcodeId::CREATE2,
            "CALL" => OpcodeId::CALL,
            "CALLCODE" => OpcodeId::CALLCODE,
            "DELEGATECALL" => OpcodeId::DELEGATECALL,
            "STATICCALL" => OpcodeId::STATICCALL,
            "SELFDESTRUCT" => OpcodeId::SELFDESTRUCT,
            "CHAINID" => OpcodeId::CHAINID,
            "BASEFEE" => OpcodeId::BASEFEE,
            _ => return Err(Error::OpcodeParsing),
        })
    }
}
