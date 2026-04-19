//! `framewalk-mi-protocol` — sans-IO state machine for driving GDB over MI v3.
//!
//! This crate owns the [`Connection`] type: a runtime-agnostic, I/O-free
//! state machine that consumes bytes from a GDB subprocess, produces typed
//! events, and serialises outbound commands with token correlation.
//!
//! Following the `h11` / `quinn-proto` / `rustls` pattern: push bytes in,
//! pull events out, push commands in, pull bytes out. No I/O, no async
//! runtime, no threads. All of those concerns live one layer up in
//! `framewalk-mi-transport`.
//!
//! # Example
//!
//! ```
//! use framewalk_mi_codec::MiCommand;
//! use framewalk_mi_protocol::{Connection, CommandRequest, Event, ProtocolError};
//!
//! # fn run() -> Result<(), ProtocolError> {
//! let mut conn = Connection::new();
//! // Submit a command — the encoder writes bytes into the outbound buffer.
//! let handle = conn.submit(CommandRequest::new(MiCommand::new("gdb-version")));
//! assert_eq!(conn.outbound(), b"1-gdb-version\n");
//!
//! // Imagine the transport wrote those bytes and acknowledged them.
//! let n = conn.outbound().len();
//! conn.consume_outbound(n)?;
//!
//! // Later, bytes arrive from GDB's stdout. Feed them in.
//! conn.receive_bytes(b"~\"GNU gdb (GDB) 15.1\\n\"\n1^done\n(gdb)\n")?;
//!
//! // Drain events until empty.
//! let mut events = vec![];
//! while let Some(e) = conn.poll_event() { events.push(e); }
//! // events: [Console("GNU gdb (GDB) 15.1\n"), CommandCompleted{..}, GroupClosed]
//! # let _ = (handle, events);
//! # Ok(())
//! # }
//! # run().expect("doctest happy path cannot fail");
//! ```

pub mod command;
pub mod connection;
pub mod error;
pub mod event;
pub mod mi_types;
pub(crate) mod pending;
pub(crate) mod results_view;
pub mod state;
pub mod token;
pub mod version;

pub use command::{CommandHandle, CommandOutcome, CommandRequest};
pub use connection::Connection;
pub use error::{ParseFailure, ProtocolError};
pub use event::{Event, NotifyEvent, RunningEvent, StoppedEvent};
pub use state::{
    Breakpoint, BreakpointId, BreakpointLocation, BreakpointRegistry, FeatureSet, Frame,
    FrameRegistry, StoppedReason, TargetState, ThreadGroupId, ThreadId, ThreadInfo, ThreadRegistry,
    ThreadState, VarObj, VarObjName, VarObjRegistry,
};
pub use version::MiVersion;
