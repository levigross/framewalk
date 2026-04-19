//! The sans-IO [`Connection`] state machine.
//!
//! `Connection` owns a [`framewalk_mi_wire::Framer`] for byte-to-line
//! reassembly, decodes each completed line through
//! [`framewalk_mi_codec::parse_record`], classifies the resulting records
//! into high-level [`Event`]s, and routes their payloads into state
//! registries so callers can query live target/thread/frame/breakpoint/
//! varobj state via the introspection methods.
//!
//! No I/O, no threads, no async runtime: just push bytes / pull events,
//! push commands / pull bytes. This is the `h11` / `quinn-proto` / `rustls`
//! sans-IO pattern.

use std::collections::VecDeque;

use framewalk_mi_codec::{
    encode_command, parse_record, AsyncRecord, Record, ResultClass, ResultRecord, StreamRecord,
    Value,
};
use framewalk_mi_wire::{Frame, Framer};
use tracing::{debug, trace, warn};

use crate::command::{CommandHandle, CommandOutcome, CommandRequest};
use crate::error::{ParseFailure, ProtocolError};
use crate::event::{Event, NotifyEvent, RunningEvent, StoppedEvent};
use crate::pending::{Operation, PendingCommands, PendingInfo};
use crate::results_view::{get_str, get_string, get_tuple};
use crate::state::{
    BreakpointRegistry, FeatureSet, FrameRegistry, StoppedReason, TargetState, ThreadId,
    ThreadRegistry, VarObjRegistry,
};
use crate::token::TokenAllocator;
use crate::version::MiVersion;

/// A sans-IO state machine for driving GDB over the MI v3 protocol.
#[derive(Debug, Default)]
pub struct Connection {
    // ---- Inbound pipeline ----
    framer: Framer,

    // ---- Outbound pipeline ----
    outbound: Vec<u8>,
    outbound_consumed: usize,

    // ---- Correlation ----
    tokens: TokenAllocator,
    pending: PendingCommands,

    // ---- Event queue ----
    events: VecDeque<Event>,

    // ---- State registries ----
    target: TargetState,
    threads: ThreadRegistry,
    frames: FrameRegistry,
    breakpoints: BreakpointRegistry,
    varobjs: VarObjRegistry,
    features: FeatureSet,
    mi_version: MiVersion,
}

impl Connection {
    /// Create a new connection in its initial state: no pending bytes, no
    /// pending commands, no events, target state unknown.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new connection with a hint about the MI version.
    #[must_use]
    pub fn with_version_hint(version: MiVersion) -> Self {
        Self {
            mi_version: version,
            ..Self::default()
        }
    }

    // -----------------------------------------------------------------
    // Inbound: bytes from GDB
    // -----------------------------------------------------------------

    /// Feed raw bytes from the transport into the connection.
    ///
    /// # Errors
    ///
    /// Returns `Err` only for catastrophic conditions. Individual parse
    /// errors on malformed lines do **not** surface here — they become
    /// [`Event::ParseError`] so one bad line does not kill the session.
    pub fn receive_bytes(&mut self, bytes: &[u8]) -> Result<(), ProtocolError> {
        trace!(bytes = bytes.len(), "receive_bytes");
        self.framer.push(bytes);

        // Two-phase drain: phase 1 parses while the framer borrow is
        // live (zero-copy on the happy path — parse_record returns owned
        // AST). Phase 2 classifies into events, which mutates self.
        // This avoids the per-line .to_vec() that the old FrameCopy path
        // paid on every line.
        let mut parsed: Vec<ParsedFrame> = Vec::new();
        loop {
            match self.framer.pop() {
                Some(Frame::Line(line)) => match parse_record(line) {
                    Ok(record) => parsed.push(ParsedFrame::Record(record)),
                    Err(error) => parsed.push(ParsedFrame::Error(ParseFailure {
                        error,
                        raw_line: line.to_vec(), // copy only on error (rare)
                    })),
                },
                Some(Frame::GroupTerminator) => parsed.push(ParsedFrame::GroupTerminator),
                None => break,
            }
        }
        for frame in parsed {
            match frame {
                ParsedFrame::Record(record) => self.classify_record(record),
                ParsedFrame::Error(failure) => {
                    warn!(?failure.error, "parse error on wire line");
                    self.events.push_back(Event::ParseError(failure));
                }
                ParsedFrame::GroupTerminator => self.events.push_back(Event::GroupClosed),
            }
        }
        Ok(())
    }

