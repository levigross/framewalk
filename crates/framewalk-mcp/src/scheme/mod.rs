//! Steel Scheme scripting layer for framewalk-mcp.
//!
//! Exposes a `scheme_eval` MCP tool that lets LLMs compose multi-step
//! GDB workflows in a single call using Steel (an R5RS Scheme dialect
//! embedded in Rust).
//!
//! The module is structured around three concerns:
//!
//! - **[`worker`]** — owns the Steel `Engine` on a dedicated thread and
//!   processes evaluation requests.
//! - **[`bindings`]** — Rust functions registered into the Scheme
//!   environment (`mi`, `wait-for-stop`).
//! - **[`marshal`]** — bidirectional conversion between MI protocol
//!   types and `SteelVal`.
//! - **[`route`]** — dynamic MCP tool route for `scheme_eval`.

pub(crate) mod bindings;
pub(crate) mod marshal;
pub(crate) mod route;
pub(crate) mod worker;

pub(crate) use route::scheme_eval_route;
pub use worker::SchemeHandle;

#[derive(Debug, Clone, Copy)]
pub struct SchemeSettings {
    pub eval_timeout: std::time::Duration,
    pub wait_timeout: std::time::Duration,
}
