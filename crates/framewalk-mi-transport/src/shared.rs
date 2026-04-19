//! Shared state passed between the [`TransportHandle`](crate::handle::TransportHandle)
//! and the background pump tasks.
//!
//! Three things are shared:
//! 1. The sans-IO [`Connection`] state machine, behind a `parking_lot::Mutex`.
//!    Critical sections are short (push bytes, drain events, take outbound
//!    bytes) and do not `await`, so a synchronous mutex is the right tool.
//! 2. The pending-oneshot map, keyed by [`CommandHandle`]. When `submit`
//!    is called, a [`oneshot::Sender`] is inserted under the new handle;
//!    the reader task removes it when the corresponding
//!    `Event::CommandCompleted` arrives and fires the oneshot so the
//!    caller's `submit` future completes.
//! 3. The broadcast sender for fan-out of all events to subscribers (MCP
//!    tool handlers, Scheme workers, debug loggers). The reader task is
//!    the only producer.
//! 4. A bounded sequenced event journal. Broadcast is still useful for
//!    long-lived subscribers, but "wait for the next stop after X" is a
//!    causal query and should not depend on a best-effort live stream.

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};

use framewalk_mi_protocol::{CommandHandle, CommandOutcome, Connection, Event, StoppedEvent};
use parking_lot::Mutex;
use tokio::sync::{broadcast, oneshot, Notify};

/// Monotonic sequence assigned to every event the reader task observes.
pub type EventSeq = u64;

#[derive(Debug, Clone)]
struct SequencedEvent {
    seq: EventSeq,
    event: Event,
}

#[derive(Debug)]
struct EventJournal {
    next_seq: EventSeq,
    entries: VecDeque<SequencedEvent>,
    latest_stopped: Option<(EventSeq, StoppedEvent)>,
}

impl EventJournal {
    fn new() -> Self {
        Self {
            next_seq: 1,
            entries: VecDeque::new(),
            latest_stopped: None,
        }
    }

    fn cursor(&self) -> EventSeq {
        self.next_seq.saturating_sub(1)
    }

    fn push(&mut self, event: Event, capacity: usize) -> EventSeq {
        let seq = self.next_seq;
        self.next_seq = self.next_seq.saturating_add(1);

        if let Event::Stopped(stopped) = &event {
            self.latest_stopped = Some((seq, stopped.clone()));
        }

        self.entries.push_back(SequencedEvent { seq, event });
        while self.entries.len() > capacity {
            self.entries.pop_front();
        }

        seq
    }

    fn find_stop_after(&self, after_seq: EventSeq) -> Option<(EventSeq, StoppedEvent)> {
        self.entries.iter().find_map(|entry| match &entry.event {
            Event::Stopped(stopped) if entry.seq > after_seq => Some((entry.seq, stopped.clone())),
            _ => None,
        })
    }

    fn latest_stopped(&self) -> Option<(EventSeq, StoppedEvent)> {
        self.latest_stopped.clone()
    }

    fn events_after(&self, after_seq: EventSeq) -> Vec<(EventSeq, Event)> {
        self.entries
            .iter()
            .filter(|entry| entry.seq > after_seq)
            .map(|entry| (entry.seq, entry.event.clone()))
            .collect()
    }

    fn latest_event(&self) -> Option<(EventSeq, Event)> {
        self.entries
            .back()
            .map(|entry| (entry.seq, entry.event.clone()))
    }

    fn earliest_seq(&self) -> Option<EventSeq> {
        self.entries.front().map(|entry| entry.seq)
    }
}

/// The state shared between the transport handle and its background tasks.
pub(crate) struct SharedState {
    /// The sans-IO protocol state machine. Uses `parking_lot::Mutex`
    /// rather than `std::sync::Mutex` because: (a) critical sections are
    /// short, CPU-bound, and never hold across `.await`, so a sync mutex
    /// is correct; (b) `parking_lot` does not poison, which is the right
    /// choice — if a panic occurs mid-mutation the state is corrupt and
    /// recovery is hiding a bug.
    pub(crate) connection: Mutex<Connection>,

    /// Pending command completions. Keyed by the handle returned from
    /// `Connection::submit`; the sender is signalled when the matching
    /// `Event::CommandCompleted` arrives.
    pub(crate) pending: Mutex<HashMap<CommandHandle, oneshot::Sender<CommandOutcome>>>,

    /// Fan-out channel for all events. Subscribers obtain a receiver via
    /// `TransportHandle::subscribe`; the reader task is the sole producer.
    pub(crate) events_tx: broadcast::Sender<Event>,

    /// Bounded event history keyed by monotonically increasing sequence
    /// numbers so callers can wait for "the next stop after cursor X"
    /// without racing a live subscription boundary.
    journal: Mutex<EventJournal>,

