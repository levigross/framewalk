//! The [`TransportHandle`] — the async API callers interact with.
//!
//! A handle is created by [`crate::subprocess::spawn`], holds the spawned
//! GDB child plus the shared state used by the background tasks, and
//! exposes three operations:
//!
//! - [`submit`](TransportHandle::submit): send a command and await its
//!   completion. Internally allocates a [`oneshot`] pair, stores the
//!   sender in the pending map, encodes the command's bytes via the
//!   sans-IO connection, forwards the bytes to the writer task, and
//!   awaits the receiver.
//! - [`subscribe`](TransportHandle::subscribe): obtain a
//!   [`broadcast::Receiver`] that emits every [`Event`] the connection
//!   produces. Use this from any long-lived consumer that needs to react
//!   to target state changes.
//! - [`next_stop_after`](TransportHandle::next_stop_after): wait for the
//!   first `*stopped` event observed after a previously captured event
//!   cursor. This is the race-free primitive for "trigger command, then
//!   wait for the resulting stop."
//! - [`shutdown`](TransportHandle::shutdown): queue `-gdb-exit` and wait
//!   for the child process to actually terminate, then return its exit
//!   status.

use std::sync::Arc;
use std::time::Duration;

use framewalk_mi_codec::MiCommand;
use framewalk_mi_protocol::{
    BreakpointRegistry, CommandOutcome, CommandRequest, Event, FeatureSet, FrameRegistry,
    MiVersion, StoppedEvent, TargetState, ThreadRegistry, VarObjRegistry,
};
use tokio::process::Child;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{timeout, timeout_at, Instant};
use tracing::{debug, warn};

use crate::error::TransportError;
use crate::shared::{EventSeq, SharedState};

/// Async transport handle. Cheaply clonable via `subscribe` for the
/// read-only event stream; the command-submission surface is unique per
/// handle.
pub struct TransportHandle {
    pub(crate) shared: Arc<SharedState>,
    pub(crate) write_tx: mpsc::Sender<Vec<u8>>,
    pub(crate) child: Child,
    pub(crate) reader_task: JoinHandle<()>,
    pub(crate) writer_task: JoinHandle<()>,
    pub(crate) stderr_task: JoinHandle<()>,
}

impl std::fmt::Debug for TransportHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The inner SharedState carries oneshot senders that do not impl
        // Debug and are uninteresting to print anyway; show only the
        // operator-visible process id.
        f.debug_struct("TransportHandle")
            .field("child_id", &self.child.id())
            .finish_non_exhaustive()
    }
}

impl TransportHandle {
    /// Submit a command and wait for its completion.
    ///
    /// The returned [`CommandOutcome`] corresponds to the `^done` /
    /// `^running` / `^connected` / `^error` / `^exit` class of the
    /// result record GDB produces.
    ///
    /// **Note on `CommandOutcome::Running`:** per the plan's load-bearing
    /// design decision, `^running` completes the `submit` future
    /// immediately — the eventual `*stopped` async record arrives later
    /// as an independent event, not as another completion on this future.
    /// Callers that need the resulting stop should capture an
    /// [`event_cursor`](Self::event_cursor) before submitting and then
    /// wait with [`next_stop_after`](Self::next_stop_after), or use
    /// [`current_or_next_stop`](Self::current_or_next_stop) when they
    /// want "return the current stop if already halted, otherwise wait".
    ///
    /// # Errors
    ///
    /// Returns [`TransportError::Exited`] if GDB disappears (stdout EOF
    /// or stdin closed) before the result arrives. Returns
    /// [`TransportError::Io`] if the write to stdin fails.
    pub async fn submit(&self, command: MiCommand) -> Result<CommandOutcome, TransportError> {
        if !self.shared.is_reader_alive() {
            return Err(TransportError::Exited);
        }

        let (tx, rx) = oneshot::channel();
        let (handle, outbound) = {
            let mut conn = self.lock_connection();
            let h = conn.submit(CommandRequest::new(command));
            let bytes = Self::drain_outbound(&mut conn)?;
            (h, bytes)
        };
        self.register_pending(handle, tx);
        self.send_or_rollback(handle, outbound).await?;
        rx.await.map_err(|_| TransportError::Exited)
    }

    /// Submit a raw MI command line verbatim, bypassing the structured
    /// encoder. See [`Connection::submit_raw`] for semantics.
    pub async fn submit_raw(&self, raw_line: &str) -> Result<CommandOutcome, TransportError> {
        if !self.shared.is_reader_alive() {
            return Err(TransportError::Exited);
        }

        let (tx, rx) = oneshot::channel();
        let (handle, outbound) = {
            let mut conn = self.lock_connection();
            let h = conn.submit_raw(raw_line);
            let bytes = Self::drain_outbound(&mut conn)?;
            (h, bytes)
        };
        self.register_pending(handle, tx);
        self.send_or_rollback(handle, outbound).await?;
        rx.await.map_err(|_| TransportError::Exited)
    }

