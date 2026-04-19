//! End-to-end MCP protocol tests for the Scheme scripting layer.
//!
//! Spawns the compiled `framewalk-mcp` binary and drives it over
//! JSON-RPC on stdin/stdout, exercising `scheme_eval` and mode
//! selection (`--mode full`, `--mode core`, `--mode scheme`).
//!
//! ```sh
//! nix develop --command cargo nextest run -p framewalk-mcp \
//!     --test scheme_mcp_roundtrip --run-ignored all
//! ```

use std::collections::BTreeSet;
use std::process::Stdio;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(20);

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_framewalk-mcp")
}

/// Spawn the MCP server with the given extra CLI args, send messages,
/// and collect replies.
///
/// `expected_replies` is the number of JSON-RPC responses to wait for
/// before closing stdin (which triggers the server to exit).  This
/// avoids a fixed sleep — the reader signals the writer once enough
/// replies have arrived, so slow operations like first-time Steel
/// engine init don't race against stdin EOF.
async fn drive_server_with_args(
    args: &[&str],
    messages: &[String],
    expected_replies: usize,
) -> Vec<Value> {
    let mut child = Command::new(binary())
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("spawn framewalk-mcp");

    let mut stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let mut reader = BufReader::new(stdout).lines();

    // Notify channel: the reader tells the writer when enough replies
    // have arrived so stdin can be closed.
    let (done_tx, done_rx) = tokio::sync::oneshot::channel::<()>();

    let owned: Vec<String> = messages.to_vec();
    let writer = tokio::spawn(async move {
        for msg in &owned {
            stdin.write_all(msg.as_bytes()).await.expect("write");
            stdin.write_all(b"\n").await.expect("write nl");
        }
        stdin.flush().await.expect("flush");
        // Block until the reader says enough replies have arrived,
        // then drop stdin so the server sees EOF and shuts down.
        // No fixed sleep — the reader determines when we're done.
        done_rx.await.ok();
        drop(stdin);
    });

    let reader_fut = async {
        let mut out = Vec::new();
        let mut done_tx = Some(done_tx);
        while let Ok(Some(line)) = reader.next_line().await {
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(v) = serde_json::from_str::<Value>(&line) {
                out.push(v);
            }
            if out.len() >= expected_replies {
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

fn init_messages() -> Vec<String> {
    vec![
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}).to_string(),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}).to_string(),
    ]
}

fn find_reply(replies: &[Value], id: i64) -> &Value {
    replies
        .iter()
        .find(|v| v.get("id").and_then(Value::as_i64) == Some(id))
        .unwrap_or_else(|| panic!("no reply for id={id} in {replies:#?}"))
}

fn tool_names_from_list_reply(reply: &Value) -> Vec<String> {
    reply["result"]["tools"]
        .as_array()
        .expect("tools array")
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str).map(String::from))
        .collect()
}

fn tool_text(reply: &Value) -> &str {
    reply["result"]["content"][0]["text"]
        .as_str()
        .expect("text content")
}

fn scheme_eval_payload(reply: &Value) -> Value {
    let text = tool_text(reply);
    serde_json::from_str(text).unwrap_or_else(|err| {
        panic!("scheme_eval success payload should be JSON: {err}; payload={text}")
    })
}

