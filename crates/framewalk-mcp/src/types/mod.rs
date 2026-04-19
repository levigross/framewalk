//! MCP tool input schemas, organised by GDB/MI command family.
//!
//! Each submodule covers one section of the GDB manual's MI chapter.
//! Domain enums (`PrintValues`, `RegisterFormat`, etc.) live in
//! `framewalk_mi_protocol::mi_types` — imported here by the structs
//! that need them so each tool's JSON schema exposes the enum choices.
//!
//! Structs are `pub` (not `pub(crate)`) because the rmcp `#[tool]`
//! macro generates code that references them across module boundaries.
//! The parent module chain is `pub(crate)`, so they are not reachable
//! from outside this crate regardless.
#![allow(unreachable_pub)]

pub(crate) mod breakpoints;
pub(crate) mod catchpoints;
pub(crate) mod context;
pub(crate) mod data;
pub(crate) mod execution;
pub(crate) mod file;
pub(crate) mod file_transfer;
pub(crate) mod misc;
pub(crate) mod raw;
pub(crate) mod session;
pub(crate) mod stack;
pub(crate) mod support;
pub(crate) mod symbol;
pub(crate) mod target;
pub(crate) mod threads;
pub(crate) mod tracepoints;
pub(crate) mod variables;