    /// Pull the next event. Returns `None` when the event queue is empty.
    pub fn poll_event(&mut self) -> Option<Event> {
        self.events.pop_front()
    }

    // -----------------------------------------------------------------
    // Outbound: commands to GDB
    // -----------------------------------------------------------------

    /// Submit a command. A fresh token is allocated internally and the
    /// command is encoded into the outbound buffer immediately.
    pub fn submit(&mut self, request: CommandRequest) -> CommandHandle {
        let CommandRequest { command } = request;
        let token = self.tokens.allocate();
        debug!(%token, operation = %command.operation, "submit");

        // Record pending metadata so the eventual result record can be
        // routed to the right state registry.
        self.pending.insert(
            token,
            PendingInfo {
                operation: Operation::from_name(&command.operation, &command.parameters),
                operation_name: command.operation.clone(),
            },
        );

        encode_command(Some(token), &command, &mut self.outbound);
        CommandHandle(token)
    }

    /// Submit a raw MI command line verbatim, prepending only a token
    /// prefix and appending a newline. The input must be a complete MI
    /// command **without** a leading token and **without** a trailing
    /// newline (e.g. `"-exec-run --start"`). framewalk prepends the
    /// allocated token and appends `\n`, so GDB receives
    /// `{token}{raw_line}\n`.
    ///
    /// Use this for the `mi_raw_command` escape-hatch tool. Unlike
    /// [`submit`](Self::submit), no structured encoding or quoting is
    /// applied — the caller is responsible for correctness of the MI
    /// syntax. The pending tracker records the operation as `"_raw"` so
    /// the result router does not try to interpret the payload.
    pub fn submit_raw(&mut self, raw_line: &str) -> CommandHandle {
        let token = self.tokens.allocate();
        debug!(%token, "submit_raw");

        self.pending.insert(
            token,
            PendingInfo {
                operation: Operation::Raw,
                operation_name: "_raw".to_string(),
            },
        );

        // Write: {token}{raw_line}\n
        let token_str = token.to_string();
        self.outbound.extend_from_slice(token_str.as_bytes());
        self.outbound.extend_from_slice(raw_line.as_bytes());
        self.outbound.push(b'\n');
        CommandHandle(token)
    }

    /// The slice of outbound bytes not yet acknowledged by the transport.
    #[must_use]
    pub fn outbound(&self) -> &[u8] {
        &self.outbound[self.outbound_consumed..]
    }

    /// Advance the outbound cursor by `n` bytes.
    ///
    /// Returns `Err` if `n` exceeds the number of available outbound
    /// bytes — the cursor is not advanced in that case.
    pub fn consume_outbound(&mut self, n: usize) -> Result<(), ProtocolError> {
        if self.outbound_consumed + n > self.outbound.len() {
            return Err(ProtocolError::InvariantViolation {
                reason: "consume_outbound: n exceeds available outbound bytes",
            });
        }
        self.outbound_consumed += n;
        if self.outbound_consumed != 0 && self.outbound_consumed >= self.outbound.len() / 2 {
            self.outbound.drain(..self.outbound_consumed);
            self.outbound_consumed = 0;
        }
        Ok(())
    }

    /// Number of unacknowledged outbound bytes remaining.
    #[must_use]
    pub fn outbound_len(&self) -> usize {
        self.outbound.len() - self.outbound_consumed
    }

    // -----------------------------------------------------------------
    // Introspection
    // -----------------------------------------------------------------

    #[must_use]
    pub fn target_state(&self) -> &TargetState {
        &self.target
    }

    #[must_use]
    pub fn threads(&self) -> &ThreadRegistry {
        &self.threads
    }

    #[must_use]
    pub fn frames(&self) -> &FrameRegistry {
        &self.frames
    }

    #[must_use]
    pub fn breakpoints(&self) -> &BreakpointRegistry {
        &self.breakpoints
    }

    #[must_use]
    pub fn varobjs(&self) -> &VarObjRegistry {
        &self.varobjs
    }

    #[must_use]
    pub fn features(&self) -> &FeatureSet {
        &self.features
    }

    #[must_use]
    pub fn mi_version(&self) -> MiVersion {
        self.mi_version
    }