    /// The most recent successfully connected `-target-select ...`
    /// command, stored in raw MI form so reconnect can replay the exact
    /// selection semantics regardless of which API surface issued it.
    last_target_selection_command: Mutex<Option<String>>,

    /// Notifies waiters that either a new event was journaled or the
    /// reader task exited. Waiters re-check the journal after waking.
    pub(crate) event_notify: Notify,

    /// Whether the reader task still owns GDB stdout. Once false, no new
    /// events will ever arrive and cursor-based waiters should wake with
    /// `TransportError::Exited`.
    reader_alive: AtomicBool,
}

impl SharedState {
    /// Capacity of the broadcast channel. Subscribers that fall more than
    /// this many events behind will receive `RecvError::Lagged` on their
    /// next `recv`. Sized generously so subscribers don't drop events
    /// during a burst of `=library-loaded` notifications at startup.
    pub(crate) const EVENT_BROADCAST_CAPACITY: usize = 1024;
    /// Retained event history. Large enough to cover short bursts of async
    /// traffic while still bounding memory if a client stops draining.
    pub(crate) const EVENT_JOURNAL_CAPACITY: usize = 1024;

    pub(crate) fn new(connection: Connection) -> Self {
        let (events_tx, _) = broadcast::channel(Self::EVENT_BROADCAST_CAPACITY);
        Self {
            connection: Mutex::new(connection),
            pending: Mutex::new(HashMap::new()),
            events_tx,
            journal: Mutex::new(EventJournal::new()),
            last_target_selection_command: Mutex::new(None),
            event_notify: Notify::new(),
            reader_alive: AtomicBool::new(true),
        }
    }

    pub(crate) fn is_reader_alive(&self) -> bool {
        self.reader_alive.load(Ordering::Acquire)
    }

    pub(crate) fn mark_reader_exited(&self) {
        self.reader_alive.store(false, Ordering::Release);
        self.event_notify.notify_waiters();
    }

    pub(crate) fn event_cursor(&self) -> EventSeq {
        self.journal.lock().cursor()
    }

    pub(crate) fn record_event(&self, event: Event) -> EventSeq {
        let seq = self
            .journal
            .lock()
            .push(event, Self::EVENT_JOURNAL_CAPACITY);
        self.event_notify.notify_waiters();
        seq
    }

    pub(crate) fn latest_stopped(&self) -> Option<(EventSeq, StoppedEvent)> {
        self.journal.lock().latest_stopped()
    }

    pub(crate) fn find_stop_after(&self, after_seq: EventSeq) -> Option<(EventSeq, StoppedEvent)> {
        self.journal.lock().find_stop_after(after_seq)
    }

    pub(crate) fn events_after(&self, after_seq: EventSeq) -> Vec<(EventSeq, Event)> {
        self.journal.lock().events_after(after_seq)
    }

    pub(crate) fn latest_event(&self) -> Option<(EventSeq, Event)> {
        self.journal.lock().latest_event()
    }

    pub(crate) fn earliest_event_seq(&self) -> Option<EventSeq> {
        self.journal.lock().earliest_seq()
    }

    pub(crate) fn remember_target_selection_command(&self, raw_command: String) {
        *self.last_target_selection_command.lock() = Some(raw_command);
    }

