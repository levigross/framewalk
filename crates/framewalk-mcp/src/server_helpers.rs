//! Outcome formatting and command-builder helpers for the MCP server.
//!
//! These turn framewalk-internal types into MCP `Content` blocks and MI
//! command strings. Kept separate from `server.rs` because they are
//! module-scope functions with no reason to touch `self`.

use framewalk_mi_codec::{encode_command, MiCommand, Value};
use framewalk_mi_protocol::{CommandOutcome, Event, StoppedReason, TargetState};
use rmcp::model::{CallToolResult, Content};
use rmcp::ErrorData as McpError;
use serde::Serialize;

use framewalk_mi_transport::{EventSeq, TransportError, TransportHandle};

use crate::raw_guard::{raw_mi_operation, RawMiRejection};
use crate::types::symbol;

/// Turn a `CommandOutcome` into a `CallToolResult` with a single text
/// content block containing compact JSON. MCP does not define a native
/// structured-JSON content type for tool outputs, so `Content::text`
/// with a JSON string is the standard approach. Compact (not
/// pretty-printed) to minimise token consumption when the response is
/// fed back into an LLM context window.
pub(crate) fn format_outcome(outcome: &CommandOutcome) -> CallToolResult {
    let json = outcome_to_json(outcome);
    // `^error` outcomes become MCP tool errors (isError = true) so the
    // client model sees them as failures rather than normal results.
    let is_error = matches!(outcome, CommandOutcome::Error { .. });
    json_tool_result(&json, is_error)
}

/// Serialise a `CommandOutcome` into a `serde_json::Value` with a stable
/// shape: `{ "class": "done", "results": [ {"name":…, "value":…}, … ] }`.
/// Results are an ordered array of name/value entries (not an object) so
/// duplicate keys and insertion order from the MI wire are preserved.
pub(crate) fn outcome_to_json(outcome: &CommandOutcome) -> serde_json::Value {
    use serde_json::json;
    match outcome {
        CommandOutcome::Done(results) => json!({
            "class": "done",
            "results": results_to_json(results),
        }),
        CommandOutcome::Running => json!({ "class": "running" }),
        CommandOutcome::Connected(results) => json!({
            "class": "connected",
            "results": results_to_json(results),
        }),
        CommandOutcome::Error { msg, code } => json!({
            "class": "error",
            "msg": msg,
            "code": code,
        }),
        CommandOutcome::Exit => json!({ "class": "exit" }),
    }
}

/// Serialise an MI result tuple as an **array** of `{"name": k, "value": v}`
/// entries. This is the lossless form: MI preserves key order and allows
/// duplicate keys (the codec's `Vec<(String, Value)>` contract), and a JSON
/// object would silently overwrite duplicates. Downstream consumers that
/// want map-style lookup can build one from the array; consumers that need
/// the full fidelity of the wire shape already have it.
pub(crate) fn results_to_json(results: &[(String, Value)]) -> serde_json::Value {
    serde_json::Value::Array(
        results
            .iter()
            .map(|(k, v)| serde_json::json!({"name": k, "value": value_to_json(v)}))
            .collect(),
    )
}

pub(crate) fn value_to_json(value: &Value) -> serde_json::Value {
    use framewalk_mi_codec::ListValue;
    match value {
        Value::Const(s) => serde_json::Value::String(s.clone()),
        Value::Tuple(pairs) => results_to_json(pairs),
        Value::List(list) => match list {
            ListValue::Empty => serde_json::Value::Array(Vec::new()),
            ListValue::Values(vs) => {
                serde_json::Value::Array(vs.iter().map(value_to_json).collect())
            }
            ListValue::Results(pairs) => results_to_json(pairs),
        },
    }
}

pub(crate) fn build_symbol_info_cmd(operation: &str, args: &symbol::SymbolInfoArgs) -> MiCommand {
    let mut cmd = MiCommand::new(operation);
    if args.include_nondebug {
        cmd = cmd.option("include-nondebug");
    }
    if let Some(name) = &args.name {
        cmd = cmd.option_with("name", name.clone());
    }
    if let Some(type_re) = &args.type_regexp {
        cmd = cmd.option_with("type", type_re.clone());
    }
    if let Some(max) = args.max_results {
        cmd = cmd.option_with("max-results", max.to_string());
    }
    cmd
}