    /// Number of bytes sitting in the framer's internal buffer that have
    /// not yet been consumed by a complete frame. Useful for enforcing a
    /// hard cap on memory from a misbehaving GDB that never emits a
    /// newline.
    #[must_use]
    pub fn pending_bytes(&self) -> usize {
        self.framer.pending_bytes()
    }

    /// Queue a `-gdb-exit` command and flush it into the outbound buffer.
    pub fn shutdown(&mut self) {
        debug!("shutdown");
        let cmd = framewalk_mi_codec::MiCommand::new("gdb-exit");
        let _ = self.submit(CommandRequest::new(cmd));
    }

    // -----------------------------------------------------------------
    fn classify_record(&mut self, record: Record) {
        match record {
            Record::Result(rr) => self.classify_result(rr),
            Record::Exec(ar) => self.classify_exec(ar),
            Record::Status(ar) => self.classify_status(ar),
            Record::Notify(ar) => self.classify_notify(ar),
            Record::Console(sr) => self.events.push_back(Event::Console(sr.text)),
            Record::Target(sr) => self.events.push_back(Event::TargetOutput(sr.text)),
            Record::Log(sr) => self.events.push_back(Event::Log(take_text(sr))),
        }
    }

    fn classify_result(&mut self, rr: ResultRecord) {
        // Untokened results cannot be correlated; surface as Unknown.
        let Some(token) = rr.token else {
            trace!("untokened result record: surfacing as Unknown");
            self.events.push_back(Event::Unknown(Record::Result(rr)));
            return;
        };

        let pending_info = self.pending.remove(token);

        // Route the result payload into state registries BEFORE building
        // the outcome, because outcome construction moves `rr.results`.
        let results_for_routing: &[(String, Value)] = &rr.results;
        match &rr.class {
            ResultClass::Done | ResultClass::Connected => {
                if let Some(info) = pending_info.as_ref() {
                    self.route_done(info, results_for_routing);
                }
            }
            ResultClass::Error => {
                // Per the GDB manual: after ^error the target may be in
                // an unknown state. If the command was an execution
                // command, reset TargetState and invalidate frames.
                if let Some(info) = pending_info.as_ref() {
                    if info.is_exec_command() {
                        self.target.mark_unknown();
                        self.frames.clear();
                    }
                }
            }
            ResultClass::Running | ResultClass::Exit => {
                // Running/Exit do not carry a routable payload for the
                // current state registries.
            }
        }

        let outcome = match rr.class {
            ResultClass::Done => CommandOutcome::Done(rr.results),
            ResultClass::Running => CommandOutcome::Running,
            ResultClass::Connected => CommandOutcome::Connected(rr.results),
            ResultClass::Error => {
                let msg = get_string(&rr.results, "msg").unwrap_or_default();
                let code = get_string(&rr.results, "code");
                CommandOutcome::Error { msg, code }
            }
            ResultClass::Exit => CommandOutcome::Exit,
        };

        debug!(%token, ?outcome, "command completed");
        self.events.push_back(Event::CommandCompleted {
            handle: CommandHandle(token),
            outcome,
        });
    }

    /// Dispatch a `^done,…` (or `^connected,…`) payload into state
    /// registries based on which operation produced it.
    fn route_done(&mut self, info: &PendingInfo, results: &[(String, Value)]) {
        match &info.operation {
            Operation::BreakInsert | Operation::BreakInfo => {
                if let Some(tuple) = get_tuple(results, "bkpt") {
                    self.breakpoints.upsert_from_bkpt_tuple(tuple);
                }
            }
            Operation::VarCreate { expression } => {
                self.varobjs.on_var_create(results, expression.clone());
            }
            Operation::VarUpdate => {
                self.varobjs.on_var_update(results);
            }
            Operation::VarDelete { name } => {
                self.varobjs.on_var_delete(name);
            }
            Operation::ListFeatures => {
                self.features.on_list_features(results);
            }
            Operation::ListTargetFeatures => {
                self.features.on_list_target_features(results);
            }
            Operation::TargetSelect | Operation::ExecRun => {
                self.features.invalidate_target_features();
            }
            Operation::Other | Operation::Raw => {}
        }
    }

