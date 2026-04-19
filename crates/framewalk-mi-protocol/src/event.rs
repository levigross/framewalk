//! Events emitted by [`Connection::poll_event`](crate::connection::Connection::poll_event).

use framewalk_mi_codec::{Record, Value};

use crate::command::{CommandHandle, CommandOutcome};
use crate::error::ParseFailure;
use crate::state::frames::Frame;
use crate::state::target::StoppedReason;
use crate::state::threads::ThreadId;

/// A semantic event produced by a [`Connection`](crate::connection::Connection).
///
/// Events are pulled with `poll_event` and arrive in wire order. Async
/// records are never reordered to "group by command" — order is strictly
/// wall-clock.
#[derive(Debug, Clone)]
pub enum Event {
    /// A submitted command completed. `handle` identifies which;
    /// `outcome` carries the result.
    CommandCompleted {
        handle: CommandHandle,
        outcome: CommandOutcome,
    },

    /// `*stopped` — the target halted. Fields are parsed from the wire
    /// at ingress so callers get typed data, not raw MI.
    Stopped(StoppedEvent),

    /// `*running` — the target is executing.
    Running(RunningEvent),

    /// A `=class,...` notification.
    Notify(NotifyEvent),

    /// A `+class,...` status record.
    Status(NotifyEvent),

    /// `~"..."` — console output for display.
    Console(String),

    /// `@"..."` — target program stdout (remote targets only).
    TargetOutput(String),

    /// `&"..."` — GDB internal log.
    Log(String),

    /// The `(gdb)` prompt closed a response group.
    GroupClosed,

    /// A record that parsed but doesn't fit the above categories.
    Unknown(Record),

    /// A wire line failed to parse.
    ParseError(ParseFailure),
}

/// Typed payload of a `*stopped` async record.
#[derive(Debug, Clone)]
pub struct StoppedEvent {
    /// Why the target stopped, parsed from the `reason` field.
    pub reason: Option<StoppedReason>,
    /// Which thread stopped.
    pub thread: Option<ThreadId>,
    /// The innermost frame at the stop point, if GDB included one.
    pub frame: Option<Frame>,
    /// All raw results from the wire, preserved as an escape hatch for
    /// fields framewalk doesn't parse into typed form yet.
    pub raw: Vec<(String, Value)>,
}

/// Typed payload of a `*running` async record.
#[derive(Debug, Clone)]
pub struct RunningEvent {
    /// Which thread transitioned to running. `None` if GDB didn't
    /// report a thread-id (unusual).
    pub thread: Option<ThreadId>,
}

/// Payload of a `=class,...` or `+class,...` record.
#[derive(Debug, Clone)]
pub struct NotifyEvent {
    /// The async class name (e.g. `"thread-created"`).
    pub class: String,
    /// Raw results — notifications carry diverse schemas per class,
    /// so we keep the raw shape and let consumers pick fields by name.
    pub results: Vec<(String, Value)>,
}