    /// Subscribe to the event stream. Every call returns a fresh
    /// [`broadcast::Receiver`] starting at the current point in the
    /// stream; a subscriber created after some events have been
    /// produced will miss them. Callers that need the full history
    /// should subscribe before spawning any work that consumes events.
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.shared.events_tx.subscribe()
    }

    /// Return the current event cursor. Any event observed after this call
    /// is guaranteed to have a strictly larger sequence number.
    #[must_use]
    pub fn event_cursor(&self) -> EventSeq {
        self.shared.event_cursor()
    }

    /// Return the earliest retained event sequence number, if any.
    #[must_use]
    pub fn earliest_event_seq(&self) -> Option<EventSeq> {
        self.shared.earliest_event_seq()
    }

    /// Return every retained event strictly after `after_seq`.
    #[must_use]
    pub fn events_after(&self, after_seq: EventSeq) -> Vec<(EventSeq, Event)> {
        self.shared.events_after(after_seq)
    }

    /// Return the most recently retained event, if any.
    #[must_use]
    pub fn latest_event(&self) -> Option<(EventSeq, Event)> {
        self.shared.latest_event()
    }

    /// Inject an `Event::Log` into the journal as if it were a real
    /// `@` log-stream record from GDB. Intended for the MCP layer to
    /// surface server-side advisories (auto-downgrades, target-type
    /// hints) through the same channel callers already drain.
    ///
    /// The event receives a monotonic [`EventSeq`] and wakes any
    /// cursor-based waiters just like a genuine event.
    pub fn record_synthetic_log(&self, text: String) -> EventSeq {
        self.shared.record_event(Event::Log(text))
    }

    /// Whether the reader task is still alive and able to observe new MI
    /// events from GDB.
    #[must_use]
    pub fn is_reader_alive(&self) -> bool {
        self.shared.is_reader_alive()
    }

    /// Return the current stop immediately if the target is already
    /// stopped, otherwise wait for the next `*stopped` event after the
    /// current cursor.
    pub async fn current_or_next_stop(
        &self,
        timeout: Duration,
    ) -> Result<Option<(EventSeq, StoppedEvent)>, TransportError> {
        if self.snapshot().target.is_stopped() {
            if let Some(stopped) = self.shared.latest_stopped() {
                return Ok(Some(stopped));
            }
        }

        self.next_stop_after(self.event_cursor(), timeout).await
    }

    /// Wait for the first `*stopped` event that occurs strictly after
    /// `after_seq`. Returns `Ok(None)` on timeout and `Err(Exited)` if the
    /// reader task has already terminated and no further events can arrive.
    pub async fn next_stop_after(
        &self,
        after_seq: EventSeq,
        timeout: Duration,
    ) -> Result<Option<(EventSeq, StoppedEvent)>, TransportError> {
        let deadline = Instant::now() + timeout;

        loop {
            if let Some(stopped) = self.shared.find_stop_after(after_seq) {
                return Ok(Some(stopped));
            }
            if !self.shared.is_reader_alive() {
                return Err(TransportError::Exited);
            }

            let notified = self.shared.event_notify.notified();

            if let Some(stopped) = self.shared.find_stop_after(after_seq) {
                return Ok(Some(stopped));
            }
            if !self.shared.is_reader_alive() {
                return Err(TransportError::Exited);
            }

            match timeout_at(deadline, notified).await {
                Ok(()) => {}
                Err(_elapsed) => return Ok(None),
            }
        }
    }

    /// Gracefully shut down: queue `-gdb-exit`, wait for the `^exit`
    /// result (with a short timeout), then wait for the child process
    /// to terminate and return its exit status.
    ///
    /// If GDB does not respond to `-gdb-exit` within the timeout, the
    /// handle falls back to killing the process (since `Child` is
    /// configured with `kill_on_drop` anyway).
    pub async fn shutdown(mut self) -> Result<std::process::ExitStatus, TransportError> {
        debug!("shutdown: queueing -gdb-exit");
        let shutdown_timeout = Duration::from_secs(2);
        let submit_fut = self.submit(MiCommand::new("gdb-exit"));
        let submit_result = match timeout(shutdown_timeout, submit_fut).await {
            Ok(result) => result,
            Err(_elapsed) => {
                warn!("timeout waiting for -gdb-exit; killing the gdb process");
                // A start_kill error means the child has already
                // exited on its own — the subsequent `child.wait()`
                // below will collect whatever exit status it left.
                if let Err(err) = self.child.start_kill() {
                    debug!(%err, "start_kill returned error (child likely already gone)");
                }
                return self.finish_shutdown().await;
            }
        };

        match submit_result {
            Ok(outcome) => {
                debug!(?outcome, "-gdb-exit completed");
            }
            Err(err) => {
                // Typically TransportError::Exited — GDB already went
                // away on its own. That's fine for shutdown.
                debug!(%err, "-gdb-exit returned early (gdb likely already exited)");
            }
        }

        self.finish_shutdown().await
    }

    async fn finish_shutdown(mut self) -> Result<std::process::ExitStatus, TransportError> {
        // Wait for the child to actually terminate so we can return its
        // exit status. `Child::wait` consumes stdin on its way out via
        // the kill path; by this point we've already dropped stdin into
        // the writer task.
        let status = self.child.wait().await.map_err(TransportError::Io)?;

        // Closing the final sender lets the writer task observe channel
        // shutdown and exit cleanly once it has drained any buffered
        // command. Join all background tasks so task failures are surfaced
        // exactly once, here, rather than being silently detached.
        let reader_task = self.reader_task;
        let writer_task = self.writer_task;
        let stderr_task = self.stderr_task;
        drop(self.write_tx);
        Self::join_background_tasks(reader_task, writer_task, stderr_task).await;

        Ok(status)
    }

    /// Access the underlying child process id, for diagnostics.
    #[must_use]
    pub fn child_id(&self) -> Option<u32> {
        self.child.id()
    }

    /// Remember the last successfully connected `-target-select ...`
    /// command in raw MI form so reconnect logic can replay it.
    pub fn remember_target_selection_command(&self, raw_command: String) {
        self.shared.remember_target_selection_command(raw_command);
    }

    /// Return the last remembered `-target-select ...` command, if any.
    #[must_use]
    pub fn last_target_selection_command(&self) -> Option<String> {
        self.shared.last_target_selection_command()
    }

    fn lock_connection(&self) -> parking_lot::MutexGuard<'_, framewalk_mi_protocol::Connection> {
        self.shared.connection.lock()
    }

    fn drain_outbound(
        conn: &mut framewalk_mi_protocol::Connection,
    ) -> Result<Vec<u8>, TransportError> {
        let bytes = conn.outbound().to_vec();
        let len = bytes.len();
        conn.consume_outbound(len)?;
        Ok(bytes)
    }

    fn register_pending(
        &self,
        handle: framewalk_mi_protocol::CommandHandle,
        tx: oneshot::Sender<CommandOutcome>,
    ) {
        self.shared.pending.lock().insert(handle, tx);
    }

    /// Send outbound bytes to the writer task. On failure, **roll back**
    /// the pending-map entry so the oneshot sender doesn't leak — this is
    /// fix 4 from the review: registration and side effects commit
    /// together or unwind together.
    async fn send_or_rollback(
        &self,
        handle: framewalk_mi_protocol::CommandHandle,
        outbound: Vec<u8>,
    ) -> Result<(), TransportError> {
        if let Err(_send_err) = self.write_tx.send(outbound).await {
            // The writer channel is closed (GDB exited). Remove the
            // pending entry so the oneshot sender drops cleanly and the
            // caller's rx.await returns Err (which we map to Exited).
            self.shared.pending.lock().remove(&handle);
            return Err(TransportError::Exited);
        }
        Ok(())
    }

    async fn join_background_tasks(
        reader_task: JoinHandle<()>,
        writer_task: JoinHandle<()>,
        stderr_task: JoinHandle<()>,
    ) {
        for (name, task) in [
            ("reader", reader_task),
            ("writer", writer_task),
            ("stderr", stderr_task),
        ] {
            if let Err(err) = task.await {
                warn!(task = name, %err, "background task join failed");
            }
        }
    }

    /// Take a cloned snapshot of every read-only registry the sans-IO
    /// `Connection` exposes. Callers that need to serialise live state
    /// to JSON (MCP tool handlers, Scheme workers) use this to avoid
    /// holding a reference across `.await` points.
    ///
    /// The snapshot is taken under a single lock acquisition: all
    /// registries reflect the same instant. Cloning is cheap because the
    /// registries are small `BTreeMap`s of a handful of entries each.
    #[must_use]
    pub fn snapshot(&self) -> StateSnapshot {
        let conn = self.shared.connection.lock();
        StateSnapshot {
            target: conn.target_state().clone(),
            threads: conn.threads().clone(),
            frames: conn.frames().clone(),
            breakpoints: conn.breakpoints().clone(),
            varobjs: conn.varobjs().clone(),
            features: conn.features().clone(),
            mi_version: conn.mi_version(),
        }
    }
}

/// A point-in-time snapshot of every read-only registry tracked by the
/// underlying [`Connection`](framewalk_mi_protocol::Connection).
///
/// Produced by [`TransportHandle::snapshot`]. All fields are owned so
/// the snapshot outlives the lock that produced it and can be moved
/// across `.await` points, serialised to JSON, or inspected by
/// long-running tool handlers without blocking the reader task.
#[derive(Debug, Clone)]
pub struct StateSnapshot {
    pub target: TargetState,
    pub threads: ThreadRegistry,
    pub frames: FrameRegistry,
    pub breakpoints: BreakpointRegistry,
    pub varobjs: VarObjRegistry,
    pub features: FeatureSet,
    pub mi_version: MiVersion,
}