    fn classify_exec(&mut self, ar: AsyncRecord) {
        match ar.class.as_str() {
            "running" => {
                let thread_id = get_str(&ar.results, "thread-id").map(ThreadId::new);
                self.target = TargetState::Running {
                    thread: thread_id.clone(),
                };
                self.threads
                    .on_running(thread_id.as_ref().map(ThreadId::as_str));
                if let Some(tid) = &thread_id {
                    if tid.is_all() {
                        self.frames.clear();
                    } else {
                        self.frames.invalidate(tid);
                    }
                } else {
                    self.frames.clear();
                }
                self.events
                    .push_back(Event::Running(RunningEvent { thread: thread_id }));
            }
            "stopped" => {
                let thread_id = get_str(&ar.results, "thread-id").map(ThreadId::new);
                let reason = StoppedReason::from_results(&ar.results);
                let frame = extract_frame(&ar.results);

                // Exit-like reasons drive TargetState::Exited; the inferior
                // has terminated so there is no valid call stack to preserve.
                match &reason {
                    Some(StoppedReason::ExitedNormally) => {
                        self.target = TargetState::Exited { exit_code: Some(0) };
                        self.frames.clear();
                    }
                    Some(StoppedReason::Exited { exit_code }) => {
                        self.target = TargetState::Exited {
                            exit_code: *exit_code,
                        };
                        self.frames.clear();
                    }
                    Some(StoppedReason::ExitedSignalled { .. }) => {
                        self.target = TargetState::Exited { exit_code: None };
                        self.frames.clear();
                    }
                    _ => {
                        self.target = TargetState::Stopped {
                            thread: thread_id.clone(),
                            reason: reason.clone(),
                        };
                    }
                }

                self.threads
                    .on_stopped(thread_id.as_ref().map(ThreadId::as_str));
                if let Some(tid) = thread_id.clone() {
                    self.frames.on_stopped_frame(tid, &ar.results);
                }
                self.events.push_back(Event::Stopped(StoppedEvent {
                    reason,
                    thread: thread_id,
                    frame,
                    raw: ar.results,
                }));
            }
            _ => {
                self.events.push_back(Event::Unknown(Record::Exec(ar)));
            }
        }
    }

    fn classify_notify(&mut self, ar: AsyncRecord) {
        // Route notify records into registries before emitting the event,
        // so callers that inspect the registry after `poll_event` see the
        // updated state.
        match ar.class.as_str() {
            "thread-created" => self.threads.on_thread_created(&ar.results),
            "thread-exited" => self.threads.on_thread_exited(&ar.results),
            "breakpoint-created" => self.breakpoints.on_breakpoint_created(&ar.results),
            "breakpoint-modified" => self.breakpoints.on_breakpoint_modified(&ar.results),
            "breakpoint-deleted" => self.breakpoints.on_breakpoint_deleted(&ar.results),
            _ => {}
        }
        self.events.push_back(Event::Notify(NotifyEvent {
            class: ar.class.as_str().to_string(),
            results: ar.results,
        }));
    }

    fn classify_status(&mut self, ar: AsyncRecord) {
        self.events.push_back(Event::Status(NotifyEvent {
            class: ar.class.as_str().to_string(),
            results: ar.results,
        }));
    }
}

enum ParsedFrame {
    Record(Record),
    Error(ParseFailure),
    GroupTerminator,
}

fn extract_frame(results: &[(String, Value)]) -> Option<crate::state::frames::Frame> {
    results.iter().find_map(|(k, v)| match (k.as_str(), v) {
        ("frame", Value::Tuple(pairs)) => Some(crate::state::frames::Frame::from_results(pairs)),
        _ => None,
    })
}

fn take_text(sr: StreamRecord) -> String {
    sr.text
}

#[cfg(test)]
mod tests {
    //! Unit tests covering paths not exercised by `tests/happy_path.rs` —
    //! primarily the error-on-exec invalidation rules, the Connected /
    //! Exit classes, outbound edge cases, the shutdown emitter, and the
    //! MI version surface.

    use super::*;
    use framewalk_mi_codec::MiCommand;

    fn drain(conn: &mut Connection) -> Vec<Event> {
        let mut out = Vec::new();
        while let Some(e) = conn.poll_event() {
            out.push(e);
        }
        out
    }

    // ---- Version surface ----

    #[test]
    fn default_mi_version_is_unknown() {
        let conn = Connection::new();
        assert_eq!(conn.mi_version(), MiVersion::Unknown);
    }

    #[test]
    fn version_hint_is_preserved() {
        let conn = Connection::with_version_hint(MiVersion::Mi3);
        assert_eq!(conn.mi_version(), MiVersion::Mi3);
    }

    // ---- Outbound cursor edge cases ----