// =========================================================================
// Mode: full — all tools + scheme_eval
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn full_mode_includes_scheme_eval_and_all_tools() {
    let mut msgs = init_messages();
    msgs.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());

    let replies = drive_server_with_args(&[], &msgs, 2).await;
    let list = find_reply(&replies, 2);
    let names = tool_names_from_list_reply(list);

    assert!(
        names.contains(&"scheme_eval".to_string()),
        "full mode should include scheme_eval: {names:?}"
    );
    assert!(
        names.contains(&"gdb_version".to_string()),
        "full mode should include gdb_version: {names:?}"
    );
    assert!(
        names.contains(&"set_breakpoint".to_string()),
        "full mode should include set_breakpoint: {names:?}"
    );
    assert!(
        names.len() > 100,
        "full mode should have >100 tools, got {}: {names:?}",
        names.len()
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn core_mode_exposes_curated_subset_plus_escape_hatches() {
    let mut full_msgs = init_messages();
    full_msgs.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());
    let full_replies = drive_server_with_args(&[], &full_msgs, 2).await;
    let full_names = tool_names_from_list_reply(find_reply(&full_replies, 2));

    let mut core_msgs = init_messages();
    core_msgs.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());
    let core_replies = drive_server_with_args(&["--mode", "core"], &core_msgs, 2).await;
    let core_names = tool_names_from_list_reply(find_reply(&core_replies, 2));

    for required in [
        "scheme_eval",
        "mi_raw_command",
        "gdb_version",
        "run",
        "backtrace",
        "inspect",
        "read_registers",
        "symbol_info_functions",
    ] {
        assert!(
            core_names.iter().any(|name| name == required),
            "core mode missing {required}: {core_names:?}"
        );
    }
    assert!(
        !core_names.iter().any(|name| name == "reverse_step"),
        "core mode should omit lower-frequency tools like reverse_step: {core_names:?}"
    );
    assert!(
        core_names.len() < full_names.len(),
        "core mode should advertise fewer tools than full mode: core={}, full={}",
        core_names.len(),
        full_names.len()
    );
    assert!(
        core_names.len() > 1,
        "core mode should advertise more than the scheme-only tool"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn standard_alias_maps_to_full_mode() {
    let mut msgs = init_messages();
    msgs.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());

    let alias_replies = drive_server_with_args(&["--mode", "standard"], &msgs, 2).await;
    let alias_names = tool_names_from_list_reply(find_reply(&alias_replies, 2));

    let full_replies = drive_server_with_args(&["--mode", "full"], &msgs, 2).await;
    let full_names = tool_names_from_list_reply(find_reply(&full_replies, 2));

    let alias_set: BTreeSet<_> = alias_names.into_iter().collect();
    let full_set: BTreeSet<_> = full_names.into_iter().collect();
    assert_eq!(
        alias_set, full_set,
        "`standard` alias should expose the same tool set as `full`"
    );
}

// =========================================================================
// Mode: scheme — scheme_eval plus operator escape hatches
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_mode_exposes_scheme_eval_plus_operator_tools() {
    let mut msgs = init_messages();
    msgs.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());

    let replies = drive_server_with_args(&["--mode", "scheme"], &msgs, 2).await;
    let list = find_reply(&replies, 2);
    let names = tool_names_from_list_reply(list);
    let actual: BTreeSet<_> = names.iter().cloned().collect();
    let expected: BTreeSet<_> = [
        "interrupt_target".to_string(),
        "target_state".to_string(),
        "drain_events".to_string(),
        "reconnect_target".to_string(),
        "scheme_eval".to_string(),
    ]
    .into_iter()
    .collect();

    assert_eq!(
        actual, expected,
        "scheme mode should expose the Scheme tool plus operator escape hatches: {names:?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_mode_instructions_mention_scheme() {
    let msgs = init_messages();
    // The initialize reply contains instructions (only 1 reply for init).
    let replies = drive_server_with_args(&["--mode", "scheme"], &msgs, 1).await;
    let init = find_reply(&replies, 1);
    let instructions = init["result"]["instructions"]
        .as_str()
        .expect("instructions should be present");
    assert!(
        instructions.contains("Scheme") || instructions.contains("scheme"),
        "scheme mode instructions should mention Scheme: {instructions}"
    );
}

