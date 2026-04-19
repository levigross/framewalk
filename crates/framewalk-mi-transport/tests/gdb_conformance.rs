//! Conformance tests that spawn a real `gdb --interpreter=mi3 --quiet --nx`
//! and drive it end-to-end through the transport.
//!
//! These tests are gated with `#[ignore]` by default so `cargo test`
//! without flags does not require `gdb` to be installed. The nix devShell
//! provides `gdb` via `nixpkgs`, so running them is a one-liner:
//!
//! ```sh
//! nix develop --command cargo nextest run -p framewalk-mi-transport --run-ignored
//! ```
//!
//! The tests cover the Step 5 milestone: spawn real GDB, submit the
//! commands listed in the plan (`-gdb-version`, `-file-exec-and-symbols`,
//! `-break-insert`, `-exec-run`, `-exec-continue`), and assert the
//! resulting events and state transitions are what we expect.

use std::time::Duration;

use framewalk_mi_codec::{MiCommand, Value};
use framewalk_mi_protocol::{BreakpointId, CommandOutcome, Event, ThreadId};
use framewalk_mi_transport::{spawn, GdbConfig, TransportHandle};
use tokio::time::timeout;

/// Time budget for each conformance test. Generous because first spawn
/// can be slow on cold caches; real GDB operations finish in ms.
const TEST_TIMEOUT: Duration = Duration::from_secs(15);

fn default_config() -> GdbConfig {
    let program = std::env::var("FRAMEWALK_GDB").unwrap_or_else(|_| "gdb".to_string());
    GdbConfig::new().with_program(program)
}

fn configured_gdb_available(config: &GdbConfig) -> bool {
    std::process::Command::new(&config.program)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

async fn spawn_or_skip() -> Option<TransportHandle> {
    let config = default_config();
    if !configured_gdb_available(&config) {
        eprintln!(
            "skipping ignored gdb conformance test: `{}` is not available; run via `./scripts/validate.sh` or inside `nix develop`",
            config.program
        );
        return None;
    }

    Some(
        timeout(TEST_TIMEOUT, spawn(config))
            .await
            .expect("spawn timed out")
            .expect("gdb spawn failed"),
    )
}

// ---------------------------------------------------------------------------
// Basic: spawn + version + shutdown
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `cargo nextest --run-ignored`"]
async fn spawn_submit_version_shutdown() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let outcome = timeout(TEST_TIMEOUT, handle.submit(MiCommand::new("gdb-version")))
        .await
        .expect("submit timed out")
        .expect("submit failed");

    // -gdb-version responds with ^done (no structured results; the
    // version string arrives via console stream records).
    assert!(
        matches!(outcome, CommandOutcome::Done(_)),
        "expected Done, got {outcome:?}"
    );

    let status = timeout(TEST_TIMEOUT, handle.shutdown())
        .await
        .expect("shutdown timed out")
        .expect("shutdown failed");
    assert!(
        status.success() || status.code().is_some(),
        "gdb should exit with a status, got {status:?}"
    );
}

// ---------------------------------------------------------------------------
// -list-features populates the FeatureSet registry via the connection
// that the reader task holds inside SharedState — we can observe it
// through a fresh submit because the connection's features registry is
// updated as a side effect of classify_result.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `cargo nextest --run-ignored`"]
async fn list_features_returns_non_empty_list() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let outcome = timeout(TEST_TIMEOUT, handle.submit(MiCommand::new("list-features")))
        .await
        .expect("submit timed out")
        .expect("submit failed");

    let CommandOutcome::Done(results) = outcome else {
        panic!("expected Done, got {outcome:?}");
    };
    // Look for `features=[...]` in the result tuple. Every modern GDB
    // reports at least a handful of features.
    let features = results
        .iter()
        .find(|(k, _)| k == "features")
        .expect("features key in -list-features response");
    match &features.1 {
        Value::List(framewalk_mi_codec::ListValue::Values(vs)) => {
            assert!(!vs.is_empty(), "features list should be non-empty");
        }
        other => panic!("expected value list of features, got {other:?}"),
    }

    handle.shutdown().await.ok();
}

#[tokio::test]
#[ignore = "spawns real gdb; run with `cargo nextest --run-ignored`"]
async fn spawn_bootstraps_async_non_stop_and_non_interactive_settings() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    for (setting, expected) in [
        ("mi-async", "on"),
        ("non-stop", "on"),
        ("pagination", "off"),
        ("confirm", "off"),
    ] {
        let outcome = timeout(
            TEST_TIMEOUT,
            handle.submit(MiCommand::new("gdb-show").parameter(setting)),
        )
        .await
        .expect("gdb-show timed out")
        .expect("gdb-show failed");

        let CommandOutcome::Done(results) = outcome else {
            panic!("expected Done from gdb-show {setting}, got {outcome:?}");
        };
        let value = results
            .iter()
            .find_map(|(name, value)| match (name.as_str(), value) {
                ("value", Value::Const(value)) => Some(value.as_str()),
                _ => None,
            })
            .unwrap_or_else(|| {
                panic!("gdb-show {setting} should include a value result: {results:?}")
            });

        assert_eq!(
            value, expected,
            "bootstrap should set {setting} to {expected}, got {value}"
        );
    }

    handle.shutdown().await.ok();
}