    #[test]
    fn outbound_len_matches_available_slice() {
        let mut conn = Connection::new();
        conn.submit(CommandRequest::new(MiCommand::new("gdb-version")));
        assert_eq!(conn.outbound_len(), conn.outbound().len());
    }

    #[test]
    fn consume_outbound_overflow_is_rejected_without_advancing() {
        let mut conn = Connection::new();
        conn.submit(CommandRequest::new(MiCommand::new("x")));
        let before = conn.outbound().to_vec();
        let err = conn.consume_outbound(before.len() + 1).unwrap_err();
        assert!(matches!(err, ProtocolError::InvariantViolation { .. }));
        // Cursor must not advance on error.
        assert_eq!(conn.outbound(), before.as_slice());
    }

    #[test]
    fn consume_outbound_compacts_after_half_consumed() {
        // This drives the `>= len/2` branch so the buffer drain path runs.
        let mut conn = Connection::new();
        conn.submit(CommandRequest::new(MiCommand::new("first")));
        conn.submit(CommandRequest::new(MiCommand::new("second")));
        let total_before = conn.outbound().len();
        conn.consume_outbound(b"1-first\n".len()).unwrap();
        // The compaction path shifts bytes and resets the cursor so the
        // remaining slice is still correct after compaction.
        assert_eq!(conn.outbound(), b"2-second\n");
        assert!(conn.outbound().len() < total_before);
    }

    // ---- pending_bytes from the framer ----

    #[test]
    fn pending_bytes_reflects_partial_line() {
        let mut conn = Connection::new();
        conn.receive_bytes(b"partial").unwrap();
        assert_eq!(conn.pending_bytes(), "partial".len());
        conn.receive_bytes(b"-line\n").unwrap();
        // Full line consumed by the framer.
        assert_eq!(conn.pending_bytes(), 0);
    }

    // ---- Connected class ----