    pub(crate) fn last_target_selection_command(&self) -> Option<String> {
        self.last_target_selection_command.lock().clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stop(tag: &str) -> StoppedEvent {
        StoppedEvent {
            reason: None,
            thread: None,
            frame: None,
            raw: vec![(
                "reason".into(),
                framewalk_mi_codec::Value::Const(tag.into()),
            )],
        }
    }

    // ---- Existing: core stop ordering ----

    #[test]
    fn event_journal_returns_first_stop_after_cursor() {
        let shared = SharedState::new(Connection::new());
        let before = shared.event_cursor();

        let stop_one = stop("one");
        let stop_two = stop("two");

        let seq_one = shared.record_event(Event::Stopped(stop_one.clone()));
        shared.record_event(Event::Console("ignored".into()));
        let seq_two = shared.record_event(Event::Stopped(stop_two));

        let observed = shared
            .find_stop_after(before)
            .expect("first stop after cursor should be retained");
        assert_eq!(observed.0, seq_one);
        assert_eq!(observed.1.raw, stop_one.raw);

        let latest = shared.latest_stopped().expect("latest stop should exist");
        assert_eq!(latest.0, seq_two);
    }

    #[test]
    fn remembers_last_target_selection_command() {
        let shared = SharedState::new(Connection::new());
        assert_eq!(shared.last_target_selection_command(), None);

        shared.remember_target_selection_command("-target-select remote :3333".to_string());
        assert_eq!(
            shared.last_target_selection_command().as_deref(),
            Some("-target-select remote :3333")
        );
    }

    // ---- Cursor and sequence invariants ----

    #[test]
    fn cursor_starts_at_zero_and_advances_monotonically() {
        let shared = SharedState::new(Connection::new());
        assert_eq!(shared.event_cursor(), 0);

        let s1 = shared.record_event(Event::Console("a".into()));
        let s2 = shared.record_event(Event::Console("b".into()));
        let s3 = shared.record_event(Event::Log("c".into()));
        assert_eq!(s1, 1);
        assert_eq!(s2, 2);
        assert_eq!(s3, 3);
        assert_eq!(shared.event_cursor(), 3);
    }

    #[test]
    fn empty_journal_has_no_earliest_or_latest() {
        let shared = SharedState::new(Connection::new());
        assert!(shared.earliest_event_seq().is_none());
        assert!(shared.latest_event().is_none());
        assert!(shared.latest_stopped().is_none());
        assert!(shared.events_after(0).is_empty());
    }

    #[test]
    fn find_stop_after_current_cursor_is_none() {
        // No Stopped events yet, so find_stop_after returns None regardless
        // of cursor position.
        let shared = SharedState::new(Connection::new());
        shared.record_event(Event::Console("noise".into()));
        assert!(shared.find_stop_after(0).is_none());
        assert!(shared.find_stop_after(shared.event_cursor()).is_none());
    }

    #[test]
    fn find_stop_after_skips_non_stopped_events_before_the_stop() {
        let shared = SharedState::new(Connection::new());
        shared.record_event(Event::Console("noise".into()));
        shared.record_event(Event::Log("more noise".into()));
        let stopped_seq = shared.record_event(Event::Stopped(stop("target")));
        let got = shared.find_stop_after(0).unwrap();
        assert_eq!(got.0, stopped_seq);
    }

    #[test]
    fn find_stop_after_returns_strictly_greater_seq() {
        let shared = SharedState::new(Connection::new());
        let s1 = shared.record_event(Event::Stopped(stop("first")));
        let s2 = shared.record_event(Event::Stopped(stop("second")));
        // cursor exactly at s1 should skip s1 and return s2.
        let got = shared.find_stop_after(s1).unwrap();
        assert_eq!(got.0, s2);
        // cursor at or past s2 returns None.
        assert!(shared.find_stop_after(s2).is_none());
    }

    // ---- events_after / latest_event ----

    #[test]
    fn events_after_includes_only_strictly_greater_seqs() {
        let shared = SharedState::new(Connection::new());
        shared.record_event(Event::Console("a".into()));
        let s2 = shared.record_event(Event::Console("b".into()));
        let s3 = shared.record_event(Event::Console("c".into()));
        let got = shared.events_after(s2);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, s3);
    }

    #[test]
    fn latest_event_matches_most_recent_record() {
        let shared = SharedState::new(Connection::new());
        shared.record_event(Event::Console("a".into()));
        let last_seq = shared.record_event(Event::Log("final".into()));
        let (seq, event) = shared.latest_event().unwrap();
        assert_eq!(seq, last_seq);
        assert!(matches!(event, Event::Log(ref t) if t == "final"));
    }

    // ---- Journal capacity bounds ----

    #[test]
    fn journal_evicts_oldest_when_capacity_exceeded() {
        let shared = SharedState::new(Connection::new());
        let cap = SharedState::EVENT_JOURNAL_CAPACITY;
        // Push cap+50 events. The oldest 50 should be evicted but
        // sequence numbers never repeat — earliest should advance past 50.
        for i in 0..(cap + 50) {
            shared.record_event(Event::Console(format!("e{i}")));
        }
        let earliest = shared.earliest_event_seq().unwrap();
        assert!(
            earliest > 50,
            "earliest seq {earliest} should be past the evicted window"
        );
        // Cursor always tracks the highest-assigned seq.
        assert_eq!(shared.event_cursor(), (cap + 50) as u64);
    }

    // ---- Reader-alive flag ----

    #[test]
    fn reader_alive_starts_true_and_flips_on_mark_exited() {
        let shared = SharedState::new(Connection::new());
        assert!(shared.is_reader_alive());
        shared.mark_reader_exited();
        assert!(!shared.is_reader_alive());
    }

    // ---- Synthetic log path (used by record_synthetic_log in handle.rs) ----

    #[test]
    fn synthetic_log_event_is_journaled_with_monotonic_seq() {
        let shared = SharedState::new(Connection::new());
        let a = shared.record_event(Event::Log("synthetic a".into()));
        let b = shared.record_event(Event::Log("synthetic b".into()));
        assert!(b > a);
        let (seq, event) = shared.latest_event().unwrap();
        assert_eq!(seq, b);
        assert!(matches!(event, Event::Log(ref t) if t == "synthetic b"));
    }
}