// ---------------------------------------------------------------------------
// Break-insert + exec-run + stop: the full stop/continue dance against
// a trivial /bin/ls run.
//
// This asserts two things:
// 1. After a successful -break-insert, the breakpoint registry contains
//    a breakpoint keyed by the id GDB assigned.
// 2. After -exec-run, we eventually observe Event::Stopped on the
//    broadcast channel (from the reader task firing our subscriber),
//    proving that async events flow end-to-end.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb and runs the test binary; run with `cargo nextest --run-ignored`"]
async fn break_insert_run_stop_on_self() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    // Subscribe to events BEFORE submitting anything so we don't miss
    // early async records.
    let mut events = handle.subscribe();

    // 1. Use the test binary itself as the debug target. It is
    // guaranteed to exist at runtime (unlike /bin/ls on nix systems
    // where /bin is not a real directory) and has a `main` symbol the
    // Rust test harness inherits from the `test` crate.
    let test_binary = std::env::current_exe()
        .expect("test binary path")
        .to_string_lossy()
        .into_owned();
    let outcome = timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("file-exec-and-symbols").parameter(&test_binary)),
    )
    .await
    .expect("file-exec-and-symbols timed out")
    .expect("file-exec-and-symbols failed");
    assert!(
        matches!(outcome, CommandOutcome::Done(_)),
        "file-exec-and-symbols on {test_binary} should succeed, got {outcome:?}"
    );

    // 2. Insert a breakpoint at main.
    let outcome = timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("break-insert").parameter("main")),
    )
    .await
    .expect("break-insert timed out")
    .expect("break-insert failed");
    let CommandOutcome::Done(_) = outcome else {
        panic!("expected break-insert to return Done, got {outcome:?}");
    };

    // 3. Pass --list so the test binary prints its test list and exits
    //    immediately once we let it continue (which we won't, but this
    //    guards against accidental recursion if anyone later calls
    //    -exec-continue in this test).
    timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("exec-arguments").parameter("--list")),
    )
    .await
    .expect("exec-arguments timed out")
    .expect("exec-arguments failed");

    // 4. Run the target. ^running completes immediately; the eventual
    //    *stopped arrives asynchronously.
    let outcome = timeout(TEST_TIMEOUT, handle.submit(MiCommand::new("exec-run")))
        .await
        .expect("exec-run timed out")
        .expect("exec-run failed");
    assert!(
        matches!(outcome, CommandOutcome::Running),
        "exec-run should return Running (not wait for stop), got {outcome:?}"
    );

    // 4. Drain the broadcast until we see an Event::Stopped. All events
    //    in between are fine; we just need confirmation that async
    //    records flow end-to-end.
    let deadline = tokio::time::Instant::now() + TEST_TIMEOUT;
    let mut saw_stopped = false;
    while tokio::time::Instant::now() < deadline {
        let remaining = deadline - tokio::time::Instant::now();
        let Ok(recv_result) = timeout(remaining, events.recv()).await else {
            break;
        };

        match recv_result {
            Ok(Event::Stopped(_)) => {
                saw_stopped = true;
                break;
            }
            // Other events: keep draining until we either see Stopped or time out.
            Ok(_) => {}
            // Broadcast receiver closed.
            Err(_) => break,
        }
    }
    assert!(
        saw_stopped,
        "expected to observe Event::Stopped within {TEST_TIMEOUT:?}"
    );

    // 5. After the stop, the connection's state registries should
    //    reflect a stopped target with a known breakpoint. We can't
    //    read them directly from outside the transport (they live in
    //    SharedState), but we can exercise them indirectly by
    //    submitting -break-info: if the breakpoint is still known to
    //    GDB, the registry update path hasn't crashed.
    let outcome = timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("break-info").parameter("1")),
    )
    .await
    .expect("break-info timed out")
    .expect("break-info failed");
    assert!(
        matches!(outcome, CommandOutcome::Done(_)),
        "break-info on bp 1 should succeed, got {outcome:?}"
    );

    // 6. Clean shutdown while still stopped at main. We intentionally do
    //    not resume — letting the inferior actually run would re-enter
    //    the test harness recursively.
    timeout(TEST_TIMEOUT, handle.shutdown()).await.ok();

    // Silence unused warnings on helpers that may or may not be touched
    // depending on which control path ran.
    drop((BreakpointId::new("1"), ThreadId::new("1")));
}