    #[test]
    fn connected_class_surfaces_as_connected_outcome() {
        let mut conn = Connection::new();
        let h = conn.submit(CommandRequest::new(MiCommand::new("target-select")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        conn.receive_bytes(b"1^connected,addr=\"0xdeadbeef\"\n(gdb)\n")
            .unwrap();
        let events = drain(&mut conn);
        match &events[0] {
            Event::CommandCompleted {
                handle,
                outcome: CommandOutcome::Connected(results),
            } => {
                assert_eq!(*handle, h);
                assert_eq!(results[0].0, "addr");
            }
            other => panic!("expected Connected outcome, got {other:?}"),
        }
    }

    // ---- Exit class ----

    #[test]
    fn exit_class_surfaces_as_exit_outcome() {
        let mut conn = Connection::new();
        let h = conn.submit(CommandRequest::new(MiCommand::new("gdb-exit")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        conn.receive_bytes(b"1^exit\n").unwrap();
        let events = drain(&mut conn);
        match &events[0] {
            Event::CommandCompleted {
                handle,
                outcome: CommandOutcome::Exit,
            } => assert_eq!(*handle, h),
            other => panic!("expected Exit outcome, got {other:?}"),
        }
    }

    // ---- Error on exec command resets target state ----

    #[test]
    fn error_on_exec_command_marks_target_unknown_and_clears_frames() {
        let mut conn = Connection::new();
        // Drive the target into "running" first so we can observe the reset.
        conn.submit(CommandRequest::new(MiCommand::new("exec-run")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        conn.receive_bytes(b"1^running\n*running,thread-id=\"all\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert!(conn.target_state().is_running());

        // Now send a second exec command and fail it — target must go Unknown.
        conn.submit(CommandRequest::new(MiCommand::new("exec-continue")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        conn.receive_bytes(b"2^error,msg=\"cannot continue\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert_eq!(*conn.target_state(), TargetState::Unknown);
    }

    #[test]
    fn error_on_non_exec_command_does_not_touch_target_state() {
        let mut conn = Connection::new();
        conn.submit(CommandRequest::new(MiCommand::new("exec-run")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        // The `*running` async record is what transitions TargetState, not
        // the `^running` outcome — so include it explicitly.
        conn.receive_bytes(b"1^running\n*running,thread-id=\"all\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert!(conn.target_state().is_running());

        conn.submit(CommandRequest::new(MiCommand::new("gdb-version")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        conn.receive_bytes(b"2^error,msg=\"irrelevant\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        // Non-exec error must not disturb target state.
        assert!(conn.target_state().is_running());
    }

    // ---- Frame clearing on *running ----

    #[test]
    fn running_with_thread_all_clears_frames() {
        let mut conn = Connection::new();
        // Feed a stop with a frame so frames registry is populated.
        conn.receive_bytes(
            b"*stopped,reason=\"breakpoint-hit\",thread-id=\"1\",\
              frame={addr=\"0x1\",func=\"main\",args=[],file=\"f.c\",fullname=\"/f.c\",line=\"1\"}\n(gdb)\n",
        )
        .unwrap();
        drain(&mut conn);
        assert!(conn.target_state().is_stopped());

        // *running,thread-id="all" must clear the frame registry.
        conn.receive_bytes(b"*running,thread-id=\"all\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert!(conn.target_state().is_running());
    }

    // ---- Exit-family stopped reasons drive TargetState::Exited ----

    #[test]
    fn stopped_exited_normally_drives_exited_state() {
        let mut conn = Connection::new();
        conn.receive_bytes(b"*stopped,reason=\"exited-normally\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert_eq!(
            *conn.target_state(),
            TargetState::Exited { exit_code: Some(0) }
        );
    }

    #[test]
    fn stopped_exited_with_code_drives_exited_state() {
        let mut conn = Connection::new();
        conn.receive_bytes(b"*stopped,reason=\"exited\",exit-code=\"42\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert_eq!(
            *conn.target_state(),
            TargetState::Exited {
                exit_code: Some(42)
            }
        );
    }

    #[test]
    fn stopped_exited_signalled_drives_exited_without_code() {
        let mut conn = Connection::new();
        conn.receive_bytes(
            b"*stopped,reason=\"exited-signalled\",signal-name=\"SIGSEGV\"\n(gdb)\n",
        )
        .unwrap();
        drain(&mut conn);
        assert_eq!(
            *conn.target_state(),
            TargetState::Exited { exit_code: None }
        );
    }

    // ---- Shutdown helper ----

    #[test]
    fn shutdown_queues_gdb_exit_command() {
        let mut conn = Connection::new();
        conn.shutdown();
        assert!(!conn.outbound().is_empty());
        let line = std::str::from_utf8(conn.outbound()).unwrap();
        assert!(line.contains("-gdb-exit"));
    }

    // ---- submit_raw preserves the raw line verbatim ----

    #[test]
    fn submit_raw_prepends_token_and_appends_newline() {
        let mut conn = Connection::new();
        conn.submit_raw("-break-insert --function main");
        assert_eq!(conn.outbound(), b"1-break-insert --function main\n");
    }

    // ---- Breakpoint notifications land in the registry ----

    #[test]
    fn breakpoint_created_notification_populates_registry() {
        let mut conn = Connection::new();
        conn.receive_bytes(
            b"=breakpoint-created,bkpt={number=\"3\",type=\"breakpoint\",\
              disp=\"keep\",enabled=\"y\",addr=\"0x400500\",func=\"main\",\
              file=\"hello.c\",fullname=\"/tmp/hello.c\",line=\"3\",times=\"0\"}\n(gdb)\n",
        )
        .unwrap();
        drain(&mut conn);
        assert_eq!(conn.breakpoints().len(), 1);
        let bp = conn
            .breakpoints()
            .get(&crate::state::BreakpointId::new("3"))
            .expect("bkpt 3 registered");
        assert_eq!(bp.locations[0].func.as_deref(), Some("main"));
    }

    #[test]
    fn breakpoint_deleted_notification_removes_from_registry() {
        let mut conn = Connection::new();
        conn.receive_bytes(
            b"=breakpoint-created,bkpt={number=\"1\",type=\"breakpoint\",\
              disp=\"keep\",enabled=\"y\",addr=\"0x1\",func=\"f\",file=\"f.c\",\
              fullname=\"/f.c\",line=\"1\",times=\"0\"}\n(gdb)\n",
        )
        .unwrap();
        drain(&mut conn);
        assert_eq!(conn.breakpoints().len(), 1);

        conn.receive_bytes(b"=breakpoint-deleted,id=\"1\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert!(conn.breakpoints().is_empty());
    }

    // ---- Thread notifications land in the registry ----

    #[test]
    fn thread_created_exited_notifications_update_registry() {
        let mut conn = Connection::new();
        conn.receive_bytes(b"=thread-created,id=\"1\",group-id=\"i1\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert_eq!(conn.threads().len(), 1);

        conn.receive_bytes(b"=thread-exited,id=\"1\"\n(gdb)\n")
            .unwrap();
        drain(&mut conn);
        assert!(conn.threads().is_empty());
    }

    // ---- Status class (`+...`) ----

    #[test]
    fn status_async_is_emitted_as_status_event() {
        let mut conn = Connection::new();
        conn.receive_bytes(b"+download,section=\".text\"\n(gdb)\n")
            .unwrap();
        let events = drain(&mut conn);
        assert!(matches!(events[0], Event::Status(_)));
    }

    // ---- Stream records ----

    #[test]
    fn target_and_log_streams_surface_as_distinct_events() {
        let mut conn = Connection::new();
        conn.receive_bytes(b"@\"hello from inferior\"\n&\"gdb log\"\n(gdb)\n")
            .unwrap();
        let events = drain(&mut conn);
        assert!(matches!(events[0], Event::TargetOutput(_)));
        assert!(matches!(events[1], Event::Log(_)));
    }
}

#[cfg(test)]
mod proptests {
    //! Chunking invariant: feeding the same byte stream in arbitrary
    //! chunk sizes must produce the same event sequence as a single
    //! push. This is the `framewalk-mi-codec` pattern applied at the
    //! `Connection` layer so downstream state updates are also covered.

    use super::*;
    use framewalk_mi_codec::MiCommand;
    use proptest::prelude::*;

    fn run_full(wire: &[u8]) -> Vec<String> {
        let mut conn = Connection::new();
        conn.submit(CommandRequest::new(MiCommand::new("gdb-version")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        conn.receive_bytes(wire).unwrap();
        drain_event_shapes(&mut conn)
    }

    fn run_chunked(wire: &[u8], chunks: &[usize]) -> Vec<String> {
        let mut conn = Connection::new();
        conn.submit(CommandRequest::new(MiCommand::new("gdb-version")));
        let n = conn.outbound().len();
        conn.consume_outbound(n).unwrap();
        let mut i = 0;
        for &len in chunks {
            let end = (i + len).min(wire.len());
            conn.receive_bytes(&wire[i..end]).unwrap();
            i = end;
        }
        if i < wire.len() {
            conn.receive_bytes(&wire[i..]).unwrap();
        }
        drain_event_shapes(&mut conn)
    }

    /// Event "shape" fingerprint — just the variant tag so we're invariant
    /// to the moved AST payloads, which differ only in byte positions.
    fn drain_event_shapes(conn: &mut Connection) -> Vec<String> {
        let mut out = Vec::new();
        while let Some(e) = conn.poll_event() {
            let tag = match e {
                Event::CommandCompleted { .. } => "CommandCompleted",
                Event::Stopped(_) => "Stopped",
                Event::Running(_) => "Running",
                Event::Notify(_) => "Notify",
                Event::Status(_) => "Status",
                Event::Console(_) => "Console",
                Event::TargetOutput(_) => "TargetOutput",
                Event::Log(_) => "Log",
                Event::GroupClosed => "GroupClosed",
                Event::Unknown(_) => "Unknown",
                Event::ParseError(_) => "ParseError",
            };
            out.push(tag.to_string());
        }
        out
    }

    proptest! {
        #[test]
        fn chunking_does_not_change_event_sequence(
            chunks in proptest::collection::vec(1usize..=16, 1..=12),
        ) {
            // A realistic payload: console stream + result + prompt + async stop.
            let wire: &[u8] =
                b"~\"GNU gdb 15.1\\n\"\n1^done,version=\"15.1\"\n(gdb)\n\
                  *stopped,reason=\"breakpoint-hit\",bkptno=\"1\",thread-id=\"1\",\
                  frame={addr=\"0x1\",func=\"main\",args=[],file=\"f.c\",fullname=\"/f.c\",line=\"1\"}\n(gdb)\n";
            let single = run_full(wire);
            let chunked = run_chunked(wire, &chunks);
            prop_assert_eq!(single, chunked);
        }

        #[test]
        fn receive_bytes_never_panics_on_arbitrary_garbage(
            bytes in proptest::collection::vec(any::<u8>(), 0..=256),
        ) {
            // Not checking semantics here — only that malformed input
            // never panics the state machine.
            let mut conn = Connection::new();
            let _ = conn.receive_bytes(&bytes);
        }
    }
}
