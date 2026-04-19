//! Lifecycle tests for [`TransportHandle`] — focused on the shutdown path,
//! the bootstrap command sequence, and the snapshot consistency guarantees.
//!
//! These tests spawn a real `gdb` under the same contract as
//! `gdb_conformance.rs`, so they're gated with `#[ignore]` and skip
//! cleanly when no gdb is on `PATH`. Run via:
//!
//! ```sh
//! nix develop --command cargo test -p framewalk-mi-transport --test handle_lifecycle -- --ignored
//! ```

use std::time::Duration;

use framewalk_mi_codec::MiCommand;
use framewalk_mi_protocol::{CommandOutcome, Event};
use framewalk_mi_transport::{spawn, GdbConfig, TransportHandle};
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(15);

// NOTE: These helpers are duplicated from `gdb_conformance.rs` on purpose.
// The repo's test convention (see `tests/resources_roundtrip.rs:39-42`)
// prefers duplication over `#[path]` module-declaration churn across
// integration test files, since each test binary has its own crate root.

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
            "skipping ignored handle_lifecycle test: `{}` is not available; run via `./scripts/validate.sh` or inside `nix develop`",
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
// Shutdown happy path: `-gdb-exit` completes and the child exits cleanly
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `--ignored`"]
async fn shutdown_returns_child_exit_status() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let status = timeout(TEST_TIMEOUT, handle.shutdown())
        .await
        .expect("shutdown timed out")
        .expect("shutdown failed");

    // GDB normally exits 0 on `-gdb-exit`. We don't assert the exact
    // status because different platforms surface the signal-vs-code
    // distinction differently — just that we got one back.
    // The shutdown path must return *some* status when GDB cooperates.
    let _ = status;
}

// ---------------------------------------------------------------------------
// Shutdown from outside: if the child is already gone before shutdown, the
// path still terminates (falls through the kill-on-timeout branch or the
// early-exited-error branch) and does not hang.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `--ignored`"]
async fn shutdown_after_child_external_kill_still_returns() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    // Take the process ID and terminate it out-of-band to simulate GDB
    // crashing or being killed by a supervisor. `shutdown` must then still
    // converge — either via the `-gdb-exit` error path or the fallback.
    let pid = handle.child_id().expect("child has a pid");

    // Send SIGKILL so GDB has no chance to respond cleanly.
    // Shelling out to /bin/kill keeps the test `unsafe_code = "deny"`-clean
    // and matches the pattern used in gdb_conformance.rs.
    let kill_status = std::process::Command::new("kill")
        .arg("-KILL")
        .arg(pid.to_string())
        .status()
        .expect("failed to invoke /bin/kill -KILL");
    assert!(kill_status.success(), "SIGKILL failed: {kill_status:?}");

    // Give the reader task a moment to observe EOF on stdout so the
    // shutdown path takes the already-exited branch rather than racing it.
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Drain any outstanding events so we don't block the broadcast channel.
    let _ = handle.subscribe();

    // The contract is that shutdown *converges* after the child is gone,
    // not that it succeeds — we accept either Ok(status) or Err.
    timeout(TEST_TIMEOUT, handle.shutdown())
        .await
        .expect("shutdown hung after external SIGKILL")
        .ok();
}

// ---------------------------------------------------------------------------
// Bootstrap order: after a fresh spawn, the transport has already issued
// the bootstrap commands (`-gdb-set mi-async on`, `-gdb-set non-stop on`,
// `-gdb-set pagination off`, `-gdb-set confirm off`). We verify this by
// asking GDB about the resulting settings — if bootstrap didn't run, these
// `-gdb-show` replies will carry the defaults instead.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `--ignored`"]
async fn bootstrap_turns_on_mi_async_non_stop_and_disables_pagination() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    // Probe each bootstrap setting. The reply shape is
    // `^done,value="on"` (or "off") per the GDB manual.
    for (setting, expected) in [
        ("mi-async", "on"),
        ("non-stop", "on"),
        ("pagination", "off"),
        ("confirm", "off"),
    ] {
        let cmd = MiCommand::new("gdb-show").parameter(setting);
        let outcome = timeout(TEST_TIMEOUT, handle.submit(cmd))
            .await
            .expect("submit timed out")
            .expect("submit failed");
        let results = match outcome {
            CommandOutcome::Done(r) => r,
            other => panic!("expected Done for -gdb-show {setting}, got {other:?}"),
        };
        let value = results
            .iter()
            .find_map(|(k, v)| match (k.as_str(), v) {
                ("value", framewalk_mi_codec::Value::Const(s)) => Some(s.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no value field in -gdb-show {setting} reply"));
        assert_eq!(
            value, expected,
            "bootstrap should have set {setting}={expected}, got {value}"
        );
    }

    // Tear down cleanly.
    let _ = handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// snapshot() is consistent: every registry comes from the same instant.
// We assert the cheaper property — that cloned snapshots are independent
// (mutations to the live connection via further commands don't mutate an
// earlier snapshot).
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `--ignored`"]
async fn snapshot_is_isolated_from_subsequent_state_changes() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let before = handle.snapshot();
    // Before any target is loaded, threads should be empty and target
    // state should not be Running.
    assert!(before.threads.is_empty());
    assert!(!before.target.is_running());

    // Issue a -list-features request. This populates the FeatureSet inside
    // the live connection but must NOT retroactively mutate `before`.
    let _ = timeout(TEST_TIMEOUT, handle.submit(MiCommand::new("list-features")))
        .await
        .expect("submit timed out")
        .expect("submit failed");

    let after = handle.snapshot();
    // The live snapshot should now have features; the old one should not.
    // FeatureSet doesn't expose a len(), so we diff via its Debug output —
    // the important invariant is that `before` did not retroactively change.
    let before_features = format!("{:?}", before.features);
    let after_features = format!("{:?}", after.features);
    assert_ne!(
        before_features, after_features,
        "snapshot taken before -list-features must not reflect later mutations"
    );

    let _ = handle.shutdown().await;
}

// ---------------------------------------------------------------------------
// record_synthetic_log: server-side advisories land in the event journal
// and are observable via events_after.
// ---------------------------------------------------------------------------

#[tokio::test]
#[ignore = "spawns real gdb; run with `--ignored`"]
async fn record_synthetic_log_is_observable_after_cursor() {
    let Some(handle) = spawn_or_skip().await else {
        return;
    };

    let cursor = handle.event_cursor();
    let seq = handle.record_synthetic_log("framewalk: advisory".to_string());
    assert!(seq > cursor);

    let events = handle.events_after(cursor);
    let found = events.iter().any(|(s, e)| {
        *s == seq && matches!(e, Event::Log(text) if text.contains("framewalk: advisory"))
    });
    assert!(
        found,
        "synthetic log should be in the journal; got {events:#?}"
    );

    let _ = handle.shutdown().await;
}
