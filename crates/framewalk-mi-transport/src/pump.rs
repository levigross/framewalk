//! Background tokio tasks that pump bytes between GDB and the sans-IO
//! [`Connection`](framewalk_mi_protocol::Connection) inside
//! [`SharedState`](crate::shared::SharedState).
//!
//! Three tasks, each a single loop:
//!
//! - **Reader**: reads stdout chunks, feeds them to the connection,
//!   drains events, fires pending oneshots on [`Event::CommandCompleted`],
//!   and broadcasts every event to subscribers.
//! - **Writer**: waits for outbound byte buffers on an `mpsc::Receiver`
//!   and writes them to stdin. Each buffer is a complete encoded command,
//!   so a single `write_all` + `flush` does the whole thing.
//! - **Stderr logger**: reads stderr chunks and logs them at `debug` via
//!   `tracing`. GDB uses stderr for remote-target diagnostics and
//!   occasional warnings; they're useful during development but not
//!   part of the MI contract.

use std::sync::Arc;

use framewalk_mi_protocol::Event;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{ChildStderr, ChildStdin, ChildStdout};
use tokio::sync::mpsc;
use tracing::{debug, error, trace, warn};

use crate::error::TransportError;
use crate::shared::SharedState;

/// Size of the stdout read buffer for each `stdout.read()` call. 8 KiB is
/// large enough that typical MI lines (tens to hundreds of bytes) usually
/// arrive in one chunk, and small enough that a misbehaving GDB cannot
/// starve other tokio tasks.
const STDOUT_READ_BUF_SIZE: usize = 8 * 1024;

/// Maximum bytes the framer is allowed to buffer without seeing a newline.
/// If GDB emits a line longer than this (or hangs mid-line), the reader
/// task disconnects to prevent unbounded memory growth. 16 MiB is generous
/// enough for any realistic MI response (even large memory dumps), while
/// still bounding memory in pathological cases.
const MAX_PENDING_BYTES: usize = 16 * 1024 * 1024;

/// RAII guard that drains `shared.pending` when dropped.
///
/// The reader task is the **only** actor that ever fires the
/// `oneshot::Sender`s stored in `SharedState::pending`. If the reader
/// exits — clean EOF on GDB's stdout, a framer overflow, a protocol
/// error, or a panic unwind — any `oneshot::Sender` still in the map
/// will never fire, and every caller parked in
/// [`TransportHandle::submit`] at `rx.await` will hang forever.
///
/// Worse, the receivers are held by futures owned by callers who
/// hold `&TransportHandle` — that borrow keeps the `TransportHandle`
/// alive, which keeps `Arc<SharedState>` alive, which keeps the
/// `HashMap<_, oneshot::Sender>` alive with the stuck senders inside.
/// A pure ownership-based fix cannot break the cycle because the
/// borrowing side is the stuck side.
///
/// `PendingDrainGuard` breaks it by explicitly clearing the pending
/// map as part of the reader task's stack unwind. Dropping each
/// `oneshot::Sender` wakes its receiver with `RecvError`, which
/// [`TransportHandle::submit`] already maps to
/// [`TransportError::Exited`] — the correct "GDB is gone" semantic.
///
/// `parking_lot::Mutex` does not poison, so `.lock().clear()` is
/// safe even during a panic unwind through the reader loop.
struct PendingDrainGuard<'a> {
    shared: &'a Arc<SharedState>,
}

impl Drop for PendingDrainGuard<'_> {
    fn drop(&mut self) {
        self.shared.mark_reader_exited();
        let mut pending = self.shared.pending.lock();
        if !pending.is_empty() {
            debug!(
                count = pending.len(),
                "reader task exiting; draining pending command senders"
            );
            pending.clear();
        }
    }
}

/// Read stdout forever, feed the sans-IO core, fan out events. Returns
/// when GDB closes stdout (clean exit) or an I/O error occurs.
pub(crate) async fn run_reader(
    shared: Arc<SharedState>,
    mut stdout: ChildStdout,
) -> Result<(), TransportError> {
    // Install the drain guard before the loop: every exit path from
    // this function (EOF return, error return, panic unwind) must run
    // it so in-flight `submit()` callers wake with `Exited` instead of
    // deadlocking.
    let _drain_guard = PendingDrainGuard { shared: &shared };

    let mut read_buf = vec![0u8; STDOUT_READ_BUF_SIZE];
    loop {
        let n = stdout.read(&mut read_buf).await?;
        if n == 0 {
            debug!("gdb stdout closed (EOF)");
            return Ok(());
        }
        trace!(bytes = n, "read from gdb stdout");

        // Feed bytes through the sans-IO core and drain every event the
        // connection can produce right now. The lock is held only for the
        // duration of the drain; we collect events into a local Vec so we
        // can release the lock before fanning out.
        let events = {
            let mut conn = shared.connection.lock();
            conn.receive_bytes(&read_buf[..n])?;

            // Guard against unbounded buffer growth: if GDB emits a
            // very long line (or hangs mid-line), the framer buffer
            // grows without bound. Disconnect before OOM.
            let pending = conn.pending_bytes();
            if pending > MAX_PENDING_BYTES {
                error!(
                    pending_bytes = pending,
                    limit = MAX_PENDING_BYTES,
                    "framer buffer exceeded limit; disconnecting"
                );
                return Err(TransportError::BufferOverflow {
                    pending_bytes: pending,
                    limit: MAX_PENDING_BYTES,
                });
            }

            let mut events = Vec::new();
            while let Some(event) = conn.poll_event() {
                events.push(event);
            }
            events
        };

        for event in events {
            dispatch_event(&shared, event);
        }
    }
}

