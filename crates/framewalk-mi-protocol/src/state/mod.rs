//! Protocol state registries.
//!
//! Each submodule owns one piece of the mutable state the protocol layer
//! tracks on the caller's behalf: target execution state, known threads,
//! current frames, breakpoints, variable objects, and GDB feature sets.
//! The [`Connection`](crate::connection::Connection) dispatches codec
//! records into these registries and exposes them via its introspection
//! methods for tools and higher layers that need to read live state.

pub mod breakpoints;
pub mod features;
pub mod frames;
pub mod target;
pub mod threads;
pub mod varobjs;

pub use breakpoints::{Breakpoint, BreakpointId, BreakpointLocation, BreakpointRegistry};
pub use features::FeatureSet;
pub use frames::{Frame, FrameRegistry};
pub use target::{StoppedReason, TargetState};
pub use threads::{ThreadGroupId, ThreadId, ThreadInfo, ThreadRegistry, ThreadState};
pub use varobjs::{VarObj, VarObjName, VarObjRegistry};
