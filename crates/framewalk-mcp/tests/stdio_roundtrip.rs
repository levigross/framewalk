//! End-to-end MCP stdio integration test.
//!
//! Spawns the compiled `framewalk-mcp` binary as a subprocess, feeds it
//! a sequence of JSON-RPC messages on stdin, and asserts the replies on
//! stdout match the MCP protocol shape. This exercises the full stack:
//! `framewalk-mcp` binary → rmcp stdio transport → `FramewalkMcp` →
//! `Arc<TransportHandle>` → tokio subprocess pump → real gdb.
//!
//! Gated `#[ignore]` because it requires a real `gdb` on PATH (nix
//! devShell provides it). Run with:
//!
//! ```sh
//! nix develop --command cargo test -p framewalk-mcp \
//!     --test stdio_roundtrip -- --ignored
//! ```

use std::collections::BTreeSet;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Build the path to the compiled framewalk-mcp binary. Cargo sets
/// `CARGO_BIN_EXE_<name>` for integration tests, giving us the
/// guaranteed location of the binary under test without having to
/// shell out to `cargo build`.
fn framewalk_mcp_binary() -> &'static str {
    env!("CARGO_BIN_EXE_framewalk-mcp")
}

/// Send a sequence of JSON-RPC messages (one per string) as stdin lines
/// and collect every JSON-RPC reply as a parsed `serde_json::Value`.
///
/// **Termination is reply-driven, not sleep-driven.**  The helper
/// pre-parses the input messages to compute the set of request IDs
/// that expect a reply (notifications have no `id` and are skipped),
/// then keeps the server's stdin open until the reader has observed
/// every expected reply.  Only then does it drop stdin, which
/// triggers the server's shutdown on EOF.  A fixed sleep would race
/// against server startup + handler latency and produce flaky
/// behaviour whenever a handler ran slower than the fudge factor —
/// see the history on `resources_roundtrip.rs` for the concrete
/// failure that motivated this shape.
async fn drive_server(messages: &[&str]) -> Vec<serde_json::Value> {
    // Pre-compute the set of request IDs that should receive a
    // reply.  Notifications (no `id` field) are not expected to
    // produce one.  Malformed input is treated as "no id" — the
    // test will still time out via `TEST_TIMEOUT` if the caller
    // writes a broken message.
    let expected_ids: BTreeSet<i64> = messages
        .iter()
        .filter_map(|m| serde_json::from_str::<serde_json::Value>(m).ok())
        .filter_map(|v| v.get("id").and_then(serde_json::Value::as_i64))
        .collect();

    let mut child = Command::new(framewalk_mcp_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn framewalk-mcp");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout).lines();

    // When the reader has seen every expected reply it signals the
    // writer via this one-shot; the writer then drops stdin.  If the
    // caller sent no request messages (everything was a notification)
    // we signal immediately so the writer doesn't park indefinitely.
    let (done_tx, done_rx) = oneshot::channel::<()>();
    let writer_messages: Vec<String> = messages.iter().map(|s| (*s).to_string()).collect();
    let writer = tokio::spawn(async move {
        for msg in writer_messages {
            stdin.write_all(msg.as_bytes()).await.expect("write");
            stdin.write_all(b"\n").await.expect("write nl");
        }
        stdin.flush().await.expect("flush");
        // Block until the reader reports that every expected reply
        // has arrived (or the oneshot is dropped because the reader
        // exited early on an I/O error).
        done_rx.await.ok();
        drop(stdin);
    });

    let reader_fut = async {
        let mut out: Vec<serde_json::Value> = Vec::new();
        let mut seen: BTreeSet<i64> = BTreeSet::new();
        let mut done_tx = Some(done_tx);

        // Nothing to wait for — release the writer immediately so
        // it can drop stdin and let the server shut down.
        if expected_ids.is_empty() {
            if let Some(tx) = done_tx.take() {
                tx.send(()).ok();
            }
        }

        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<serde_json::Value>(&line) {
                Ok(v) => {
                    if let Some(id) = v.get("id").and_then(serde_json::Value::as_i64) {
                        seen.insert(id);
                    }
                    out.push(v);
                }
                Err(err) => eprintln!("test: failed to parse server line {line:?}: {err}"),
            }

            // Once every expected reply has been collected, release
            // the writer; it drops stdin, the server shuts down on
            // EOF, `next_line` eventually returns None, and this
            // loop exits.  Further trailing lines (log records,
            // unsolicited notifications) are still collected into
            // `out` so callers can inspect them.
            if done_tx.is_some() && expected_ids.iter().all(|id| seen.contains(id)) {
                if let Some(tx) = done_tx.take() {
                    tx.send(()).ok();
                }
            }
        }
        out
    };

    let replies = timeout(TEST_TIMEOUT, reader_fut)
        .await
        .unwrap_or_else(|_| panic!("server did not finish within {TEST_TIMEOUT:?}"));

    writer.await.ok();
    child.wait().await.ok();
    replies
}

