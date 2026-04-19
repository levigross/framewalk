//! Rust functions registered into the Steel Scheme engine.
//!
//! These are the irreducible primitives that cross the Rust ↔ Scheme
//! boundary.  Everything else — convenience wrappers, composition
//! helpers — is pure Scheme in [`prelude.scm`].
//!
//! Both functions capture `Arc<TransportHandle>` and a
//! `tokio::runtime::Handle`, using `Handle::block_on` to bridge from
//! the synchronous scheme worker thread into the async transport.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use framewalk_mi_codec::{MiCommand, Value};
use framewalk_mi_protocol::CommandOutcome;
use framewalk_mi_transport::TransportHandle;
use steel::gc::Gc;
use steel::rerrs::{ErrorKind, SteelErr};
use steel::rvals::{SteelHashMap, SteelString, SteelVal};
use steel::steel_vm::engine::Engine;
use steel::steel_vm::register_fn::RegisterFn;
use steel::HashMap;

use crate::raw_guard::validate_raw_mi_command;
use crate::scheme::marshal;
use crate::scheme::SchemeSettings;
use crate::server_helpers::{self, timeout_context_message};

/// Shared deadline for the current `scheme_eval` invocation.
///
/// Set by the worker thread before each eval; read by wait primitives
/// to detect when a requested wait timeout would exceed the remaining
/// eval budget.  `None` means no deadline is active (e.g. during
/// prelude load).
pub(crate) type EvalDeadline = Arc<Mutex<Option<Instant>>>;

/// Register all framewalk-specific primitives into `engine`.
///
/// The `deadline` is a shared cell that the worker must update before
/// each `engine.run()` call so that wait primitives can validate their
/// timeout against the remaining eval budget.
pub(crate) fn register_all(
    engine: &mut Engine,
    transport: Arc<TransportHandle>,
    allow_shell: bool,
    rt: &tokio::runtime::Handle,
    settings: SchemeSettings,
    deadline: &EvalDeadline,
) {
    register_mi(engine, Arc::clone(&transport), allow_shell, rt.clone());
    register_gdb_version(engine, Arc::clone(&transport), rt.clone());
    register_mi_quote(engine);
    register_trigger_and_wait_primitives(engine, &transport, rt, settings.wait_timeout, deadline);
    register_wait_for_stop(
        engine,
        Arc::clone(&transport),
        rt.clone(),
        settings.wait_timeout,
        deadline,
    );
    register_drain_events(engine, transport);
}

// ---------------------------------------------------------------------------
// (mi-quote string) → string
// ---------------------------------------------------------------------------

/// Register the `mi-quote` primitive.
///
/// `(mi-quote "hello world")` returns `"\"hello world\""` — the MI
/// c-string encoding of the input, suitable for interpolating into a raw
/// MI command string. Strings that don't need quoting (no spaces, quotes,
/// backslashes, or control characters) are returned as-is.
///
/// This is the building block that `mi-cmd` (defined in the Scheme
/// prelude) uses to safely construct MI command strings from dynamic
/// arguments, preventing parameter injection from paths with spaces,
/// expressions with quotes, etc.
fn register_mi_quote(engine: &mut Engine) {
    engine.register_fn("mi-quote", |s: String| -> String { mi_quote_param(&s) });
}

