//! Commands submitted into a [`Connection`](crate::connection::Connection)
//! and the outcome carried back on their result records.

use framewalk_mi_codec::{MiCommand, Token, Value};

/// A request from a caller to submit an MI command to GDB.
///
/// This is a thin wrapper around [`framewalk_mi_codec::MiCommand`] that
/// exists so the protocol layer can add protocol-level concerns (deadlines,
/// cancellation, tracing spans) in future steps without breaking the
/// public API. For Step 3 it's just a newtype.
#[derive(Debug, Clone)]
pub struct CommandRequest {
    pub(crate) command: MiCommand,
}

impl CommandRequest {
    /// Wrap an [`MiCommand`] into a request ready for
    /// [`Connection::submit`](crate::connection::Connection::submit).
    pub fn new(command: MiCommand) -> Self {
        Self { command }
    }

    /// Access the underlying command for inspection.
    #[must_use]
    pub fn command(&self) -> &MiCommand {
        &self.command
    }
}

impl From<MiCommand> for CommandRequest {
    fn from(command: MiCommand) -> Self {
        Self::new(command)
    }
}

/// Opaque handle returned by
/// [`Connection::submit`](crate::connection::Connection::submit). The
/// caller retains it and uses it to match an eventual
/// [`Event::CommandCompleted`](crate::event::Event::CommandCompleted).
///
/// The handle is an opaque wrapper over the internal token; callers should
/// not assume any structure beyond equality and hashing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CommandHandle(pub(crate) Token);

impl CommandHandle {
    /// The raw token value, for diagnostics only.
    #[must_use]
    pub fn token(self) -> Token {
        self.0
    }
}

/// The completion status of a submitted command.
///
/// Note that [`CommandOutcome::Running`] means the command itself is
/// **complete** — GDB accepted it and the target is now running. The
/// eventual `*stopped` event arrives later as an independent
/// [`Event::Stopped`](crate::event::Event::Stopped), not as another
/// completion on this handle. Treating `Running` as "still pending" and
/// waiting for the stop is the single biggest deadlock trap in MI frontends;
/// framewalk's design enforces the distinction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutcome {
    /// `^done[,results...]`.
    Done(Vec<(String, Value)>),
    /// `^running` — command accepted, target running. Command is complete.
    Running,
    /// `^connected[,results...]` — GDB connected to a remote target.
    Connected(Vec<(String, Value)>),
    /// `^error,msg=...[,code=...]`.
    Error { msg: String, code: Option<String> },
    /// `^exit` — GDB is shutting down.
    Exit,
}
