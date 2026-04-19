//! End-to-end MCP resources integration test.
//!
//! Spawns the compiled `framewalk-mcp` binary, drives it through
//! `initialize` + `resources/list` + `resources/read` over stdio, and
//! asserts the replies match the contract the resource layer promises:
//!
//! * the server advertises `capabilities.resources` on initialize
//! * `resources/list` returns exactly 33 items, all `text/markdown`
//! * all three resource families (guide, reference, recipe) are present
//! * `resources/read framewalk://guide/getting-started` returns the
//!   body embedded via `include_str!` from the top-level `docs/` tree
//! * `resources/read` on an unknown URI returns a JSON-RPC error
//!
//! Gated `#[ignore]` because the binary spawns a real `gdb` subprocess
//! on start-up, which is only available in the nix devShell. Run with:
//!
//! ```sh
//! nix develop --command cargo test -p framewalk-mcp \
//!     --test resources_roundtrip -- --ignored
//! ```

use std::collections::BTreeSet;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(20);

fn framewalk_mcp_binary() -> &'static str {
    env!("CARGO_BIN_EXE_framewalk-mcp")
}

/// Send a sequence of JSON-RPC messages and collect every reply as a
/// parsed JSON value. Mirrors the helper in `stdio_roundtrip.rs`;
/// duplicated rather than extracted into a shared `tests/common.rs`
/// because sharing test helpers across integration tests requires a
/// `#[path]` attribute on a module declaration in every test file,
/// which is more churn than the ~30 lines of copy.
///
/// **Termination is reply-driven, not sleep-driven.**  The helper
/// pre-parses the input messages to compute the set of request IDs
/// that expect a reply (notifications have no `id` and are skipped),
/// then keeps the server's stdin open until the reader has observed
/// every expected reply.  Only then does it drop stdin, which
/// triggers the server's shutdown on EOF.  A fixed sleep race
/// against server startup + handler latency would be wrong: the
/// resources feature, for example, takes longer than the previous
/// 300ms fudge factor to produce its first `resources/list` reply on
/// a cold build, causing that reply to be lost when the server's
/// rmcp loop saw EOF mid-handler.
async fn drive_server(messages: &[&str]) -> Vec<serde_json::Value> {
    // Pre-compute the set of request IDs that should receive a
    // reply.  Notifications (no `id` field) are not expected to
    // produce one.  Malformed input is treated as "no id" — the test
    // will still time out via `TEST_TIMEOUT` if the user writes a
    // broken message.
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
        // Block until the reader says every expected reply has
        // arrived (or the oneshot is dropped, which happens if the
        // reader exits early on an I/O error).
        done_rx.await.ok();
        drop(stdin);
    });

    let reader_fut = async {
        let mut out: Vec<serde_json::Value> = Vec::new();
        let mut seen: BTreeSet<i64> = BTreeSet::new();
        let mut done_tx = Some(done_tx);

        // If the caller expects no replies at all, tell the writer
        // to close stdin immediately.  Without this the writer would
        // park forever waiting for a signal the reader will never
        // send.
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

            // Once we hold every expected reply, release the writer
            // so it drops stdin; the server then shuts down cleanly
            // and `next_line` will eventually see EOF so this loop
            // exits.  After this point any further lines we read
            // (trailing notifications, final logs) are still
            // accumulated into `out` for the caller to inspect.
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
// Test 1: initialize advertises the resources capability
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn initialize_exposes_resources_capability() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
    ])
    .await;

    let init = find_reply(&replies, 1);
    let resources = &init["result"]["capabilities"]["resources"];
    assert!(
        resources.is_object(),
        "capabilities.resources should be a non-null object: {init:#?}"
    );
}

// ---------------------------------------------------------------------------
// Test 2: resources/list returns 33 markdown resources across 3 families
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn resources_list_returns_thirty_three_markdown_items() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#,
    ])
    .await;

    let list = find_reply(&replies, 2);
    let items = list["result"]["resources"]
        .as_array()
        .expect("resources array in resources/list reply");

    assert_eq!(
        items.len(),
        33,
        "expected 33 resources; got {} — {list:#?}",
        items.len()
    );

    // Every item has markdown mime and a non-empty size.
    for item in items {
        assert_eq!(
            item["mimeType"].as_str(),
            Some("text/markdown"),
            "non-markdown mime on item: {item:#?}"
        );
        assert!(
            item["size"].as_u64().unwrap_or(0) > 0,
            "zero size on item: {item:#?}"
        );
    }

    // Spot-check that all three families are represented.
    let uris: Vec<&str> = items.iter().filter_map(|i| i["uri"].as_str()).collect();
    for expected in [
        "framewalk://guide/getting-started",
        "framewalk://guide/execution-model",
        "framewalk://guide/no-source",
        "framewalk://reference/session",
        "framewalk://reference/breakpoints",
        "framewalk://recipe/debug-segfault",
    ] {
        assert!(
            uris.contains(&expected),
            "missing expected uri {expected} in {uris:?}"
        );
    }

    // Count by family to confirm the 12/16/5 split.
    let guides = uris
        .iter()
        .filter(|u| u.starts_with("framewalk://guide/"))
        .count();
    let refs = uris
        .iter()
        .filter(|u| u.starts_with("framewalk://reference/"))
        .count();
    let recipes = uris
        .iter()
        .filter(|u| u.starts_with("framewalk://recipe/"))
        .count();
    assert_eq!(guides, 12, "expected 12 guides; got {guides}");
    assert_eq!(refs, 16, "expected 16 references; got {refs}");
    assert_eq!(recipes, 5, "expected 5 recipes; got {recipes}");
}