/// Apply MI parameter quoting: if the string is a valid `non-blank-sequence`
/// (non-empty, no spaces/tabs/newlines/quotes/backslashes), return it as-is.
/// Otherwise, wrap it in a c-string literal with ISO C escapes.
///
/// This mirrors the quoting logic in `framewalk_mi_codec::encode`, but
/// produces a `String` instead of appending to a `Vec<u8>`.
fn mi_quote_param(s: &str) -> String {
    use std::fmt::Write;
    if !s.is_empty()
        && !s.bytes().any(|b| {
            matches!(
                b,
                b' ' | b'\t' | b'\n' | b'\r' | b'"' | b'\\' | 0x00..=0x1f | 0x7f
            )
        })
    {
        return s.to_string();
    }
    // Needs c-string encoding.
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for &b in s.as_bytes() {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x07 => out.push_str("\\a"),
            0x08 => out.push_str("\\b"),
            0x0b => out.push_str("\\v"),
            0x0c => out.push_str("\\f"),
            0x00..=0x1f | 0x7f => {
                _ = write!(out, "\\x{b:02x}");
            }
            _ => out.push(b as char),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// (mi command-string) → result-entry-list | symbol | error
// ---------------------------------------------------------------------------

/// Register the `mi` primitive.
///
/// `(mi "-break-insert main")` submits the raw MI command to GDB and
/// returns the result as a lossless Scheme result-entry list. Shell-adjacent commands
/// are rejected by the same [`validate_raw_mi_command`] guard that
/// protects the `mi_raw_command` MCP tool.
fn register_mi(
    engine: &mut Engine,
    transport: Arc<TransportHandle>,
    allow_shell: bool,
    rt: tokio::runtime::Handle,
) {
    engine.register_fn("mi", move |command: String| -> Result<SteelVal, SteelErr> {
        // Security gate — same boundary as mi_raw_command.
        validate_raw_mi_command(&command, allow_shell).map_err(|rejection| {
            SteelErr::new(
                ErrorKind::Generic,
                format!("mi command rejected: {}", rejection.reason()),
            )
        })?;

        let outcome = rt
            .block_on(transport.submit_raw(&command))
            .map_err(|err| SteelErr::new(ErrorKind::Generic, format!("transport error: {err}")))?;

        server_helpers::remember_successful_raw_target_select(&transport, &command, &outcome);
        marshal::outcome_to_steel(&outcome)
    });
}

// ---------------------------------------------------------------------------
// (gdb-version) → result-entry-list with "version"
// ---------------------------------------------------------------------------

/// Register the `gdb-version` primitive.
///
/// GDB reports its banner for `-gdb-version` via console-stream output
/// rather than the `^done` result tuple, so the raw `(mi "-gdb-version")`
/// path is intentionally still empty. This helper captures the banner
/// and returns it as a synthetic result-entry list so callers can use
/// `(result-field "version" (gdb-version))`.
fn register_gdb_version(
    engine: &mut Engine,
    transport: Arc<TransportHandle>,
    rt: tokio::runtime::Handle,
) {
    engine.register_fn("gdb-version", move || -> Result<SteelVal, SteelErr> {
        let before_seq = transport.event_cursor();
        let outcome = rt
            .block_on(transport.submit(MiCommand::new("gdb-version")))
            .map_err(|err| SteelErr::new(ErrorKind::Generic, format!("transport error: {err}")))?;

        match outcome {
            CommandOutcome::Done(_) | CommandOutcome::Connected(_) => {
                let version = server_helpers::collect_console_text_since(&transport, before_seq)
                    .unwrap_or_default();
                marshal::outcome_to_steel(&CommandOutcome::Done(vec![(
                    "version".to_string(),
                    Value::Const(version),
                )]))
            }
            other => marshal::outcome_to_steel(&other),
        }
    });
}

// ---------------------------------------------------------------------------
// (wait-for-stop) → hash-map
// ---------------------------------------------------------------------------

/// Register the `wait-for-stop` primitive.
///
/// Blocks the Scheme thread until GDB reports a `*stopped` event.
/// Returns a hash-map with `"reason"`, `"thread"`, and `"raw"` keys
/// (when available).  Times out after [`WAIT_FOR_STOP_TIMEOUT`].
///
/// The transport now tracks a sequenced event journal, so standalone
/// `wait-for-stop` returns immediately if the target is already stopped
/// and otherwise waits for the next stop after the current cursor.
/// For trigger-then-wait flows, the [`run-and-wait`](register_trigger_and_wait_primitives),
/// `cont-and-wait`, `step-and-wait`, `next-and-wait`, `finish-and-wait`,
/// and `until-and-wait` primitives remain the preferred interface
/// because they capture the cursor before issuing the command and then
/// wait for the first stop strictly after that point.
fn register_wait_for_stop(
    engine: &mut Engine,
    transport: Arc<TransportHandle>,
    rt: tokio::runtime::Handle,
    default_timeout: Duration,
    deadline: &EvalDeadline,
) {
    let transport_default = Arc::clone(&transport);
    let rt_default = rt.clone();
    let deadline_default = Arc::clone(deadline);
    engine.register_fn(
        "wait-for-stop/default",
        move || -> Result<SteelVal, SteelErr> {
            let timeout = clamp_to_eval_budget(default_timeout, &deadline_default)?;
            wait_for_stop_with_timeout(&transport_default, &rt_default, timeout)
        },
    );

    let deadline_timeout = Arc::clone(deadline);
    engine.register_fn(
        "wait-for-stop/timeout",
        move |seconds: isize| -> Result<SteelVal, SteelErr> {
            let timeout = timeout_from_seconds(seconds)?;
            check_eval_budget(timeout, &deadline_timeout)?;
            wait_for_stop_with_timeout(&transport, &rt, timeout)
        },
    );
}

fn wait_for_stop_with_timeout(
    transport: &Arc<TransportHandle>,
    rt: &tokio::runtime::Handle,
    timeout: Duration,
) -> Result<SteelVal, SteelErr> {
    let before_seq = transport.event_cursor();
    match rt.block_on(transport.current_or_next_stop(timeout)) {
        Ok(Some((_, ev))) => stopped_event_to_steel(&ev),
        Ok(None) => Err(timeout_error_with_warnings(
            "wait-for-stop",
            transport,
            before_seq,
            timeout,
        )),
        Err(err) => Err(SteelErr::new(
            ErrorKind::Generic,
            format!("transport error: {err}"),
        )),
    }
}

// ---------------------------------------------------------------------------
// (run-and-wait) / (cont-and-wait) / (step-and-wait) / … → hash-map
// ---------------------------------------------------------------------------

/// Register the race-free trigger-and-wait primitives.
///
/// Each primitive:
///
/// 1. Captures the event cursor **before** submitting the command —
///    this closes the window where a `*stopped` event could arrive
///    between the trigger and the wait registration.
/// 2. Submits the MI command via `submit_raw`.
/// 3. If the outcome is `Running`, waits for the next `*stopped`
///    event (with timeout).  For any other outcome, returns it as-is
///    via [`marshal::outcome_to_steel`] — that path already raises a
///    `SteelErr` for `Error`/`Exit`.
///
/// The no-argument primitives (`run-and-wait`, `cont-and-wait`,
/// `step-and-wait`, `next-and-wait`, `finish-and-wait`) differ only in
/// the literal MI command string.  `until-and-wait` takes a location
/// string and quotes it via the same `mi_quote_param` used by
/// `mi-quote`.
fn register_trigger_and_wait_primitives(
    engine: &mut Engine,
    transport: &Arc<TransportHandle>,
    rt: &tokio::runtime::Handle,
    default_timeout: Duration,
    deadline: &EvalDeadline,
) {
    // Fixed-command variants — same shape, different literal.
    let fixed: &[(&str, &str, &str)] = &[
        ("run-and-wait/default", "run-and-wait/timeout", "-exec-run"),
        (
            "cont-and-wait/default",
            "cont-and-wait/timeout",
            "-exec-continue",
        ),
        (
            "step-and-wait/default",
            "step-and-wait/timeout",
            "-exec-step",
        ),
        (
            "next-and-wait/default",
            "next-and-wait/timeout",
            "-exec-next",
        ),
        (
            "finish-and-wait/default",
            "finish-and-wait/timeout",
            "-exec-finish",
        ),
    ];

    for (default_name, timeout_name, command) in fixed {
        let transport = Arc::clone(transport);
        let rt = rt.clone();
        let command = (*command).to_string();
        let default_transport = Arc::clone(&transport);
        let default_rt = rt.clone();
        let default_command = command.clone();
        let deadline_default = Arc::clone(deadline);
        engine.register_fn(default_name, move || -> Result<SteelVal, SteelErr> {
            let timeout = clamp_to_eval_budget(default_timeout, &deadline_default)?;
            submit_and_await_stop(&default_transport, &default_rt, &default_command, timeout)
        });

        let deadline_timeout = Arc::clone(deadline);
        engine.register_fn(
            timeout_name,
            move |seconds: isize| -> Result<SteelVal, SteelErr> {
                let timeout = timeout_from_seconds(seconds)?;
                check_eval_budget(timeout, &deadline_timeout)?;
                submit_and_await_stop(&transport, &rt, &command, timeout)
            },
        );
    }

    // `until-and-wait` takes a location parameter and quotes it.
    let transport_default = Arc::clone(transport);
    let rt_default = rt.clone();
    let deadline_default = Arc::clone(deadline);
    engine.register_fn(
        "until-and-wait/default",
        move |loc: String| -> Result<SteelVal, SteelErr> {
            let raw = format!("-exec-until {}", mi_quote_param(&loc));
            let timeout = clamp_to_eval_budget(default_timeout, &deadline_default)?;
            submit_and_await_stop(&transport_default, &rt_default, &raw, timeout)
        },
    );
    let transport = Arc::clone(transport);
    let rt = rt.clone();
    let deadline_timeout = Arc::clone(deadline);
    engine.register_fn(
        "until-and-wait/timeout",
        move |loc: String, seconds: isize| -> Result<SteelVal, SteelErr> {
            let raw = format!("-exec-until {}", mi_quote_param(&loc));
            let timeout = timeout_from_seconds(seconds)?;
            check_eval_budget(timeout, &deadline_timeout)?;
            submit_and_await_stop(&transport, &rt, &raw, timeout)
        },
    );
}

/// Shared implementation for every `*-and-wait` primitive.
///
/// Capture cursor first, submit second, wait third — that ordering is
/// load-bearing. If you reorder to submit-then-wait you reintroduce the
/// exact race this helper exists to eliminate.
fn submit_and_await_stop(
    transport: &Arc<TransportHandle>,
    rt: &tokio::runtime::Handle,
    raw: &str,
    timeout: Duration,
) -> Result<SteelVal, SteelErr> {
    let after_seq = transport.event_cursor();

    rt.block_on(async move {
        let outcome = transport
            .submit_raw(raw)
            .await
            .map_err(|err| SteelErr::new(ErrorKind::Generic, format!("transport error: {err}")))?;

        // If GDB didn't actually put the target into the running
        // state, there will be no `*stopped` to wait for.  Return the
        // outcome as-is — `marshal::outcome_to_steel` already raises a
        // SteelErr for `Error`/`Exit` variants.
        if !matches!(outcome, CommandOutcome::Running) {
            return marshal::outcome_to_steel(&outcome);
        }

        match transport.next_stop_after(after_seq, timeout).await {
            Ok(Some((_, ev))) => stopped_event_to_steel(&ev),
            Ok(None) => Err(timeout_error_with_warnings(
                "trigger-and-wait",
                transport,
                after_seq,
                timeout,
            )),
            Err(err) => Err(SteelErr::new(
                ErrorKind::Generic,
                format!("transport error: {err}"),
            )),
        }
    })
}

// ---------------------------------------------------------------------------
// (drain-events) / (drain-events after-seq) → list of hash-maps
// ---------------------------------------------------------------------------

/// Register the `drain-events` primitives.
///
/// `(drain-events)` returns all retained events.
/// `(drain-events after-seq)` returns events strictly after `after-seq`.
///
/// Each event is a hash-map with keys `"seq"`, `"kind"`, and optional
/// `"text"`, `"class"`, `"thread"`, `"reason"`.
fn register_drain_events(engine: &mut Engine, transport: Arc<TransportHandle>) {
    let transport_all = Arc::clone(&transport);
    engine.register_fn("drain-events/all", move || -> SteelVal {
        drain_events_impl(&transport_all, 0)
    });

    engine.register_fn(
        "drain-events/after",
        move |after_seq: isize| -> Result<SteelVal, SteelErr> {
            let seq = u64::try_from(after_seq).map_err(|_| {
                SteelErr::new(
                    ErrorKind::Generic,
                    format!("drain-events: after-seq must be non-negative, got {after_seq}"),
                )
            })?;
            Ok(drain_events_impl(&transport, seq))
        },
    );
}

fn drain_events_impl(transport: &Arc<TransportHandle>, after_seq: u64) -> SteelVal {
    let payload = server_helpers::drain_observed_events(transport, after_seq);
    let events: Vec<SteelVal> = payload.events.iter().map(observed_event_to_steel).collect();
    SteelVal::ListV(events.into())
}

fn observed_event_to_steel(ev: &server_helpers::ObservedEvent) -> SteelVal {
    let mut map = HashMap::new();
    map.insert(
        SteelVal::StringV(SteelString::from("seq")),
        SteelVal::IntV(isize::try_from(ev.seq).unwrap_or(isize::MAX)),
    );
    map.insert(
        SteelVal::StringV(SteelString::from("kind")),
        SteelVal::StringV(SteelString::from(ev.kind)),
    );
    if let Some(text) = &ev.text {
        map.insert(
            SteelVal::StringV(SteelString::from("text")),
            SteelVal::StringV(SteelString::from(text.as_str())),
        );
    }
    if let Some(class) = &ev.class {
        map.insert(
            SteelVal::StringV(SteelString::from("class")),
            SteelVal::StringV(SteelString::from(class.as_str())),
        );
    }
    if let Some(thread) = &ev.thread {
        map.insert(
            SteelVal::StringV(SteelString::from("thread")),
            SteelVal::StringV(SteelString::from(thread.as_str())),
        );
    }
    if let Some(reason) = &ev.reason {
        map.insert(
            SteelVal::StringV(SteelString::from("reason")),
            SteelVal::StringV(SteelString::from(reason.as_str())),
        );
    }
    SteelVal::HashMapV(SteelHashMap::from(Gc::new(map)))
}

/// Build a rich timeout error that includes transport context and any
/// recent GDB warnings emitted since `after_seq`.
fn timeout_error_with_warnings(
    label: &str,
    transport: &Arc<TransportHandle>,
    after_seq: u64,
    timeout: Duration,
) -> SteelErr {
    let mut msg = format!(
        "{label} timed out after {}s ({})",
        timeout.as_secs(),
        timeout_context_message(transport)
    );
    let warnings = collect_recent_warnings(transport, after_seq);
    if !warnings.is_empty() {
        msg.push_str("\nRecent GDB warnings:");
        for w in &warnings {
            msg.push_str("\n  ");
            msg.push_str(w);
        }
    }
    SteelErr::new(ErrorKind::Generic, msg)
}

/// Collect recent GDB `&"warning: ..."` log events from the journal
/// since `after_seq`, suitable for enriching timeout error messages.
fn collect_recent_warnings(transport: &Arc<TransportHandle>, after_seq: u64) -> Vec<String> {
    transport
        .events_after(after_seq)
        .into_iter()
        .filter_map(|(_, event)| {
            if let framewalk_mi_protocol::Event::Log(text) = &event {
                let trimmed = text.trim();
                if trimmed.starts_with("warning:") || trimmed.starts_with("Warning:") {
                    return Some(trimmed.to_string());
                }
            }
            None
        })
        .take(5)
        .collect()
}

fn timeout_from_seconds(seconds: isize) -> Result<Duration, SteelErr> {
    let secs = u64::try_from(seconds).map_err(|_| {
        SteelErr::new(
            ErrorKind::Generic,
            format!("timeout must be a positive integer, got {seconds}"),
        )
    })?;
    if secs == 0 {
        return Err(SteelErr::new(
            ErrorKind::Generic,
            format!("timeout must be a positive integer, got {seconds}"),
        ));
    }
    Ok(Duration::from_secs(secs))
}

/// Fail fast when an explicit wait timeout exceeds the remaining
/// `scheme_eval` budget.  Called for user-supplied timeout overrides
/// (e.g. `(cont-and-wait 180)`).
fn check_eval_budget(requested: Duration, deadline: &EvalDeadline) -> Result<(), SteelErr> {
    let guard = deadline.lock().map_err(|_| {
        SteelErr::new(
            ErrorKind::Generic,
            "eval deadline mutex poisoned".to_string(),
        )
    })?;
    if let Some(dl) = *guard {
        let remaining = dl.saturating_duration_since(Instant::now());
        if requested > remaining {
            return Err(SteelErr::new(
                ErrorKind::Generic,
                format!(
                    "wait timeout of {}s exceeds remaining scheme_eval budget of {}s; \
                     increase --scheme-eval-timeout-secs or reduce the wait",
                    requested.as_secs(),
                    remaining.as_secs()
                ),
            ));
        }
    }
    Ok(())
}

/// For default (non-user-supplied) timeouts, silently clamp to the
/// remaining eval budget rather than erroring.  This avoids surprising
/// failures when the default wait timeout happens to be larger than
/// the remaining budget — the user didn't ask for it, so an error
/// would be confusing.
fn clamp_to_eval_budget(
    default_timeout: Duration,
    deadline: &EvalDeadline,
) -> Result<Duration, SteelErr> {
    let guard = deadline.lock().map_err(|_| {
        SteelErr::new(
            ErrorKind::Generic,
            "eval deadline mutex poisoned".to_string(),
        )
    })?;
    if let Some(dl) = *guard {
        let remaining = dl.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(SteelErr::new(
                ErrorKind::Generic,
                "scheme_eval budget exhausted; increase --scheme-eval-timeout-secs".to_string(),
            ));
        }
        Ok(default_timeout.min(remaining))
    } else {
        Ok(default_timeout)
    }
}

/// Convert a [`StoppedEvent`] into a Steel hash-map with string keys.
fn stopped_event_to_steel(
    ev: &framewalk_mi_protocol::event::StoppedEvent,
) -> Result<SteelVal, SteelErr> {
    let mut map = HashMap::new();

    if let Some(reason) = &ev.reason {
        map.insert(
            SteelVal::StringV(SteelString::from("reason")),
            SteelVal::StringV(SteelString::from(
                server_helpers::format_stopped_reason(reason).as_str(),
            )),
        );
    }

    if let Some(ref thread) = ev.thread {
        // ThreadId wraps a String (e.g. "1", "all"); pass it through
        // as a Scheme string so the caller handles all cases.
        map.insert(
            SteelVal::StringV(SteelString::from("thread")),
            SteelVal::StringV(SteelString::from(thread.as_str())),
        );
    }

    // Include the raw MI results so Scheme code can access any field
    // GDB reports, not just the ones we parse into typed form.
    if !ev.raw.is_empty() {
        let raw = marshal::outcome_to_steel(&framewalk_mi_protocol::CommandOutcome::Done(
            ev.raw.clone(),
        ))?;
        map.insert(SteelVal::StringV(SteelString::from("raw")), raw);
    }

    Ok(SteelVal::HashMapV(SteelHashMap::from(Gc::new(map))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bare_word_unchanged() {
        assert_eq!(mi_quote_param("main"), "main");
        assert_eq!(mi_quote_param("hello.c:42"), "hello.c:42");
        assert_eq!(mi_quote_param("*0x400500"), "*0x400500");
    }

    #[test]
    fn space_triggers_quoting() {
        assert_eq!(mi_quote_param("hello world"), "\"hello world\"");
        assert_eq!(
            mi_quote_param("/path/with spaces/file"),
            "\"/path/with spaces/file\""
        );
    }

    #[test]
    fn embedded_quote_is_escaped() {
        assert_eq!(mi_quote_param("x + \"y\""), "\"x + \\\"y\\\"\"");
    }

    #[test]
    fn backslash_is_escaped() {
        assert_eq!(mi_quote_param("a\\b"), "\"a\\\\b\"");
    }

    #[test]
    fn empty_string_is_quoted() {
        assert_eq!(mi_quote_param(""), "\"\"");
    }

    #[test]
    fn control_chars_are_hex_escaped() {
        assert_eq!(mi_quote_param("\x00"), "\"\\x00\"");
        assert_eq!(mi_quote_param("\n"), "\"\\n\"");
        assert_eq!(mi_quote_param("\t"), "\"\\t\"");
    }

    // -----------------------------------------------------------------
    // Prelude test harness
    // -----------------------------------------------------------------
    //
    // Goal: lock in the behaviour of every function and macro exported
    // from `prelude.scm` so a Steel upgrade, a rename, or an accidental
    // deletion surfaces as a failing test rather than a silent runtime
    // regression.  Stub every Rust primitive the prelude depends on
    // with a known-return-value shim, load the real prelude via
    // `worker::load_prelude`, and assert against the captured call or
    // the evaluated Scheme value.  No GDB subprocess involved.

    use std::sync::{Arc, Mutex};
    use steel::rvals::SteelVal;
    use steel::steel_vm::engine::Engine;

    /// Fresh engine with capturing stubs and the real prelude loaded.
    struct PreludeHarness {
        engine: Engine,
        mi_calls: Arc<Mutex<Vec<String>>>,
    }

    impl PreludeHarness {
        fn new() -> Self {
            let mut engine = Engine::new_sandboxed();
            crate::scheme::worker::seal_sandbox(&mut engine).expect("seal must succeed");

            // `mi` captures the wire string and returns an empty list
            // so downstream `result-fields`/`result-field` calls don't
            // misinterpret the return value.
            let mi_calls: Arc<Mutex<Vec<String>>> = Arc::default();
            let cap = Arc::clone(&mi_calls);
            engine.register_fn("mi", move |cmd: String| -> Vec<String> {
                cap.lock().expect("mi_calls mutex poisoned").push(cmd);
                Vec::new()
            });
            engine.register_fn("gdb-version", || -> Vec<String> { Vec::new() });
            engine.register_fn("mi-quote", |s: String| -> String { s });

            // Wait-family and drain-events stubs return distinct
            // marker strings so dispatch tests can verify which arm
            // of each `syntax-rules` macro was selected.
            engine.register_fn("wait-for-stop/default", || -> String {
                "wait-default".to_string()
            });
            engine.register_fn("wait-for-stop/timeout", |n: isize| -> String {
                format!("wait-timeout:{n}")
            });
            engine.register_fn("run-and-wait/default", || -> String {
                "run-default".to_string()
            });
            engine.register_fn("run-and-wait/timeout", |n: isize| -> String {
                format!("run-timeout:{n}")
            });
            engine.register_fn("cont-and-wait/default", || -> String {
                "cont-default".to_string()
            });
            engine.register_fn("cont-and-wait/timeout", |n: isize| -> String {
                format!("cont-timeout:{n}")
            });
            engine.register_fn("step-and-wait/default", || -> String {
                "step-default".to_string()
            });
            engine.register_fn("step-and-wait/timeout", |n: isize| -> String {
                format!("step-timeout:{n}")
            });
            engine.register_fn("next-and-wait/default", || -> String {
                "next-default".to_string()
            });
            engine.register_fn("next-and-wait/timeout", |n: isize| -> String {
                format!("next-timeout:{n}")
            });
            engine.register_fn("finish-and-wait/default", || -> String {
                "finish-default".to_string()
            });
            engine.register_fn("finish-and-wait/timeout", |n: isize| -> String {
                format!("finish-timeout:{n}")
            });
            engine.register_fn("until-and-wait/default", |loc: String| -> String {
                format!("until-default:{loc}")
            });
            engine.register_fn(
                "until-and-wait/timeout",
                |loc: String, n: isize| -> String { format!("until-timeout:{loc}:{n}") },
            );
            engine.register_fn("drain-events/all", || -> String { "drain-all".to_string() });
            engine.register_fn("drain-events/after", |n: isize| -> String {
                format!("drain-after:{n}")
            });

            crate::scheme::worker::load_prelude(&mut engine).expect("prelude must load cleanly");

            Self { engine, mi_calls }
        }

        fn clear_mi_calls(&self) {
            self.mi_calls
                .lock()
                .expect("mi_calls mutex poisoned")
                .clear();
        }

        fn mi_calls_snapshot(&self) -> Vec<String> {
            self.mi_calls
                .lock()
                .expect("mi_calls mutex poisoned")
                .clone()
        }

        fn last_mi_call(&self) -> Option<String> {
            self.mi_calls
                .lock()
                .expect("mi_calls mutex poisoned")
                .last()
                .cloned()
        }

        fn run_ok(&mut self, code: &str) -> Vec<SteelVal> {
            self.engine
                .run(code.to_string())
                .unwrap_or_else(|err| panic!("scheme eval failed for {code:?}: {err}"))
        }

        /// Evaluate `code` and return the last top-level value as a
        /// string.  Panics if the last value is not a string.  Used
        /// for macro-dispatch tests where each stubbed variant returns
        /// a distinct marker string.
        fn eval_string(&mut self, code: &str) -> String {
            let values = self.run_ok(code);
            let last = values
                .last()
                .unwrap_or_else(|| panic!("no result for {code:?}"));
            match last {
                SteelVal::StringV(s) => s.as_str().to_string(),
                other => panic!("expected string for {code:?}, got {other:?}"),
            }
        }

        /// Evaluate `code`, which must be an `(equal? ...)` or similar
        /// boolean expression, and assert the result is `#t`.  Used
        /// for pure-Scheme tests of accessors and `compact`.
        fn assert_true(&mut self, code: &str) {
            let values = self.run_ok(code);
            let last = values
                .last()
                .unwrap_or_else(|| panic!("no result for {code:?}"));
            match last {
                SteelVal::BoolV(true) => {}
                other => panic!("expected #t for {code:?}, got {other:?}"),
            }
        }
    }

    // -----------------------------------------------------------------
    // Thin MI command wrappers: each emits a specific wire form
    // -----------------------------------------------------------------

    #[test]
    fn prelude_command_builders_emit_correct_mi_wire_form() {
        // Every function whose job is to forward to a specific MI
        // command belongs in this table.  If you add a wrapper to
        // `prelude.scm`, add a row here.
        let cases: &[(&str, &str)] = &[
            // Session
            (
                r#"(load-file "/bin/true")"#,
                "-file-exec-and-symbols /bin/true",
            ),
            ("(attach 42)", "-target-attach 42"),
            (r#"(attach "42")"#, "-target-attach 42"),
            ("(detach)", "-target-detach"),
            // Execution
            ("(run)", "-exec-run"),
            ("(cont)", "-exec-continue"),
            ("(step)", "-exec-step"),
            ("(next)", "-exec-next"),
            ("(finish)", "-exec-finish"),
            ("(interrupt)", "-exec-interrupt --all"),
            (r#"(until "main")"#, "-exec-until main"),
            ("(step-instruction)", "-exec-step-instruction"),
            ("(next-instruction)", "-exec-next-instruction"),
            ("(reverse-step)", "-exec-step --reverse"),
            ("(reverse-next)", "-exec-next --reverse"),
            ("(reverse-continue)", "-exec-continue --reverse"),
            ("(reverse-finish)", "-exec-finish --reverse"),
            // Breakpoints
            (r#"(set-breakpoint "main")"#, "-break-insert main"),
            (r#"(set-temp-breakpoint "main")"#, "-break-insert -t main"),
            (r#"(set-hw-breakpoint "main")"#, "-break-insert -h main"),
            (
                r#"(set-temp-hw-breakpoint "main")"#,
                "-break-insert -t -h main",
            ),
            ("(delete-breakpoint 1)", "-break-delete 1"),
            (r#"(delete-breakpoint "1")"#, "-break-delete 1"),
            ("(enable-breakpoint 1)", "-break-enable 1"),
            (r#"(enable-breakpoint "1")"#, "-break-enable 1"),
            ("(disable-breakpoint 1)", "-break-disable 1"),
            (r#"(disable-breakpoint "1")"#, "-break-disable 1"),
            ("(list-breakpoints)", "-break-list"),
            // Stack inspection
            ("(list-locals)", "-stack-list-locals 1"),
            ("(list-arguments)", "-stack-list-arguments 1"),
            ("(stack-depth)", "-stack-info-depth"),
            ("(select-frame 2)", "-stack-select-frame 2"),
            (r#"(select-frame "2")"#, "-stack-select-frame 2"),
            // Threads
            ("(list-threads)", "-thread-info"),
            ("(select-thread 3)", "-thread-select 3"),
            (r#"(select-thread "3")"#, "-thread-select 3"),
            // Variables
            (r#"(inspect "x")"#, "-data-evaluate-expression x"),
        ];

        let mut h = PreludeHarness::new();
        for (code, expected) in cases {
            h.clear_mi_calls();
            h.run_ok(code);
            let last = h
                .last_mi_call()
                .unwrap_or_else(|| panic!("no mi call for {code:?}"));
            assert_eq!(last.as_str(), *expected, "code = {code}");
        }
    }

    // -----------------------------------------------------------------
    // Wait-family macros: verify all three arms of each syntax-rules
    // -----------------------------------------------------------------

    #[test]
    fn prelude_wait_family_macros_dispatch_correctly() {
        let cases: &[(&str, &str)] = &[
            ("(wait-for-stop)", "wait-default"),
            ("(wait-for-stop 5)", "wait-timeout:5"),
            ("(wait-for-stop timeout: 5)", "wait-timeout:5"),
            ("(run-and-wait)", "run-default"),
            ("(run-and-wait 5)", "run-timeout:5"),
            ("(run-and-wait timeout: 5)", "run-timeout:5"),
            ("(cont-and-wait)", "cont-default"),
            ("(cont-and-wait 5)", "cont-timeout:5"),
            ("(cont-and-wait timeout: 5)", "cont-timeout:5"),
            ("(step-and-wait)", "step-default"),
            ("(step-and-wait 5)", "step-timeout:5"),
            ("(step-and-wait timeout: 5)", "step-timeout:5"),
            ("(next-and-wait)", "next-default"),
            ("(next-and-wait 5)", "next-timeout:5"),
            ("(next-and-wait timeout: 5)", "next-timeout:5"),
            ("(finish-and-wait)", "finish-default"),
            ("(finish-and-wait 5)", "finish-timeout:5"),
            ("(finish-and-wait timeout: 5)", "finish-timeout:5"),
            (r#"(until-and-wait "main")"#, "until-default:main"),
            (r#"(until-and-wait "main" 5)"#, "until-timeout:main:5"),
            (
                r#"(until-and-wait "main" timeout: 5)"#,
                "until-timeout:main:5",
            ),
        ];
        let mut h = PreludeHarness::new();
        for (code, expected) in cases {
            assert_eq!(h.eval_string(code), *expected, "code = {code}");
        }
    }

    #[test]
    fn prelude_drain_events_macro_dispatches_all_arms() {
        let cases: &[(&str, &str)] = &[
            ("(drain-events)", "drain-all"),
            ("(drain-events 7)", "drain-after:7"),
            ("(drain-events after: 7)", "drain-after:7"),
        ];
        let mut h = PreludeHarness::new();
        for (code, expected) in cases {
            assert_eq!(h.eval_string(code), *expected, "code = {code}");
        }
    }

    // -----------------------------------------------------------------
    // backtrace macro: (backtrace), (backtrace N), (backtrace limit: N)
    // -----------------------------------------------------------------

    #[test]
    fn prelude_backtrace_no_args_emits_bare_command() {
        let mut h = PreludeHarness::new();
        h.run_ok("(backtrace)");
        let calls = h.mi_calls_snapshot();
        assert!(
            calls.iter().any(|c| c == "-stack-list-frames"),
            "expected bare -stack-list-frames, got {calls:?}"
        );
    }

    #[test]
    fn prelude_backtrace_with_limit_emits_low_high_range() {
        // Both positional `(backtrace N)` and `(backtrace limit: N)`
        // expand to `-stack-list-frames 0 N-1` (inclusive high).
        for form in ["(backtrace 5)", "(backtrace limit: 5)"] {
            let mut h = PreludeHarness::new();
            h.run_ok(form);
            let calls = h.mi_calls_snapshot();
            assert!(
                calls.iter().any(|c| c == "-stack-list-frames 0 4"),
                "{form}: expected -stack-list-frames 0 4, got {calls:?}"
            );
        }
    }

    // -----------------------------------------------------------------
    // Result accessors
    // -----------------------------------------------------------------

    #[test]
    fn prelude_result_fields_extracts_all_matches_in_order() {
        let mut h = PreludeHarness::new();
        h.assert_true(
            r#"
            (equal?
              (result-fields "frame"
                (list (hash "name" "frame" "value" "a")
                      (hash "name" "other" "value" "skip")
                      (hash "name" "frame" "value" "b")))
              (list "a" "b"))
            "#,
        );
    }

    #[test]
    fn prelude_result_fields_returns_empty_for_missing_and_falsy() {
        let mut h = PreludeHarness::new();
        h.assert_true(r#"(equal? (result-fields "missing" (list)) (list))"#);
        h.assert_true(r#"(equal? (result-fields "missing" #f) (list))"#);
    }

    #[test]
    fn prelude_result_field_returns_unique_value() {
        let mut h = PreludeHarness::new();
        h.assert_true(
            r#"
            (equal?
              (result-field "x" (list (hash "name" "x" "value" "one")))
              "one")
            "#,
        );
    }

    #[test]
    fn prelude_result_field_returns_false_when_absent() {
        let mut h = PreludeHarness::new();
        h.assert_true(r#"(not (result-field "missing" (list)))"#);
    }

    #[test]
    fn prelude_result_field_raises_on_duplicate_names() {
        let mut h = PreludeHarness::new();
        let err = h
            .engine
            .run(
                r#"
                (result-field "frame"
                  (list (hash "name" "frame" "value" "a")
                        (hash "name" "frame" "value" "b")))
                "#
                .to_string(),
            )
            .expect_err("result-field must raise on duplicate names");
        assert!(
            err.to_string().contains("result-field"),
            "expected result-field error, got {err}"
        );
    }

    #[test]
    fn prelude_id_string_helper_rejects_non_string_non_number() {
        let mut h = PreludeHarness::new();
        let err = h
            .engine
            .run("(delete-breakpoint (list 1 2))".to_string())
            .expect_err("delete-breakpoint should reject composite ids");
        assert!(
            err.to_string()
                .contains("delete-breakpoint expects an id as a string or number"),
            "expected friendly id coercion error, got {err}"
        );
    }

    #[test]
    fn stopped_event_reason_uses_mi_style_token() {
        let event = framewalk_mi_protocol::event::StoppedEvent {
            reason: Some(framewalk_mi_protocol::StoppedReason::BreakpointHit {
                bkptno: Some("1".to_string()),
            }),
            thread: None,
            frame: None,
            raw: Vec::new(),
        };

        let steel = stopped_event_to_steel(&event).expect("stop conversion should succeed");
        let SteelVal::HashMapV(map) = steel else {
            panic!("expected stop hash map");
        };
        let reason = map
            .get(&SteelVal::StringV(SteelString::from("reason")))
            .unwrap_or_else(|| panic!("missing reason field"));
        assert_eq!(
            reason,
            &SteelVal::StringV(SteelString::from("breakpoint-hit"))
        );
    }

    // -----------------------------------------------------------------
    // compact
    // -----------------------------------------------------------------

    #[test]
    fn prelude_compact_collapses_unique_entries_to_flat_hash() {
        let mut h = PreludeHarness::new();
        h.assert_true(
            r#"
            (equal?
              (compact (list (hash "name" "a" "value" "1")
                             (hash "name" "b" "value" "2")))
              (hash "a" "1" "b" "2"))
            "#,
        );
    }

    #[test]
    fn prelude_compact_preserves_duplicate_keys_as_entry_list() {
        let mut h = PreludeHarness::new();
        h.assert_true(
            r#"
            (equal?
              (compact (list (hash "name" "frame" "value" "a")
                             (hash "name" "frame" "value" "b")))
              (list (hash "name" "frame" "value" "a")
                    (hash "name" "frame" "value" "b")))
            "#,
        );
    }

    #[test]
    fn prelude_compact_recurses_into_nested_entry_values() {
        let mut h = PreludeHarness::new();
        h.assert_true(
            r#"
            (equal?
              (compact
                (list (hash "name" "outer"
                            "value" (list (hash "name" "inner" "value" "v")))))
              (hash "outer" (hash "inner" "v")))
            "#,
        );
    }

    #[test]
    fn prelude_compact_passes_non_list_values_through() {
        let mut h = PreludeHarness::new();
        h.assert_true(r#"(equal? (compact "abc") "abc")"#);
        h.assert_true(r"(equal? (compact 42) 42)");
        h.assert_true(r"(not (compact #f))");
    }

    #[test]
    fn prelude_compact_empty_list_becomes_empty_hash() {
        let mut h = PreludeHarness::new();
        h.assert_true(r"(equal? (compact (list)) (hash))");
    }

    #[test]
    fn prelude_compact_leaves_non_entry_lists_element_wise_compacted() {
        // A list of strings isn't an entry list; compact should map
        // through (each element unchanged since strings pass through).
        let mut h = PreludeHarness::new();
        h.assert_true(r#"(equal? (compact (list "a" "b" "c")) (list "a" "b" "c"))"#);
    }

    // -----------------------------------------------------------------
    // Composition helpers: step-n, next-n, run-to
    // -----------------------------------------------------------------

    #[test]
    fn prelude_step_n_collects_step_and_wait_results() {
        let mut h = PreludeHarness::new();
        h.assert_true(r#"(equal? (step-n 3) (list "step-default" "step-default" "step-default"))"#);
    }

    #[test]
    fn prelude_next_n_collects_next_and_wait_results() {
        let mut h = PreludeHarness::new();
        h.assert_true(r#"(equal? (next-n 2) (list "next-default" "next-default"))"#);
    }

    #[test]
    fn prelude_step_n_zero_returns_empty_list() {
        let mut h = PreludeHarness::new();
        h.assert_true(r"(equal? (step-n 0) (list))");
    }

    #[test]
    fn prelude_run_to_sets_temp_breakpoint_then_runs_and_waits() {
        let mut h = PreludeHarness::new();
        let result = h.eval_string(r#"(run-to "main")"#);
        assert_eq!(result, "run-default");
        let calls = h.mi_calls_snapshot();
        assert!(
            calls.iter().any(|c| c == "-break-insert -t main"),
            "expected -break-insert -t main, got {calls:?}"
        );
    }

    // -----------------------------------------------------------------
    // Bootstrap smoke — narrowed to its original purpose.
    // -----------------------------------------------------------------
    //
    // Every command builder is now exercised by
    // `prelude_command_builders_emit_correct_mi_wire_form`, so this
    // test is kept solely to catch the Steel variadic-lambda
    // `FreeIdentifier: ##params2` regression.  A define-only load
    // succeeds even when that bug is present; only a call-time
    // evaluation of a zero-arg `mi-cmd` expansion surfaces it.
    #[test]
    fn prelude_bootstrap_smoke() {
        let mut h = PreludeHarness::new();
        for code in [
            "(gdb-version)",
            r#"(load-file "/bin/true")"#,
            r#"(run-to "main")"#,
            "(wait-for-stop)",
            "(wait-for-stop 5)",
            "(wait-for-stop timeout: 5)",
            "(drain-events)",
            "(drain-events 0)",
            "(drain-events after: 0)",
            "(backtrace)",
            "(backtrace 5)",
            "(backtrace limit: 5)",
        ] {
            h.run_ok(code);
        }
    }
}