fn find_reply(replies: &[serde_json::Value], id: i64) -> &serde_json::Value {
    replies
        .iter()
        .find(|v| v.get("id").and_then(serde_json::Value::as_i64) == Some(id))
        .unwrap_or_else(|| panic!("no reply for id={id} in {replies:?}"))
}

// ---------------------------------------------------------------------------
// Test 1: initialize + tools/list publishes every expected tool
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
#[allow(clippy::too_many_lines)]
async fn initialize_and_list_tools() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#,
    ])
    .await;

    // Initialize succeeded with the right server name.
    let init = find_reply(&replies, 1);
    assert_eq!(
        init["result"]["serverInfo"]["name"].as_str(),
        Some("framewalk-mcp"),
        "initialize reply: {init:#?}"
    );
    // Resources capability must be advertised so clients call
    // resources/list instead of assuming the server is tools-only.
    assert!(
        init["result"]["capabilities"]["resources"].is_object(),
        "capabilities.resources should be a non-null object: {init:#?}"
    );

    // tools/list returned every expected tool name.
    let list = find_reply(&replies, 2);
    let tools = list["result"]["tools"]
        .as_array()
        .expect("tools array in tools/list reply");
    let tool_names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t.get("name").and_then(serde_json::Value::as_str))
        .collect();

    let required = [
        // session
        "gdb_version",
        "load_file",
        "attach",
        "detach",
        // always-available operator tools
        "interrupt_target",
        "target_state",
        "drain_events",
        "reconnect_target",
        // execution (original)
        "run",
        "cont",
        "step",
        "next",
        "finish",
        "interrupt",
        // execution (expanded)
        "step_instruction",
        "next_instruction",
        "until",
        "return_from_function",
        "jump",
        // reverse execution
        "reverse_step",
        "reverse_next",
        "reverse_continue",
        "reverse_finish",
        // breakpoints
        "set_breakpoint",
        "list_breakpoints",
        "delete_breakpoint",
        "enable_breakpoint",
        "disable_breakpoint",
        "set_watchpoint",
        "break_condition",
        "break_after",
        "break_commands",
        "break_passcount",
        "break_info",
        "dprintf_insert",
        // inspection
        "backtrace",
        "list_threads",
        "select_frame",
        "select_thread",
        // stack (expanded)
        "frame_info",
        "stack_depth",
        "list_locals",
        "list_arguments",
        "list_variables",
        "enable_frame_filters",
        // variables (var-create/update/delete via watch_*)
        "inspect",
        "watch_create",
        "watch_list",
        "watch_delete",
        // variable objects (remaining -var-* commands)
        "var_list_children",
        "var_evaluate_expression",
        "var_assign",
        "var_set_format",
        "var_show_format",
        "var_info_num_children",
        "var_info_type",
        "var_info_expression",
        "var_info_path_expression",
        "var_show_attributes",
        "var_set_frozen",
        "var_set_update_range",
        "var_set_visualizer",
        "enable_pretty_printing",
        // data
        "read_memory",
        "write_memory",
        "disassemble",
        "list_register_names",
        "read_registers",
        "list_changed_registers",
        "read_memory_deprecated",
        // program context
        "set_args",
        "set_cwd",
        "show_cwd",
        "set_inferior_tty",
        "show_inferior_tty",
        "environment_directory",
        "environment_path",
        // file
        "exec_file",
        "symbol_file",
        "list_source_files",
        "list_shared_libraries",
        "list_exec_source_file",
        // target
        "target_select",
        "target_download",
        "target_disconnect",
        "target_flash_erase",
        // file transfer
        "target_file_put",
        "target_file_get",
        "target_file_delete",
        // support
        "list_features",
        "list_target_features",
        "info_mi_command",
        "gdb_set",
        "gdb_show",
        // catchpoints
        "catch_load",
        "catch_unload",
        "catch_assert",
        "catch_exception",
        "catch_handlers",
        "catch_throw",
        "catch_rethrow",
        "catch_catch",
        // tracepoints
        "trace_insert",
        "trace_start",
        "trace_stop",
        "trace_status",
        "trace_save",
        "trace_list_variables",
        "trace_define_variable",
        "trace_find",
        "trace_frame_collected",
        // symbol query
        "symbol_info_functions",
        "symbol_info_types",
        "symbol_info_variables",
        "symbol_info_modules",
        "symbol_info_module_functions",
        "symbol_info_module_variables",
        "symbol_list_lines",
        // ada exceptions
        "info_ada_exceptions",
        // miscellaneous
        "list_thread_groups",
        "info_os",
        "add_inferior",
        "remove_inferior",
        "thread_list_ids",
        "ada_task_info",
        "enable_timings",
        "complete",
        // raw escape hatch
        "mi_raw_command",
        // scheme scripting
        "scheme_eval",
    ];
    for expected in required {
        assert!(
            tool_names.contains(&expected),
            "missing tool '{expected}' in {tool_names:?}"
        );
    }
    // Tool count checked below against the full required list.
    assert_eq!(
        tool_names.len(),
        required.len(),
        "framewalk-mcp should publish exactly {} tools; got {} — {tool_names:?}",
        required.len(),
        tool_names.len()
    );
}

