//! Internal library surface of `framewalk-mcp`.
//!
//! The binary in `main.rs` is a thin wrapper around this crate; everything
//! of substance — the MCP server, tool and resource implementations,
//! security guards, and serialisation helpers — lives here so it can be
//! unit- and integration-tested.

pub(crate) mod config;
pub(crate) mod raw_guard;
pub(crate) mod resources;
pub(crate) mod scheme;
pub(crate) mod server;
pub(crate) mod server_helpers;
pub(crate) mod tool_catalog;
pub(crate) mod tools;
pub(crate) mod types;

pub use config::Config;
pub use scheme::{SchemeHandle, SchemeSettings};
pub use server::FramewalkMcp;