// =========================================================================
// scheme_eval over MCP protocol
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_eval_arithmetic_via_mcp() {
    let mut msgs = init_messages();
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "scheme_eval",
                "arguments": { "code": "(+ 1 2 3)" }
            }
        })
        .to_string(),
    );

    let replies = drive_server_with_args(&[], &msgs, 2).await;
    let call = find_reply(&replies, 2);

    assert_ne!(
        call["result"]["isError"].as_bool(),
        Some(true),
        "scheme_eval should succeed: {call:#?}"
    );
    let payload = scheme_eval_payload(call);
    assert_eq!(
        payload["result"],
        json!(6),
        "1+2+3 should be 6, got: {payload:#?}"
    );
    assert!(
        payload.get("streams").is_none(),
        "scheme_eval should omit streams by default: {payload:#?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_eval_gdb_version_via_mcp() {
    let mut msgs = init_messages();
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "scheme_eval",
                "arguments": { "code": "(gdb-version)" }
            }
        })
        .to_string(),
    );

    let replies = drive_server_with_args(&[], &msgs, 2).await;
    let call = find_reply(&replies, 2);

    assert_ne!(
        call["result"]["isError"].as_bool(),
        Some(true),
        "scheme_eval(gdb-version) should succeed: {call:#?}"
    );
    let payload = scheme_eval_payload(call);
    let result = payload["result"]
        .as_array()
        .unwrap_or_else(|| panic!("gdb-version should return an entry list: {payload:#?}"));
    assert!(
        result.iter().any(|entry| {
            entry["name"] == json!("version")
                && entry["value"].as_str().is_some_and(|text| !text.is_empty())
        }),
        "gdb-version should expose a non-empty version field: {payload:#?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_eval_error_is_mcp_error() {
    let mut msgs = init_messages();
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "scheme_eval",
                "arguments": { "code": "(/ 1 0)" }
            }
        })
        .to_string(),
    );

    let replies = drive_server_with_args(&[], &msgs, 2).await;
    let call = find_reply(&replies, 2);

    // Scheme errors surface as tool errors (isError=true), not JSON-RPC errors.
    assert_eq!(
        call["result"]["isError"].as_bool(),
        Some(true),
        "division by zero should return tool error: {call:#?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_eval_state_persists_across_mcp_calls() {
    let mut msgs = init_messages();
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "scheme_eval",
                "arguments": { "code": "(define mcp-x 99)" }
            }
        })
        .to_string(),
    );
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {
                "name": "scheme_eval",
                "arguments": { "code": "(+ mcp-x 1)" }
            }
        })
        .to_string(),
    );

    // 3 replies: init (id=1), define (id=2), eval (id=3).
    let replies = drive_server_with_args(&[], &msgs, 3).await;

    let call = find_reply(&replies, 3);
    assert_ne!(
        call["result"]["isError"].as_bool(),
        Some(true),
        "reading mcp-x should succeed: {call:#?}"
    );
    let payload = scheme_eval_payload(call);
    assert_eq!(
        payload["result"],
        json!(100),
        "mcp-x + 1 should be 100, got: {payload:#?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn gdb_version_tool_returns_banner_via_mcp() {
    let mut msgs = init_messages();
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "gdb_version",
                "arguments": {}
            }
        })
        .to_string(),
    );

    let replies = drive_server_with_args(&[], &msgs, 2).await;
    let call = find_reply(&replies, 2);

    assert_ne!(
        call["result"]["isError"].as_bool(),
        Some(true),
        "gdb_version should succeed: {call:#?}"
    );
    let payload: Value = serde_json::from_str(tool_text(call))
        .unwrap_or_else(|err| panic!("gdb_version payload should be JSON: {err}; call={call:#?}"));
    assert!(
        payload["version"]
            .as_str()
            .is_some_and(|text| !text.is_empty()),
        "gdb_version should include a non-empty version string: {payload:#?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn scheme_mode_full_workflow_via_mcp() {
    let test_binary = std::env::current_exe()
        .expect("current_exe")
        .to_string_lossy()
        .into_owned();

    let code = format!(
        r#"(begin
            (load-file "{test_binary}")
            (set-breakpoint "main")
            (run)
            (wait-for-stop)
            (backtrace))"#
    );

    let mut msgs = init_messages();
    msgs.push(
        json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "scheme_eval",
                "arguments": { "code": code }
            }
        })
        .to_string(),
    );

    let replies = drive_server_with_args(&["--mode", "scheme"], &msgs, 2).await;
    let call = find_reply(&replies, 2);

    assert_ne!(
        call["result"]["isError"].as_bool(),
        Some(true),
        "full workflow should succeed: {call:#?}"
    );
    let payload = scheme_eval_payload(call);
    let rendered = payload["result"].to_string();
    assert!(
        rendered.contains("main"),
        "backtrace should contain main: {payload:#?}"
    );
}

// =========================================================================
// Existing tools list is unchanged (regression)
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn full_mode_tool_count_is_previous_plus_one() {
    let mut msgs = init_messages();
    msgs.push(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}).to_string());

    let replies = drive_server_with_args(&[], &msgs, 2).await;
    let list = find_reply(&replies, 2);
    let names = tool_names_from_list_reply(list);

    // Baseline MI3 tools (124) + operator escape hatches (4) + scheme_eval = 129 in full mode.
    // If this number drifts because a new tool was added, update the
    // constant here *and* in the matching assertion in scheme mode
    // below (if any); the test's intent is "scheme_eval is always
    // registered on top of the full MI3 tool set", not a specific
    // magic count.
    assert!(
        names.contains(&"scheme_eval".to_string()),
        "scheme_eval must be registered in full mode: {names:?}"
    );
    assert_eq!(
        names.len(),
        129,
        "full mode should have exactly 129 tools (124 MI3 + 4 operator tools + scheme_eval), got {}: {names:?}",
        names.len()
    );
}