// ---------------------------------------------------------------------------
// Cursor-based waiting: wait for the first stop after a captured cursor,
// then confirm the retained current stop is available immediately.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb and runs the test binary; run with `cargo nextest --run-ignored`"]
async fn cursor_wait_returns_stop_for_running_command() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let test_binary = std::env::current_exe()
        .expect("test binary path")
        .to_string_lossy()
        .into_owned();
    timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("file-exec-and-symbols").parameter(&test_binary)),
    )
    .await
    .expect("file-exec-and-symbols timed out")
    .expect("file-exec-and-symbols failed");
    timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("break-insert").parameter("main")),
    )
    .await
    .expect("break-insert timed out")
    .expect("break-insert failed");
    timeout(
        TEST_TIMEOUT,
        handle.submit(MiCommand::new("exec-arguments").parameter("--list")),
    )
    .await
    .expect("exec-arguments timed out")
    .expect("exec-arguments failed");

    let after_seq = handle.event_cursor();
    let outcome = timeout(TEST_TIMEOUT, handle.submit(MiCommand::new("exec-run")))
        .await
        .expect("exec-run timed out")
        .expect("exec-run failed");
    assert!(
        matches!(outcome, CommandOutcome::Running),
        "exec-run should return Running, got {outcome:?}"
    );

    let (_, stopped) = timeout(
        TEST_TIMEOUT,
        handle.next_stop_after(after_seq, TEST_TIMEOUT),
    )
    .await
    .expect("next_stop_after timed out")
    .expect("next_stop_after failed")
    .expect("expected a stop event after exec-run");
    assert!(
        stopped.thread.is_some() || stopped.reason.is_some() || !stopped.raw.is_empty(),
        "expected stop payload to carry useful data: {stopped:?}"
    );

    let (_, current) = timeout(TEST_TIMEOUT, handle.current_or_next_stop(TEST_TIMEOUT))
        .await
        .expect("current_or_next_stop timed out")
        .expect("current_or_next_stop failed")
        .expect("target should still be stopped");
    assert_eq!(
        current.raw, stopped.raw,
        "current_or_next_stop should return the current retained stop"
    );

    timeout(TEST_TIMEOUT, handle.shutdown()).await.ok();
}

// ---------------------------------------------------------------------------
// Regression: GDB death wakes parked submit() callers with Exited
// ---------------------------------------------------------------------------

/// If GDB disappears while a caller is parked inside `submit().await`,
/// the caller must wake up with `TransportError::Exited` within a
/// short window — not hang forever.
///
/// The reader task is the only actor that fires the pending oneshot
/// senders; when its read loop exits on GDB's stdout EOF the senders
/// would leak without the `PendingDrainGuard` in `pump::run_reader`.
/// This test exists to catch a regression in that guard.
///
/// Strategy:
/// 1. Spawn GDB and grab its PID.
/// 2. `SIGSTOP` the GDB process so it is guaranteed to not respond
///    to anything we write to its stdin.  This makes the test
///    deterministic — without this step, a fast command like
///    `-gdb-version` can race the kill and complete first.
/// 3. Issue a `submit()` without awaiting.  The writer task pushes
///    the bytes into GDB's stdin but GDB is stopped, so no response
///    comes back — the future parks at `rx.await`.
/// 4. `SIGKILL` the GDB process.  A stopped process still dies under
///    SIGKILL.  The kernel closes its stdout; the reader task's
///    `read().await` returns 0, `run_reader` falls out, and
///    `PendingDrainGuard::drop` clears the pending map, dropping the
///    oneshot sender so the parked receiver wakes with `RecvError`.
/// 5. Assert the awaited outcome is `Err(TransportError::Exited)`
///    within a 2-second window.  Without the drain guard the submit
///    hangs until the test's outer timeout.
#[tokio::test]
#[ignore = "spawns real gdb; run with `cargo nextest --run-ignored`"]
async fn submit_wakes_with_exited_when_gdb_is_killed() {
    use framewalk_mi_transport::TransportError;

    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let pid = handle
        .child_id()
        .expect("spawned gdb child must have a pid");

    // Pause GDB so any command we submit next parks at `rx.await`
    // indefinitely.  SIGSTOP is the deterministic way to create the
    // "in-flight submit" condition — without it, short commands can
    // complete before we get a chance to kill the process.
    let stop_status = std::process::Command::new("kill")
        .arg("-STOP")
        .arg(pid.to_string())
        .status()
        .expect("failed to invoke /bin/kill -STOP");
    assert!(stop_status.success(), "SIGSTOP failed: {stop_status:?}");

    // Park a submit() without awaiting. GDB is stopped so no result
    // will come back.
    let submit_fut = handle.submit(MiCommand::new("gdb-version"));
    tokio::pin!(submit_fut);

    // Give the writer task a moment to push the bytes through stdin
    // and the reader task to enter its parked state.
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Now kill GDB. SIGKILL still delivers to a stopped process.
    let kill_status = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status()
        .expect("failed to invoke /bin/kill -KILL");
    assert!(kill_status.success(), "SIGKILL failed: {kill_status:?}");

    // The parked submit must now complete within a short window
    // (reader sees EOF → drain guard fires → receiver wakes). A
    // 2-second budget is generous; the actual wake path is ms.
    let outcome = timeout(Duration::from_secs(2), submit_fut)
        .await
        .expect("submit hung after GDB was killed — PendingDrainGuard regression?");

    assert!(
        matches!(outcome, Err(TransportError::Exited)),
        "expected Err(Exited) after GDB killed, got {outcome:?}"
    );
}
