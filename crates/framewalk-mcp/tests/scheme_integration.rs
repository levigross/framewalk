//! Integration tests for the Steel Scheme scripting layer.
//!
//! Exercises `SchemeHandle` directly against a real GDB subprocess.
//! Gated `#[ignore]` because GDB must be on PATH (nix devShell).
//!
//! ```sh
//! nix develop --command cargo nextest run -p framewalk-mcp \
//!     --test scheme_integration --run-ignored all
//! ```

use std::sync::Arc;
use std::time::Duration;

use framewalk_mcp::{SchemeHandle, SchemeSettings};
use framewalk_mi_transport::{spawn, GdbConfig};
use tokio::time::timeout;

const TEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Shared setup: spawn GDB and a scheme worker, returning handles.
async fn setup() -> (Arc<framewalk_mi_transport::TransportHandle>, SchemeHandle) {
    let transport = timeout(TEST_TIMEOUT, spawn(GdbConfig::new()))
        .await
        .expect("spawn timed out")
        .expect("gdb spawn failed");
    let transport = Arc::new(transport);
    let rt = tokio::runtime::Handle::current();
    let scheme = timeout(
        TEST_TIMEOUT,
        SchemeHandle::spawn(
            Arc::clone(&transport),
            false,
            rt,
            SchemeSettings {
                eval_timeout: TEST_TIMEOUT,
                wait_timeout: TEST_TIMEOUT,
            },
        ),
    )
    .await
    .expect("scheme spawn timed out")
    .expect("scheme spawn failed");
    (transport, scheme)
}

/// Helper: eval with timeout.  Returns only the display string so
/// existing assertions (which only ever examined the rendered output)
/// keep working after `SchemeHandle::eval` was upgraded to return a
/// structured reply.
async fn eval(scheme: &SchemeHandle, code: &str) -> Result<String, String> {
    timeout(TEST_TIMEOUT, scheme.eval(code.to_string(), None, false))
        .await
        .expect("scheme eval timed out")
        .map(|reply| reply.display)
}

// =========================================================================
// Pure Scheme (no GDB interaction)
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn pure_arithmetic() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, "(+ 1 2 3 4 5)").await;
    assert_eq!(result.expect("eval failed"), "15");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn pure_string_ops() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(string-append "hello" " " "world")"#).await;
    assert_eq!(result.expect("eval failed"), r#""hello world""#);
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn pure_list_ops() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, "(map (lambda (x) (* x x)) '(1 2 3 4))").await;
    assert_eq!(result.expect("eval failed"), "(1 4 9 16)");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn state_persists_across_calls() {
    let (_transport, scheme) = setup().await;

    eval(&scheme, "(define my-counter 0)")
        .await
        .expect("define failed");
    eval(&scheme, "(set! my-counter (+ my-counter 10))")
        .await
        .expect("set! failed");
    eval(&scheme, "(set! my-counter (+ my-counter 32))")
        .await
        .expect("set! failed");

    let result = eval(&scheme, "my-counter").await.expect("read failed");
    assert_eq!(result, "42");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn define_and_call_custom_function() {
    let (_transport, scheme) = setup().await;

    eval(
        &scheme,
        "(define (factorial n) (if (<= n 1) 1 (* n (factorial (- n 1)))))",
    )
    .await
    .expect("define failed");

    let result = eval(&scheme, "(factorial 10)").await.expect("call failed");
    assert_eq!(result, "3628800");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn syntax_error_returns_err() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, "(defin x 42)").await;
    assert!(result.is_err(), "malformed code should fail: {result:?}");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn runtime_error_returns_err() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, "(/ 1 0)").await;
    assert!(result.is_err(), "division by zero should fail: {result:?}");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn error_does_not_corrupt_engine() {
    let (_transport, scheme) = setup().await;

    // Cause an error.
    let _ = eval(&scheme, "(error \"boom\")").await;

    // Engine should still work.
    let result = eval(&scheme, "(+ 100 200)")
        .await
        .expect("engine broken after error");
    assert_eq!(result, "300");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn multiple_expressions_return_last() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, "(define a 1)\n(define b 2)\n(+ a b)")
        .await
        .expect("eval failed");
    // engine.run returns all top-level expression results; display shows all.
    assert!(result.contains('3'), "result should contain 3: {result}");
}