pub(crate) fn build_symbol_module_cmd(
    operation: &str,
    args: &symbol::SymbolModuleArgs,
) -> MiCommand {
    let mut cmd = MiCommand::new(operation);
    if let Some(module) = &args.module {
        cmd = cmd.option_with("module", module.clone());
    }
    if let Some(name) = &args.name {
        cmd = cmd.option_with("name", name.clone());
    }
    if let Some(type_re) = &args.type_regexp {
        cmd = cmd.option_with("type", type_re.clone());
    }
    cmd
}

pub(crate) fn raw_mi_rejection_to_mcp_error(rejection: RawMiRejection) -> McpError {
    McpError::invalid_params(rejection.reason().to_string(), None)
}

/// Recognise the narrow class of `-target-select` `^error` messages
/// that indicate the remote stub advertised (via qSupported) that
/// non-stop mode is unavailable. Matches only if the message mentions
/// `non-stop` **and** one of a short list of unambiguous phrases, so a
/// caller-facing error string that incidentally contains "non-stop"
/// does not trigger an unrelated retry.
pub(crate) fn is_non_stop_mismatch(msg: &str) -> bool {
    let haystack = msg.to_ascii_lowercase();
    if !haystack.contains("non-stop") {
        return false;
    }
    haystack.contains("does not support")
        || haystack.contains("not supported")
        || haystack.contains("cannot enable")
}

/// Attempt a one-shot recovery when `-target-select` fails because the
/// remote stub does not support non-stop: emit a synthetic `warning:`
/// log so `drain-events` and `collect_recent_warnings` pick it up,
/// submit `-gdb-set non-stop off`, and return a rebuilt `-target-select`
/// `CommandOutcome` from the retry. On any failure along the way the
/// original error is preserved and surfaced to the caller.
///
/// Never recurses — the caller must only invoke this once per logical
/// target-select request.
pub(crate) async fn downgrade_non_stop_and_retry(
    transport: &TransportHandle,
    original_msg: &str,
    rebuild: impl Fn() -> MiCommand,
) -> Result<CommandOutcome, McpError> {
    transport.record_synthetic_log(format!(
        "warning: remote rejected non-stop ({original_msg}); downgrading to all-stop and retrying -target-select"
    ));

    let downgrade = MiCommand::new("gdb-set")
        .parameter("non-stop")
        .parameter("off");
    match transport.submit(downgrade).await {
        Ok(CommandOutcome::Done(_) | CommandOutcome::Connected(_)) => {}
        Ok(other) => {
            transport.record_synthetic_log(format!(
                "warning: `-gdb-set non-stop off` returned {other:?}; surfacing original target-select error"
            ));
            return Ok(CommandOutcome::Error {
                msg: original_msg.to_string(),
                code: None,
            });
        }
        Err(err) => {
            transport.record_synthetic_log(format!(
                "warning: `-gdb-set non-stop off` failed ({err}); surfacing original target-select error"
            ));
            return Ok(CommandOutcome::Error {
                msg: original_msg.to_string(),
                code: None,
            });
        }
    }

    transport
        .submit(rebuild())
        .await
        .map_err(|err| transport_error_to_mcp(&err))
}

/// Recognise the small set of `-target-select` transport names that
/// connect to a remote stub. These are the only situations where a
/// vmlinux-shaped target is plausible, so the probe is gated to them
/// to avoid a spurious extra MI round-trip on every local connect.
pub(crate) fn is_remote_target_transport(transport: &str) -> bool {
    matches!(
        transport.trim().to_ascii_lowercase().as_str(),
        "remote" | "extended-remote" | "qnx"
    )
}

/// Kick off a best-effort probe for a vmlinux-shaped target (presence
/// of the `linux_banner` symbol) and, on success, record an
/// informational log event so `drain-events` surfaces the hint.
///
/// Spawns a bounded background task so a slow probe cannot block the
/// `-target-select` tool return.  Silent on probe failure, absence of
/// the symbol, or timeout — detection is best-effort, not a guarantee.
pub(crate) fn spawn_vmlinux_probe(transport: std::sync::Arc<TransportHandle>) {
    tokio::spawn(async move {
        let probe = MiCommand::new("symbol-info-variables").option_with("name", "linux_banner");

        let probe_future = transport.submit(probe);
        let Ok(probe_result) =
            tokio::time::timeout(std::time::Duration::from_secs(2), probe_future).await
        else {
            return;
        };
        let Ok(outcome) = probe_result else {
            return;
        };

        let (CommandOutcome::Done(results) | CommandOutcome::Connected(results)) = outcome else {
            return;
        };

        // `-symbol-info-variables` reports matches under a `symbols`
        // tuple whose `debug` list is non-empty when the symbol
        // resolves.  Any non-empty nested value is enough to flag
        // detection; the exact shape varies between gdb versions.
        if vmlinux_probe_detected(&results) {
            transport.record_synthetic_log(
                "info: vmlinux-shaped target detected — consider `source scripts/gdb/vmlinux-gdb.py`".to_string(),
            );
        }
    });
}

