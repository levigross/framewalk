//! Transport-layer errors.
//!
//! These wrap I/O failures plus protocol-layer errors the transport might
//! see. Callers distinguish by variant: `Spawn` is a startup-time failure,
//! `Io` is a mid-session stream error, `Exited` means GDB went away while
//! the caller was waiting for a result, `Protocol` surfaces the sans-IO
//! `ProtocolError` through the transport boundary.

use framewalk_mi_protocol::ProtocolError;
use thiserror::Error;

/// A transport-layer error.
#[derive(Debug, Error)]
pub enum TransportError {
    /// Failed to spawn the GDB subprocess. Typically means the `gdb`
    /// binary is not on `PATH` or the configured program path is wrong.
    #[error("failed to spawn gdb: {0}")]
    Spawn(#[source] std::io::Error),

    /// One of the expected stdio pipes (`stdin`, `stdout`, or `stderr`)
    /// was not available on the spawned child. In practice this indicates
    /// a tokio/process version mismatch and should not happen.
    #[error("expected stdio pipe '{0}' was not available on the spawned gdb child")]
    PipeMissing(&'static str),

    /// A generic I/O error while reading from or writing to GDB.
    #[error("transport I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// The sans-IO protocol layer rejected some bytes. In Step 5 this is
    /// reserved for future use — `Connection::receive_bytes` currently
    /// only surfaces parse errors as events, never as return values, so
    /// this variant exists for forward-compat.
    #[error("protocol error: {0}")]
    Protocol(#[from] ProtocolError),

    /// GDB exited (or its stdin/stdout closed) before the caller's
    /// operation could complete. For `submit` this means the oneshot
    /// channel was dropped without a result; for `shutdown` it means
    /// the child process was no longer alive when we tried to wait.
    #[error("gdb exited before the operation completed")]
    Exited,

    /// A mandatory session-bootstrap command failed before the MCP
    /// server started serving requests. This is fatal because framewalk's
    /// execution model assumes the bootstrap semantics are in place.
    #[error("gdb bootstrap command `{command}` failed: {message}")]
    Bootstrap { command: String, message: String },

    /// The framer's internal buffer grew beyond the configured limit.
    /// This means GDB emitted a very long line without a newline, or
    /// hung mid-line. The reader task disconnects to prevent OOM.
    #[error("framer buffer overflow: {pending_bytes} bytes pending (limit: {limit})")]
    BufferOverflow { pending_bytes: usize, limit: usize },
}
