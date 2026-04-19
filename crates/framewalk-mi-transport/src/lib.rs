//! `framewalk-mi-transport` — async tokio transport for the framewalk
//! sans-IO core.
//!
//! Spawns `gdb --interpreter=mi3 --quiet --nx` as a subprocess and pumps
//! bytes between its stdin/stdout and a
//! [`framewalk_mi_protocol::Connection`]. Events fan out over a tokio
//! broadcast channel so multiple subscribers (MCP tool handlers, Scheme
//! workers, debug loggers) can consume the same event stream.
//!
//! # Example
//!
//! ```no_run
//! use framewalk_mi_codec::MiCommand;
//! use framewalk_mi_transport::{spawn, GdbConfig, TransportError};
//!
//! # async fn run() -> Result<(), TransportError> {
//! let handle = spawn(GdbConfig::new()).await?;
//!
//! // Subscribe to the event stream before anything else so we don't
//! // miss early notifications.
//! let mut events = handle.subscribe();
//!
//! // Submit a command and await its completion.
//! let outcome = handle.submit(MiCommand::new("gdb-version")).await?;
//! println!("version result: {outcome:?}");
//!
//! // Drain any pending events.
//! while let Ok(event) = events.try_recv() {
//!     println!("event: {event:?}");
//! }
//!
//! handle.shutdown().await?;
//! # Ok(())
//! # }
//! ```

pub(crate) mod error;
pub(crate) mod handle;
pub(crate) mod pump;
pub(crate) mod shared;
pub(crate) mod subprocess;

pub use error::TransportError;
pub use handle::{StateSnapshot, TransportHandle};
pub use shared::EventSeq;
pub use subprocess::{spawn, GdbConfig};