fn vmlinux_probe_detected(results: &[(String, Value)]) -> bool {
    fn value_has_non_empty_list(v: &Value) -> bool {
        use framewalk_mi_codec::ListValue;
        match v {
            Value::Const(_) | Value::List(ListValue::Empty) => false,
            Value::Tuple(pairs) => pairs
                .iter()
                .any(|(_, inner)| value_has_non_empty_list(inner)),
            Value::List(ListValue::Values(vs)) => {
                !vs.is_empty() || vs.iter().any(value_has_non_empty_list)
            }
            Value::List(ListValue::Results(pairs)) => {
                !pairs.is_empty()
                    || pairs
                        .iter()
                        .any(|(_, inner)| value_has_non_empty_list(inner))
            }
        }
    }

    results
        .iter()
        .any(|(name, value)| name == "symbols" && value_has_non_empty_list(value))
}

pub(crate) fn remember_successful_target_select_command(
    transport: &TransportHandle,
    command: &MiCommand,
    outcome: &CommandOutcome,
) {
    if !matches!(
        outcome,
        CommandOutcome::Connected(_) | CommandOutcome::Done(_)
    ) {
        return;
    }
    if !command.operation.eq_ignore_ascii_case("target-select") {
        return;
    }

    transport.remember_target_selection_command(encode_mi_command(command));
}

pub(crate) fn remember_successful_raw_target_select(
    transport: &TransportHandle,
    raw_command: &str,
    outcome: &CommandOutcome,
) {
    if !matches!(
        outcome,
        CommandOutcome::Connected(_) | CommandOutcome::Done(_)
    ) {
        return;
    }

    if raw_mi_operation(raw_command)
        .ok()
        .is_some_and(|operation| operation.eq_ignore_ascii_case("target-select"))
    {
        transport.remember_target_selection_command(raw_command.trim().to_string());
    }
}

pub(crate) fn json_tool_result(value: &serde_json::Value, is_error: bool) -> CallToolResult {
    let text = serde_json::to_string(value).unwrap_or_else(|_| value.to_string());
    if is_error {
        CallToolResult::error(vec![Content::text(text)])
    } else {
        CallToolResult::success(vec![Content::text(text)])
    }
}