/// Dispatch a single event: fire any matching pending-command oneshot,
/// then broadcast the event to all subscribers.
fn dispatch_event(shared: &Arc<SharedState>, event: Event) {
    // If it's a command completion, take the oneshot out of the pending
    // map and fire it with a clone of the outcome. The event itself is
    // still forwarded to broadcast subscribers so higher layers can
    // observe completions holistically (e.g., for logging).
    if let Event::CommandCompleted { handle, outcome } = &event {
        let maybe_sender = shared.pending.lock().remove(handle);
        if let Some(tx) = maybe_sender {
            // A send error here just means the caller dropped its
            // receiver (timed out). That's expected, not a fault.
            tx.send(outcome.clone()).ok();
        } else {
            trace!(?handle, "no pending sender for completed command");
        }
    }

    shared.record_event(event.clone());

    // `broadcast::Sender::send` returns `Err(SendError)` when there
    // are no active receivers, which is the common case during tests
    // and early startup — nothing to report.
    shared.events_tx.send(event).ok();
}

/// Writer task: drain outbound byte buffers from the mpsc channel and
/// write them to GDB's stdin. Terminates when all senders are dropped.
pub(crate) async fn run_writer(
    mut stdin: ChildStdin,
    mut write_rx: mpsc::Receiver<Vec<u8>>,
) -> Result<(), TransportError> {
    while let Some(bytes) = write_rx.recv().await {
        trace!(bytes = bytes.len(), "writing to gdb stdin");
        stdin.write_all(&bytes).await?;
        stdin.flush().await?;
    }
    debug!("writer channel closed; writer task exiting");
    Ok(())
}

/// Stderr logger task: read GDB's stderr line by line and emit each line
/// at `debug` level via `tracing`. Not parsed — stderr is not part of the
/// MI contract, so we just surface it for operator diagnosis.
pub(crate) async fn run_stderr_logger(stderr: ChildStderr) -> Result<(), TransportError> {
    let mut reader = BufReader::new(stderr).lines();
    while let Some(line) = reader.next_line().await? {
        if line.is_empty() {
            continue;
        }
        // Use `warn` for lines that look like actual GDB warnings, and
        // `debug` for everything else, so operators can tune via
        // RUST_LOG without drowning in noise.
        if line.contains("warning:") || line.contains("error:") {
            warn!(target: "framewalk_mi_transport::gdb_stderr", "{line}");
        } else {
            debug!(target: "framewalk_mi_transport::gdb_stderr", "{line}");
        }
    }
    debug!("gdb stderr closed (EOF)");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    use framewalk_mi_codec::MiCommand;
    use framewalk_mi_protocol::{CommandRequest, Connection};
    use tokio::sync::oneshot;

    /// Dropping a `PendingDrainGuard` must wake every parked
    /// `submit()` caller whose `oneshot::Sender` is still sitting in
    /// `SharedState::pending`.  The mechanism: clearing the map drops
    /// each `Sender`, which delivers `RecvError` to the matching
    /// `Receiver`, which `TransportHandle::submit` then maps to
    /// `TransportError::Exited`.
    ///
    /// This is the behaviour-level test for the deadlock fix: without
    /// the guard, a reader-task exit on GDB stdout EOF leaves the
    /// senders orphaned and the receivers hang forever.
    #[test]
    fn drain_guard_wakes_parked_senders() {
        let shared = Arc::new(SharedState::new(Connection::new()));

        // Install two fake pending senders under real CommandHandles
        // (constructed via the connection so we don't need to reach
        // into protocol-crate internals).
        let (tx1, mut rx1) = oneshot::channel();
        let (tx2, mut rx2) = oneshot::channel();
        let (h1, h2) = {
            let mut conn = shared.connection.lock();
            let h1 = conn.submit(CommandRequest::new(MiCommand::new("gdb-version")));
            let h2 = conn.submit(CommandRequest::new(MiCommand::new("list-features")));
            (h1, h2)
        };
        {
            let mut pending = shared.pending.lock();
            pending.insert(h1, tx1);
            pending.insert(h2, tx2);
        }
        assert_eq!(shared.pending.lock().len(), 2);

        // Scope a drain guard and drop it — simulates the reader
        // task returning on EOF (or any other exit path).
        {
            let _guard = PendingDrainGuard { shared: &shared };
        }

        // The pending map must be empty and both receivers must see
        // the sender-closed signal.
        assert!(shared.pending.lock().is_empty());
        assert!(
            matches!(rx1.try_recv(), Err(oneshot::error::TryRecvError::Closed)),
            "rx1 should observe its sender was dropped"
        );
        assert!(
            matches!(rx2.try_recv(), Err(oneshot::error::TryRecvError::Closed)),
            "rx2 should observe its sender was dropped"
        );
    }
}