// =========================================================================
// mi-quote and mi-cmd — parameter safety
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_quote_bare_word() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi-quote "main")"#)
        .await
        .expect("mi-quote failed");
    // Bare word: returned as-is (no quotes).
    assert_eq!(result, r#""main""#);
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_quote_with_spaces() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi-quote "/path/with spaces/file")"#)
        .await
        .expect("mi-quote failed");
    // Should be c-string-quoted.
    assert_eq!(result, r#""\"/path/with spaces/file\"""#);
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_cmd_gdb_version() {
    let (_transport, scheme) = setup().await;
    // mi-cmd builds a structured MI command — test it directly.
    let result = eval(&scheme, r#"(mi-cmd "gdb-version")"#)
        .await
        .expect("mi-cmd gdb-version failed");
    assert!(
        !result.is_empty(),
        "gdb-version should return non-empty result"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_cmd_with_parameter() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();
    // mi-cmd should properly quote the parameter, even if it has spaces.
    let result = eval(
        &scheme,
        &format!(r#"(mi-cmd "file-exec-and-symbols" "{path}")"#),
    )
    .await
    .expect("mi-cmd load failed");
    assert!(!result.is_empty(), "load-file via mi-cmd should succeed");
}

// =========================================================================
// GDB interaction — session basics
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_gdb_version() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi "-gdb-version")"#)
        .await
        .expect("mi failed");
    // The result is a hash-map; its display should contain something
    // from the version response (like "msg").
    assert!(
        !result.is_empty(),
        "gdb-version should return non-empty result"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn prelude_gdb_version() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(result-field "version" (gdb-version))"#)
        .await
        .expect("prelude gdb-version failed");
    assert!(
        !result.is_empty(),
        "prelude (gdb-version) should return a non-empty version field"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_list_features() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi "-list-features")"#)
        .await
        .expect("mi failed");
    // Features response contains a list under the "features" key.
    assert!(!result.is_empty(), "features should be non-empty: {result}");
}

// =========================================================================
// Security — raw_guard is enforced in Scheme
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_rejects_interpreter_exec() {
    let (_transport, scheme) = setup().await;
    let result = eval(
        &scheme,
        r#"(mi "-interpreter-exec console \"shell echo pwned\"")"#,
    )
    .await;
    assert!(
        result.is_err(),
        "shell pivot should be rejected: {result:?}"
    );
    let err = result.expect_err("should be error");
    assert!(
        err.contains("rejected"),
        "error should mention rejection: {err}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_rejects_raw_cli() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi "info break")"#).await;
    assert!(
        result.is_err(),
        "raw CLI (no dash prefix) should be rejected: {result:?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn mi_rejects_empty() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi "")"#).await;
    assert!(
        result.is_err(),
        "empty MI command should be rejected: {result:?}"
    );
}

// =========================================================================
// GDB interaction — load, breakpoint, run, stop, inspect
// =========================================================================

/// Load the test binary itself into GDB — guaranteed to exist and have
/// a `main` symbol.
fn test_binary_path() -> String {
    std::env::current_exe()
        .expect("current_exe")
        .to_string_lossy()
        .into_owned()
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn load_file_via_prelude() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();
    let result = eval(&scheme, &format!(r#"(load-file "{path}")"#))
        .await
        .expect("load-file failed");
    // Successful load returns a hash-map (^done result).
    assert!(!result.is_empty(), "load-file should return results");
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn breakpoint_set_and_list() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    eval(&scheme, &format!(r#"(load-file "{path}")"#))
        .await
        .expect("load-file failed");

    let bp = eval(&scheme, r#"(set-breakpoint "main")"#)
        .await
        .expect("set-breakpoint failed");
    assert!(!bp.is_empty(), "breakpoint result should be non-empty");

    let list = eval(&scheme, "(list-breakpoints)")
        .await
        .expect("list-breakpoints failed");
    assert!(
        list.contains("main"),
        "breakpoint list should mention main: {list}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn full_run_stop_backtrace_cycle() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    // Compose the entire workflow in a single Scheme expression.
    // Uses run-and-wait (not run + wait-for-stop) to avoid the
    // subscription race — see docs/scheme-reference.md.
    let code = format!(
        r#"(begin
            (load-file "{path}")
            (set-breakpoint "main")
            (run-and-wait)
            (backtrace))"#
    );

    let result = eval(&scheme, &code).await.expect("full cycle failed");
    assert!(
        result.contains("main"),
        "backtrace should mention main: {result}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn run_to_prelude_helper() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    eval(&scheme, &format!(r#"(load-file "{path}")"#))
        .await
        .expect("load-file failed");

    // run-to sets a temp breakpoint, runs, and waits for stop.
    let result = eval(&scheme, r#"(run-to "main")"#)
        .await
        .expect("run-to failed");
    // Returns a stopped-event hash-map.
    assert!(
        !result.is_empty(),
        "run-to should return a stopped event: {result}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn inspect_expression() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    let code = format!(
        r#"(begin
            (load-file "{path}")
            (set-breakpoint "main")
            (run-and-wait)
            (inspect "1+2"))"#
    );

    let result = eval(&scheme, &code).await.expect("inspect failed");
    // The expression "1 + 2" should evaluate to "3" in GDB.
    assert!(
        result.contains('3'),
        "inspect(1+2) should contain 3: {result}"
    );
}

// =========================================================================
// Composition — multi-step workflows
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn step_n_composition() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    // `step-n 3` must (a) return a list of length 3 and (b) each
    // element must be a genuine `*stopped` hash-map with a `reason`
    // field.  Prior to the step-and-wait rewrite, `step-n` returned a
    // list of GDB `^error` records because the loop issued each
    // `-exec-step` while the target was still running from the
    // previous one — the old test only checked length and happily
    // passed on that broken output.  The `every-has-reason?` check
    // below is the load-bearing assertion.
    let code = format!(
        r#"(begin
            (load-file "{path}")
            (set-breakpoint "main")
            (run-and-wait)
            (define stops (step-n 3))
            (define (every-has-reason? xs)
              (cond ((null? xs) #t)
                    ((hash-contains? (car xs) "reason")
                     (every-has-reason? (cdr xs)))
                    (else #f)))
            (list (length stops) (every-has-reason? stops)))"#
    );

    let result = eval(&scheme, &code).await.expect("step-n failed");
    assert!(
        result.contains('3') && result.contains("#t"),
        "step-n 3 should return three hash-maps each with a \"reason\" field; got: {result}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn result_field_accessor() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    // Load file and extract a specific field from the breakpoint result.
    let code = format!(
        r#"(begin
            (load-file "{path}")
            (define bp (set-breakpoint "main"))
            (result-field "bkpt" bp))"#
    );

    let result = eval(&scheme, &code).await.expect("result-field failed");
    // The "bkpt" field should be the breakpoint detail entry list.
    assert!(
        !result.is_empty(),
        "result-field should extract bkpt: {result}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn scheme_loop_collecting_locals() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    // A more complex workflow: stop at main, step a few times, collect
    // backtrace after each step.  Uses run-and-wait and step-and-wait
    // throughout to avoid the subscription race — see
    // docs/scheme-reference.md.
    let code = format!(
        r#"(begin
            (load-file "{path}")
            (set-breakpoint "main")
            (run-and-wait)
            (define traces
              (let loop ((i 0) (acc '()))
                (if (>= i 2)
                    (reverse acc)
                    (begin
                      (step-and-wait)
                      (loop (+ i 1)
                            (cons (backtrace) acc))))))
            (length traces))"#
    );

    let result = eval(&scheme, &code).await.expect("loop failed");
    assert_eq!(result, "2", "should have 2 backtraces: {result}");
}

// =========================================================================
// Concurrency / ordering
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn sequential_evals_are_serialised() {
    let (_transport, scheme) = setup().await;

    // Fire multiple evals; they should serialize on the worker thread.
    let h1 = scheme.eval("(define seq-a 10)".to_string(), None, false);
    let h2 = scheme.eval("(define seq-b 20)".to_string(), None, false);
    let h3 = scheme.eval("(+ seq-a seq-b)".to_string(), None, false);

    let _ = timeout(TEST_TIMEOUT, h1).await.expect("h1 timed out");
    let _ = timeout(TEST_TIMEOUT, h2).await.expect("h2 timed out");
    let r3 = timeout(TEST_TIMEOUT, h3)
        .await
        .expect("h3 timed out")
        .expect("h3 eval failed");
    assert_eq!(r3.display, "30");
}

// =========================================================================
// list-threads via prelude (exercises ThreadInfoArgs)
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn list_threads_via_prelude() {
    let (_transport, scheme) = setup().await;
    let path = test_binary_path();

    // Load, set breakpoint at main, run, stop — now we have at least one thread.
    let code = format!(
        r#"(begin
            (load-file "{path}")
            (set-breakpoint "main")
            (run-and-wait)
            (list-threads))"#
    );

    let result = eval(&scheme, &code).await.expect("list-threads failed");
    // The thread-info result should contain thread details.
    assert!(
        !result.is_empty(),
        "list-threads should return thread info: {result}"
    );
}

// =========================================================================
// Raw-guard allowlist permits trace commands
// =========================================================================

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn raw_guard_allows_trace_commands() {
    let (_transport, scheme) = setup().await;
    // trace-status should be accepted by the allowlist (trace- family).
    let result = eval(&scheme, r#"(mi "-trace-status")"#).await;
    // It might return an error from GDB (not started), but should NOT
    // be rejected by the raw guard.
    assert!(
        result.is_ok() || !result.as_ref().unwrap_err().contains("rejected"),
        "trace-status should not be blocked by raw guard: {result:?}"
    );
}

#[tokio::test]
#[ignore = "spawns real gdb; run with --run-ignored"]
async fn raw_guard_rejects_unknown_family() {
    let (_transport, scheme) = setup().await;
    let result = eval(&scheme, r#"(mi "-unknown-command foo")"#).await;
    assert!(
        result.is_err(),
        "unknown command family should be rejected: {result:?}"
    );
    let err = result.expect_err("should be error");
    assert!(
        err.contains("rejected") || err.contains("allowlist"),
        "error should indicate rejection: {err}"
    );
}
