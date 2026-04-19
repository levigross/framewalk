//! `framewalk-mi-codec` — typed AST, recursive-descent parser, and command
//! encoder for the GDB/MI v3 wire grammar.
//!
//! This crate is `#![no_std]` + `alloc` and has zero external dependencies
//! besides `framewalk-mi-wire`. The parser is hand-written so every function
//! has a direct textual correspondence to the BNF productions in the GDB
//! manual — important for clean-room provenance.
//!
//! See [`parse_record`](parse::parse_record) for the main entry point and
//! the [`ast`] module for the types it produces.

#![no_std]

extern crate alloc;

// `proptest` macros expand to code that references `std::panic` / `std::format`.
// Bringing `std` into scope under `cfg(test)` keeps the non-test crate graph
// std-free while still letting test binaries use proptest freely.
#[cfg(test)]
extern crate std;

pub mod ast;
pub mod command;
pub mod encode;
pub mod error;
pub mod parse;

pub use ast::{
    AsyncClass, AsyncRecord, ListValue, Record, ResultClass, ResultRecord, StreamRecord, Token,
    Value,
};
pub use command::{CommandOption, MiCommand};
pub use encode::encode_command;
pub use error::{CStringError, CodecError, CodecErrorKind, Expected};
pub use parse::parse_record;