/// Map a [`TransportError`] to a structured [`McpError`] so MCP clients
/// can distinguish failure modes (gdb exited vs I/O error vs protocol
/// rejection) rather than seeing a single opaque string.
pub(crate) fn transport_error_to_mcp(err: &TransportError) -> McpError {
    match err {
        TransportError::Exited => McpError::internal_error(
            "gdb exited before the operation completed — restart the session",
            None,
        ),
        TransportError::Bootstrap { command, message } => McpError::internal_error(
            format!("gdb bootstrap failed for `{command}`: {message}"),
            None,
        ),
        TransportError::BufferOverflow {
            pending_bytes,
            limit,
        } => McpError::internal_error(
            format!(
                "framer buffer overflow ({pending_bytes} bytes, limit {limit}) — \
                 gdb may be producing malformed output"
            ),
            None,
        ),
        TransportError::Spawn(io_err) => {
            McpError::internal_error(format!("failed to spawn gdb: {io_err}"), None)
        }
        TransportError::PipeMissing(pipe) => {
            McpError::internal_error(format!("gdb subprocess missing {pipe} pipe"), None)
        }
        TransportError::Io(io_err) => {
            McpError::internal_error(format!("I/O error communicating with gdb: {io_err}"), None)
        }
        TransportError::Protocol(proto_err) => {
            McpError::invalid_params(format!("protocol error: {proto_err}"), None)
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ObservedTargetState {
    pub(crate) state: &'static str,
    pub(crate) reader_alive: bool,
    pub(crate) cursor: EventSeq,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) thread: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) last_async: Option<ObservedEvent>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct DrainEventsPayload {
    pub(crate) from_cursor: EventSeq,
    pub(crate) cursor: EventSeq,
    pub(crate) truncated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) retained_from: Option<EventSeq>,
    pub(crate) events: Vec<ObservedEvent>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ObservedEvent {
    pub seq: EventSeq,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub class: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub results: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<serde_json::Value>,
}

pub(crate) fn observe_target_state(transport: &TransportHandle) -> ObservedTargetState {
    let snapshot = transport.snapshot();
    let cursor = transport.event_cursor();
    let reader_alive = transport.is_reader_alive();
    let last_async = transport
        .events_after(0)
        .into_iter()
        .filter_map(|(seq, event)| summarize_async_event(seq, &event))
        .next_back();

    let (state, thread, reason, exit_code) =
        if !reader_alive && !matches!(snapshot.target, TargetState::Exited { .. }) {
            ("disconnected", None, None, None)
        } else {
            match snapshot.target {
                TargetState::Running { thread } => (
                    "running",
                    thread.map(|tid| tid.as_str().to_string()),
                    None,
                    None,
                ),
                TargetState::Stopped { thread, reason } => (
                    "stopped",
                    thread.map(|tid| tid.as_str().to_string()),
                    reason.as_ref().map(format_stopped_reason),
                    stopped_reason_exit_code(reason.as_ref()),
                ),
                TargetState::Exited { exit_code } => ("exited", None, None, exit_code),
                TargetState::Unknown => ("unknown", None, None, None),
            }
        };

    ObservedTargetState {
        state,
        reader_alive,
        cursor,
        thread,
        reason,
        exit_code,
        last_async,
    }
}

pub(crate) fn drain_observed_events(
    transport: &TransportHandle,
    from_cursor: EventSeq,
) -> DrainEventsPayload {
    let retained_from = transport.earliest_event_seq();
    let events: Vec<ObservedEvent> = transport
        .events_after(from_cursor)
        .into_iter()
        .filter_map(|(seq, event)| summarize_event(seq, &event))
        .collect();
    let cursor = transport.event_cursor();
    let truncated = retained_from.is_some_and(|earliest| from_cursor.saturating_add(1) < earliest);

    DrainEventsPayload {
        from_cursor,
        cursor,
        truncated,
        retained_from,
        events,
    }
}

pub(crate) fn collect_console_text_since(
    transport: &TransportHandle,
    from_cursor: EventSeq,
) -> Option<String> {
    let mut buf = String::new();
    for (_, event) in transport.events_after(from_cursor) {
        if let Event::Console(text) = event {
            buf.push_str(&text);
        }
    }

    let trimmed = buf.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(crate) fn timeout_context_message(transport: &TransportHandle) -> String {
    let observed = observe_target_state(transport);
    let mut parts = vec![
        format!("target_state={}", observed.state),
        format!("reader_alive={}", observed.reader_alive),
        format!("cursor={}", observed.cursor),
    ];
    if let Some(thread) = observed.thread {
        parts.push(format!("thread={thread}"));
    }
    if let Some(reason) = observed.reason {
        parts.push(format!("reason={reason}"));
    }
    if let Some(exit_code) = observed.exit_code {
        parts.push(format!("exit_code={exit_code}"));
    }
    if let Some(last_async) = observed.last_async {
        let mut async_desc = format!("{}@{}", last_async.kind, last_async.seq);
        if let Some(class) = last_async.class {
            async_desc.push_str(" class=");
            async_desc.push_str(&class);
        }
        if let Some(reason) = last_async.reason {
            async_desc.push_str(" reason=");
            async_desc.push_str(&reason);
        }
        if let Some(thread) = last_async.thread {
            async_desc.push_str(" thread=");
            async_desc.push_str(&thread);
        }
        parts.push(format!("last_async={async_desc}"));
    }
    parts.join(", ")
}

fn summarize_event(seq: EventSeq, event: &Event) -> Option<ObservedEvent> {
    match event {
        Event::Running(ev) => Some(ObservedEvent {
            seq,
            kind: "running",
            thread: ev.thread.as_ref().map(|tid| tid.as_str().to_string()),
            ..ObservedEvent::default()
        }),
        Event::Stopped(ev) => Some(ObservedEvent {
            seq,
            kind: "stopped",
            thread: ev.thread.as_ref().map(|tid| tid.as_str().to_string()),
            reason: ev.reason.as_ref().map(format_stopped_reason),
            exit_code: stopped_reason_exit_code(ev.reason.as_ref()),
            results: if ev.raw.is_empty() {
                None
            } else {
                Some(results_to_json(&ev.raw))
            },
            frame: ev.frame.as_ref().map(|frame| {
                serde_json::json!({
                    "level": frame.level,
                    "func": frame.func,
                    "file": frame.file,
                    "fullname": frame.fullname,
                    "line": frame.line,
                    "addr": frame.addr,
                })
            }),
            ..ObservedEvent::default()
        }),
        Event::Notify(ev) => Some(ObservedEvent {
            seq,
            kind: "notify",
            class: Some(ev.class.clone()),
            results: Some(results_to_json(&ev.results)),
            ..ObservedEvent::default()
        }),
        Event::Status(ev) => Some(ObservedEvent {
            seq,
            kind: "status",
            class: Some(ev.class.clone()),
            results: Some(results_to_json(&ev.results)),
            ..ObservedEvent::default()
        }),
        Event::Console(text) => Some(stream_event(seq, "console", text)),
        Event::TargetOutput(text) => Some(stream_event(seq, "target-output", text)),
        Event::Log(text) => Some(stream_event(seq, "log", text)),
        Event::ParseError(err) => Some(stream_event(seq, "parse-error", &err.error.to_string())),
        Event::CommandCompleted { .. } | Event::GroupClosed | Event::Unknown(_) => None,
    }
}

fn summarize_async_event(seq: EventSeq, event: &Event) -> Option<ObservedEvent> {
    match event {
        Event::Running(_) | Event::Stopped(_) | Event::Notify(_) | Event::Status(_) => {
            summarize_event(seq, event)
        }
        Event::Console(_)
        | Event::TargetOutput(_)
        | Event::Log(_)
        | Event::ParseError(_)
        | Event::CommandCompleted { .. }
        | Event::GroupClosed
        | Event::Unknown(_) => None,
    }
}

fn stream_event(seq: EventSeq, kind: &'static str, text: &str) -> ObservedEvent {
    ObservedEvent {
        seq,
        kind,
        text: Some(text.to_string()),
        ..ObservedEvent::default()
    }
}

pub(crate) fn format_stopped_reason(reason: &StoppedReason) -> String {
    match reason {
        StoppedReason::BreakpointHit { .. } => "breakpoint-hit".to_string(),
        StoppedReason::WatchpointTrigger => "watchpoint-trigger".to_string(),
        StoppedReason::FunctionFinished => "function-finished".to_string(),
        StoppedReason::LocationReached => "location-reached".to_string(),
        StoppedReason::EndSteppingRange => "end-stepping-range".to_string(),
        StoppedReason::ExitedNormally => "exited-normally".to_string(),
        StoppedReason::Exited { .. } => "exited".to_string(),
        StoppedReason::ExitedSignalled { .. } => "exited-signalled".to_string(),
        StoppedReason::SignalReceived { .. } => "signal-received".to_string(),
        StoppedReason::Fork => "fork".to_string(),
        StoppedReason::Vfork => "vfork".to_string(),
        StoppedReason::Exec => "exec".to_string(),
        StoppedReason::SyscallEntry { .. } => "syscall-entry".to_string(),
        StoppedReason::SyscallReturn { .. } => "syscall-return".to_string(),
        StoppedReason::Other(other) => other.clone(),
    }
}

fn stopped_reason_exit_code(reason: Option<&StoppedReason>) -> Option<i32> {
    match reason {
        Some(StoppedReason::Exited { exit_code }) => *exit_code,
        Some(StoppedReason::ExitedNormally) => Some(0),
        _ => None,
    }
}

fn encode_mi_command(command: &MiCommand) -> String {
    let mut encoded = Vec::new();
    encode_command(None, command, &mut encoded);
    let Some(b'\n') = encoded.last().copied() else {
        panic!("encoded MI command should end with a newline");
    };
    encoded.pop();
    String::from_utf8(encoded).expect("encoded MI command should be UTF-8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_mi_command_matches_mi_wire_format() {
        let command = MiCommand::new("target-select")
            .parameter("extended-remote")
            .parameter("localhost:1234");

        assert_eq!(
            encode_mi_command(&command),
            "-target-select extended-remote localhost:1234"
        );
    }

    #[test]
    fn encode_mi_command_quotes_parameters() {
        let command = MiCommand::new("target-select")
            .parameter("remote")
            .parameter("/tmp/with spaces/socket");

        assert_eq!(
            encode_mi_command(&command),
            "-target-select remote \"/tmp/with spaces/socket\""
        );
    }
}