// ---------------------------------------------------------------------------
// Test 3: resources/read framewalk://guide/getting-started returns the
//         exact body of docs/getting-started.md from the repo root
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn read_resource_guide_getting_started_returns_docs_body() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"framewalk://guide/getting-started"}}"#,
    ])
    .await;

    let call = find_reply(&replies, 2);
    let contents = call["result"]["contents"]
        .as_array()
        .expect("contents array");
    assert_eq!(contents.len(), 1, "expected exactly one content block");

    let first = &contents[0];
    assert_eq!(
        first["uri"].as_str(),
        Some("framewalk://guide/getting-started")
    );
    assert_eq!(first["mimeType"].as_str(), Some("text/markdown"));

    let text = first["text"].as_str().expect("text field");
    // The body should match the docs/ file byte-for-byte (both via
    // include_str! — the test crate is also compiled from the same
    // repo root).
    let expected = include_str!("../../../docs/getting-started.md");
    assert_eq!(text, expected);
}

// ---------------------------------------------------------------------------
// Test 4: resources/read on a reference page returns non-empty markdown
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn read_resource_reference_breakpoints_non_empty() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"framewalk://reference/breakpoints"}}"#,
    ])
    .await;

    let call = find_reply(&replies, 2);
    let contents = call["result"]["contents"]
        .as_array()
        .expect("contents array");
    let text = contents[0]["text"].as_str().expect("text field");
    assert!(!text.trim().is_empty(), "reference page was empty");
    assert!(
        text.contains("set_breakpoint"),
        "reference/breakpoints should mention set_breakpoint"
    );
}

// ---------------------------------------------------------------------------
// Test 5: resources/read on a recipe — covers the third resource family
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn read_resource_recipe_debug_segfault_mentions_workflow() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"framewalk://recipe/debug-segfault"}}"#,
    ])
    .await;

    let call = find_reply(&replies, 2);
    let contents = call["result"]["contents"]
        .as_array()
        .expect("contents array");
    let text = contents[0]["text"].as_str().expect("text field");
    assert!(!text.trim().is_empty(), "recipe was empty");
    assert!(
        text.contains("backtrace") && text.contains("SIGSEGV"),
        "recipe should walk through the segfault workflow"
    );
}

// ---------------------------------------------------------------------------
// Test 6: resources/read on an unknown URI returns a JSON-RPC error
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn read_resource_unknown_uri_returns_error() {
    let replies = drive_server(&[
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#,
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"framewalk://nonsense"}}"#,
    ])
    .await;

    let call = find_reply(&replies, 2);
    let err = call.get("error").expect("expected JSON-RPC error object");
    let msg = err["message"].as_str().unwrap_or("");
    assert!(
        msg.contains("nonsense") || msg.contains("unknown"),
        "error message should mention the bad URI or be an unknown-uri error: {err:#?}"
    );
}

// ---------------------------------------------------------------------------
// Test 7: every URI from resources/list is readable with non-empty body
//
// This is the strongest end-to-end test: it catches drift between the
// registry declaration and what is actually serveable. If anyone adds a
// new resource entry but forgets the markdown file, the registry tests
// fail at compile time (include_str!), but if anyone adds a new entry
// and the read handler regresses, this is the test that fires.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb via the mcp binary; run with --ignored"]
async fn every_listed_resource_is_readable() {
    // First request: discover the URI set.
    let init = [
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
        r#"{"jsonrpc":"2.0","id":2,"method":"resources/list"}"#.to_string(),
    ];
    let init_refs: Vec<&str> = init.iter().map(String::as_str).collect();
    let replies = drive_server(&init_refs).await;
    let list = find_reply(&replies, 2);
    let items = list["result"]["resources"]
        .as_array()
        .expect("resources array");
    let uris: Vec<String> = items
        .iter()
        .filter_map(|i| i["uri"].as_str().map(str::to_string))
        .collect();
    assert!(!uris.is_empty(), "resources/list returned an empty set");

    // Second request: read every URI. Each read goes through the full
    // server stack (rmcp dispatch, handler, registry lookup), so we
    // catch any regression in the read path that the in-process unit
    // tests would miss.
    let mut messages = vec![
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"test","version":"0"}}}"#.to_string(),
        r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#.to_string(),
    ];
    for (idx, uri) in uris.iter().enumerate() {
        let id = 100 + i64::try_from(idx).expect("uri count fits i64");
        messages.push(format!(
            r#"{{"jsonrpc":"2.0","id":{id},"method":"resources/read","params":{{"uri":"{uri}"}}}}"#
        ));
    }
    let msg_refs: Vec<&str> = messages.iter().map(String::as_str).collect();
    let read_replies = drive_server(&msg_refs).await;

    for (idx, uri) in uris.iter().enumerate() {
        let id = 100 + i64::try_from(idx).expect("uri count fits i64");
        let reply = find_reply(&read_replies, id);
        let contents = reply["result"]["contents"]
            .as_array()
            .unwrap_or_else(|| panic!("no contents for {uri}: {reply:#?}"));
        assert!(
            !contents.is_empty(),
            "empty contents array for {uri}: {reply:#?}"
        );
        let block = &contents[0];
        assert_eq!(block["uri"].as_str(), Some(uri.as_str()));
        assert_eq!(
            block["mimeType"].as_str(),
            Some("text/markdown"),
            "wrong mime for {uri}"
        );
        let text = block["text"].as_str().unwrap_or("");
        assert!(!text.trim().is_empty(), "empty body for {uri}");
    }
}