// ---------------------------------------------------------------------------
// Test 2: tools/call on `gdb_version` returns a valid success result
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn tools_call_gdb_version() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"gdb_version","arguments":{}}}"#,
    ])
    .await;

    let call = find_reply(&replies, 2);
    let result = &call["result"];
    // isError should be false (or missing, which mcp treats as false).
    assert_ne!(
        result["isError"].as_bool(),
        Some(true),
        "gdb_version should succeed: {call:#?}"
    );
    // Content should be a non-empty array with at least one text block.
    let content = result["content"].as_array().expect("content array");
    assert!(!content.is_empty(), "gdb_version content was empty");
    assert_eq!(
        content[0]["type"].as_str(),
        Some("text"),
        "first content block should be text"
    );
}

// ---------------------------------------------------------------------------
// Test 3: raw-MI guard fires for shell-adjacent commands without
// --allow-shell. The MCP client receives an invalid_params error, not a
// silent forward to GDB.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn raw_mi_command_rejects_shell_by_default() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"mi_raw_command","arguments":{"command":"-interpreter-exec console \"shell ls\""}}}"#,
    ])
    .await;

    let call = find_reply(&replies, 2);
    // The guard returns an McpError::invalid_params, which rmcp surfaces
    // as a JSON-RPC error object (not as a tool-call result with
    // isError=true). Either shape is an acceptable rejection; we just
    // need to see the failure surface somewhere.
    let rejected = call.get("error").is_some() || call["result"]["isError"].as_bool() == Some(true);
    assert!(
        rejected,
        "mi_raw_command with shell pivot should be rejected: {call:#?}"
    );
}
