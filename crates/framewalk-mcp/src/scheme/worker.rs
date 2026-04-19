//! Scheme worker thread and its async-compatible handle.
//!
//! [`SchemeHandle`] is the async-side façade: it accepts Scheme source
//! strings and returns evaluation results as plain strings.
//! Internally a dedicated OS thread owns the Steel [`Engine`],
//! communicating via a `tokio::sync::mpsc` channel (worker ← caller)
//! and per-request `oneshot` channels (worker → caller).
//!
//! The dedicated thread exists because Steel's `Engine` is
//! `!Send + !Sync`.  It uses `tokio::runtime::Handle::block_on` to
//! call into the async transport when Scheme code invokes `(mi …)` or
//! `(wait-for-stop)`.

use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use framewalk_mi_transport::TransportHandle;
use steel::rerrs::SteelErr;
use steel::steel_vm::engine::Engine;
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info};

use crate::scheme::bindings::EvalDeadline;
use crate::scheme::marshal;
use crate::scheme::{bindings, SchemeSettings};

/// Capacity of the eval request channel. Provides backpressure if
/// requests arrive faster than the single-threaded Scheme engine can
/// process them, preventing unbounded memory growth.
const EVAL_CHANNEL_CAPACITY: usize = 32;

// ---------------------------------------------------------------------------
// Initialisation errors
// ---------------------------------------------------------------------------

/// Errors that can occur while spawning the Scheme worker thread.
///
/// All variants are fatal: if any of these fire, the worker thread is
/// not running and the caller should abort startup. Carrying the
/// underlying message as a `String` keeps the error `Send` across the
/// worker-thread → async-caller boundary.
#[derive(Debug, thiserror::Error)]
pub enum SchemeInitError {
    /// The prelude failed to evaluate cleanly. This almost always
    /// means a Scheme-level bug in `prelude.scm` or an incompatibility
    /// with the version of Steel we're pinned to — it should never be
    /// a runtime condition.
    #[error("failed to load scheme prelude: {0}")]
    PreludeLoad(String),

    /// The worker thread panicked (or exited) before signalling ready.
    /// The thread's panic hook may carry more detail in the tracing
    /// log; this variant just tells the caller to fail the boot.
    #[error("scheme worker thread exited before initialisation completed")]
    WorkerExitedDuringInit,
}

// ---------------------------------------------------------------------------
// Channel types
// ---------------------------------------------------------------------------

/// A request to evaluate Scheme code.
struct SchemeRequest {
    code: String,
    /// Per-call override of the default `--scheme-eval-timeout-secs`.
    /// `None` → fall back to `SchemeSettings::eval_timeout`. Both the
    /// outer tokio timeout (caller side) and the in-worker deadline
    /// cell are armed from the same effective value so wait primitives
    /// and the harness agree on the budget.
    budget: Option<Duration>,
    /// When `true`, the worker captures an event-journal cursor before
    /// running the code and, on return, filters stream-class events
    /// produced during the call into the reply so callers don't need a
    /// separate `drain-events` round-trip.
    capture_streams: bool,
    reply: oneshot::Sender<Result<SchemeEvalReply, String>>,
}

/// Per-call cap on inlined stream events so a chatty `(cont)` over a
/// noisy console doesn't balloon a single MCP response.
const MAX_INLINED_STREAM_EVENTS: usize = 100;

/// Reply payload from a scheme evaluation.  `display` is the classic
/// human-readable rendering of the top-level values; `streams`
/// carries any stream-class events (`console`, `target-output`,
/// `log`) observed during the call when `capture_streams` was set.
#[derive(Debug, Default)]
pub struct SchemeEvalReply {
    pub display: String,
    pub result: serde_json::Value,
    pub streams: Vec<crate::server_helpers::ObservedEvent>,
    pub truncated_streams: bool,
    pub truncated_journal: bool,
}

// ---------------------------------------------------------------------------
// SchemeHandle (async side)
// ---------------------------------------------------------------------------

/// Async-side handle to the scheme worker thread.
///
/// The handle is **not** `Clone`: it carries the worker thread's
/// `JoinHandle` so that shutdown can deterministically wait for the
/// thread to finish. Callers that need shared access should wrap it
/// in `Arc<SchemeHandle>`.
pub struct SchemeHandle {
    tx: mpsc::Sender<SchemeRequest>,
    eval_timeout: Duration,

    /// Retained so [`SchemeHandle::join`] can wait for the worker
    /// thread to finish after the eval channel has closed.  `Option`
    /// only so `join` can take it out of `&mut self`.
    join_handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for SchemeHandle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SchemeHandle")
            .field("open", &!self.tx.is_closed())
            .finish_non_exhaustive()
    }
}

impl SchemeHandle {
    /// Spawn the scheme worker thread and return a handle to it.
    ///
    /// The worker thread is constructed in two phases:
    ///
    /// 1. **Init**: build a fresh Steel `Engine`, seal the dangerous
    ///    builtin modules, and evaluate the embedded prelude. If any
    ///    of these steps fails, the thread sends the error back over
    ///    a one-shot and exits; this method returns
    ///    [`SchemeInitError`] and the caller must abort startup.
    /// 2. **Serve**: enter `worker_loop` and process eval requests
    ///    until the request channel closes.
    ///
    /// The two-phase structure means a broken prelude is a hard
    /// startup error instead of a silent warning — the previous
    /// behaviour booted the engine into a state where every
    /// prelude-defined helper raised `FreeIdentifier` at call time.
    pub async fn spawn(
        transport: Arc<TransportHandle>,
        allow_shell: bool,
        rt: tokio::runtime::Handle,
        settings: SchemeSettings,
    ) -> Result<Self, SchemeInitError> {
        let (tx, rx) = mpsc::channel(EVAL_CHANNEL_CAPACITY);
        let (ready_tx, ready_rx) = oneshot::channel::<Result<(), SchemeInitError>>();
        let eval_timeout = settings.eval_timeout;

        let join_handle = std::thread::Builder::new()
            .name("framewalk-scheme".to_string())
            .spawn(move || {
                // The eval deadline is created once and shared across
                // engine rebuilds so registered closures always point
                // at the same Arc.
                let deadline: EvalDeadline = Arc::new(std::sync::Mutex::new(None));

                // Phase 1: build engine + load prelude.
                let engine = match build_engine(
                    Arc::clone(&transport),
                    allow_shell,
                    &rt,
                    settings,
                    &deadline,
                ) {
                    Ok(engine) => {
                        // `.ok()` discards the send result on purpose:
                        // if the caller dropped `ready_rx` we're about
                        // to enter the serve loop anyway.
                        ready_tx.send(Ok(())).ok();
                        engine
                    }
                    Err(err) => {
                        let msg = err.to_string();
                        error!(%msg, "scheme engine initialisation failed");
                        ready_tx.send(Err(SchemeInitError::PreludeLoad(msg))).ok();
                        return;
                    }
                };

                // Phase 2: serve eval requests until the channel closes.
                worker_loop(
                    engine,
                    &deadline,
                    rx,
                    &transport,
                    allow_shell,
                    &rt,
                    settings,
                );
            })
            .expect("failed to spawn scheme worker thread");

        let init_result = match ready_rx.await {
            Ok(result) => result,
            Err(_recv_err) => {
                // `ready_tx` dropped without a send — the worker
                // thread panicked before (or during) the init step.
                if let Err(panic) = join_handle.join() {
                    error!(
                        panic = %panic_message(&panic),
                        "scheme worker thread panicked during initialisation"
                    );
                }
                return Err(SchemeInitError::WorkerExitedDuringInit);
            }
        };

        match init_result {
            Ok(()) => {
                info!("scheme worker thread started");
                Ok(Self {
                    tx,
                    eval_timeout,
                    join_handle: Some(join_handle),
                })
            }
            Err(init_err) => {
                // Worker signalled an init error and exited. Join it
                // so the thread record doesn't leak; log any panic
                // payload so we don't lose it silently.
                if let Err(panic) = join_handle.join() {
                    error!(
                        panic = %panic_message(&panic),
                        "scheme worker thread panicked after signalling init error"
                    );
                }
                Err(init_err)
            }
        }
    }

    /// Evaluate Scheme `code` and return the display string of the
    /// result.
    ///
    /// Returns `Err` with a human-readable message on timeout, channel
    /// failure, or Scheme evaluation error.
    ///
    /// # Ordering guarantee
    ///
    /// The binary runs on a `current_thread` tokio runtime (see the
    /// `#[tokio::main(flavor = "current_thread")]` annotation in
    /// `main.rs`), which polls spawned tasks in FIFO order from the
    /// ready queue.  Since rmcp dispatches each MCP request via
    /// `tokio::spawn` (see `crates/rmcp/src/service.rs` in the rmcp
    /// repository — the per-request dispatch site), the spawn
    /// order matches the request arrival order on stdin, and the FIFO
    /// polling order carries that through to `tx.send()` — the worker
    /// thread processes requests in the order they arrived on the
    /// wire.
    ///
    /// On a `multi_thread` runtime this guarantee does NOT hold: two
    /// spawned tasks can be polled by different worker threads in
    /// arbitrary order. If you ever change the runtime flavor, you
    /// will need to add explicit ordering (sequence numbers, a Mutex,
    /// or a serialising dispatcher task) here.
    pub async fn eval(
        &self,
        code: String,
        budget_override: Option<Duration>,
        capture_streams: bool,
    ) -> Result<SchemeEvalReply, String> {
        let effective_budget = budget_override.unwrap_or(self.eval_timeout);
        let (reply_tx, reply_rx) = oneshot::channel();
        self.tx
            .send(SchemeRequest {
                code,
                budget: budget_override,
                capture_streams,
                reply: reply_tx,
            })
            .await
            .map_err(|_| "scheme worker thread has exited".to_string())?;

        let Ok(reply) = tokio::time::timeout(effective_budget, reply_rx).await else {
            return Err(format!(
                "scheme_eval timed out after {}s",
                effective_budget.as_secs()
            ));
        };

        match reply {
            Ok(result) => result,
            Err(_) => Err("scheme worker dropped the reply channel".to_string()),
        }
    }

    /// Close the eval channel and wait for the worker thread to exit.
    ///
    /// Consumes the handle so callers cannot accidentally leave a
    /// dangling `Sender` alive (which would prevent the worker from
    /// observing the channel closure and cause this to block
    /// indefinitely).  The returned future is cheap — once the channel
    /// closes the worker's blocking `rx.blocking_recv()` returns
    /// immediately and the thread unwinds.
    ///
    /// Called from the shutdown sequence in `main.rs` to guarantee the
    /// scheme worker has released its `Arc<TransportHandle>` clone
    /// before the binary attempts graceful GDB shutdown.
    pub fn join(mut self) -> Option<JoinHandle<()>> {
        // Drop the sender so the worker thread's blocking_recv returns
        // None on the next iteration.
        drop(self.tx);
        self.join_handle.take()
    }
}

// ---------------------------------------------------------------------------
// Worker loop (runs on dedicated std::thread)
// ---------------------------------------------------------------------------

/// Main loop for the scheme worker thread.
///
/// Main loop for the scheme worker thread.
///
/// The engine is owned here and rebuilt from scratch if a panic
/// escapes `catch_unwind`.  A rebuild failure (extremely unlikely
/// since the prelude is static and was known-good in phase 1) exits
/// the loop — the caller will then see `scheme worker thread has
/// exited` on the next `eval`.
fn worker_loop(
    mut engine: Engine,
    deadline: &EvalDeadline,
    mut rx: mpsc::Receiver<SchemeRequest>,
    transport: &Arc<TransportHandle>,
    allow_shell: bool,
    rt: &tokio::runtime::Handle,
    settings: SchemeSettings,
) {
    while let Some(request) = rx.blocking_recv() {
        process_one(
            &mut engine,
            deadline,
            request,
            transport,
            allow_shell,
            rt,
            settings,
        );
    }

    info!("scheme worker thread exiting (channel closed)");
}

/// Evaluate a single [`SchemeRequest`] against the engine, sending
/// the result (or error) back to the caller via the request's
/// oneshot.  On panic, the engine is rebuilt; on rebuild failure,
/// the error is reported to the caller and the function returns
/// `false` to signal that the worker should exit.
fn process_one(
    engine: &mut Engine,
    deadline: &EvalDeadline,
    request: SchemeRequest,
    transport: &Arc<TransportHandle>,
    allow_shell: bool,
    rt: &tokio::runtime::Handle,
    settings: SchemeSettings,
) {
    let code = request.code.clone();
    let effective_budget = request.budget.unwrap_or(settings.eval_timeout);
    let capture_streams = request.capture_streams;

    // Capture the cursor before eval so stream events produced during
    // this call can be folded into the reply.  Taking the cursor
    // inside the worker (which serialises calls) is race-free; any
    // other site could observe events from adjacent calls.
    let cursor_before = if capture_streams {
        Some(transport.event_cursor())
    } else {
        None
    };

    // Set the eval deadline so wait primitives can validate their
    // timeout against the remaining budget.  Must match the outer
    // tokio::time::timeout armed on the caller side — a mismatch
    // would let the harness kill the worker before check_eval_budget
    // gets a chance to reject an over-long wait.
    if let Ok(mut guard) = deadline.lock() {
        *guard = Some(Instant::now() + effective_budget);
    }

    // Handle a panic from the Steel VM first (rebuild the engine),
    // then handle the normal eval result separately — avoids the
    // nested Ok(Ok(..)) / Ok(Err(..)) / Err(..) antipattern.
    let eval_result = match std::panic::catch_unwind(AssertUnwindSafe(|| engine.run(code))) {
        Ok(r) => r,
        Err(panic_payload) => {
            let msg = panic_message(&panic_payload);
            error!(msg, "scheme engine panicked — rebuilding");
            match build_engine(Arc::clone(transport), allow_shell, rt, settings, deadline) {
                Ok(fresh) => {
                    *engine = fresh;
                }
                Err(rebuild_err) => {
                    error!(
                        %rebuild_err,
                        panic = %msg,
                        "scheme engine rebuild failed after panic; worker exiting"
                    );
                    request
                        .reply
                        .send(Err(format!(
                            "scheme engine panicked and rebuild failed: {rebuild_err}"
                        )))
                        .ok();
                    return;
                }
            }
            request
                .reply
                .send(Err(format!("scheme engine panicked: {msg}")))
                .ok();
            return;
        }
    };

    let response = match eval_result {
        Ok(values) => {
            let display = marshal::steel_to_display_string(&values);
            let result = marshal::steel_values_to_json(&values);
            let (streams, truncated_streams, truncated_journal) =
                collect_stream_events(transport, cursor_before);
            Ok(SchemeEvalReply {
                display,
                result,
                streams,
                truncated_streams,
                truncated_journal,
            })
        }
        Err(err) => Err(format!("{err}")),
    };

    // A send error means the caller timed out and dropped the
    // receiver — that's expected, not a worker-level error.
    request.reply.send(response).ok();
}

/// Drain stream-class events (`console`, `target-output`, `log`)
/// produced during the eval, capped at [`MAX_INLINED_STREAM_EVENTS`].
/// Returns `(events, truncated_streams, truncated_journal)`; both
/// flags are `false` when `cursor_before` is `None` (capture disabled).
fn collect_stream_events(
    transport: &Arc<TransportHandle>,
    cursor_before: Option<framewalk_mi_transport::EventSeq>,
) -> (Vec<crate::server_helpers::ObservedEvent>, bool, bool) {
    let Some(cursor) = cursor_before else {
        return (Vec::new(), false, false);
    };

    let payload = crate::server_helpers::drain_observed_events(transport, cursor);
    let mut streams: Vec<_> = payload
        .events
        .into_iter()
        .filter(|ev| matches!(ev.kind, "console" | "target-output" | "log"))
        .collect();

    let truncated_streams = streams.len() > MAX_INLINED_STREAM_EVENTS;
    if truncated_streams {
        streams.truncate(MAX_INLINED_STREAM_EVENTS);
    }

    (streams, truncated_streams, payload.truncated)
}

/// Construct a fresh Steel engine with framewalk bindings and prelude
/// installed.
///
/// Order is load-bearing:
///
/// 1. `new_sandboxed()` — Steel installs its own builtin modules.
/// 2. [`seal_sandbox`] — neutralises `steel/process` and `load`
///    *before* user-visible bindings are registered, so no framewalk
///    binding can accidentally be clobbered by a later seal step.
/// 3. [`bindings::register_all`] — adds `mi`, `mi-quote`,
///    `wait-for-stop` and their Rust closures.
/// 4. [`load_prelude`] — evaluates the compiled-in Scheme prelude.
///    Must run last because the prelude references the bindings from
///    step 3.
fn build_engine(
    transport: Arc<TransportHandle>,
    allow_shell: bool,
    rt: &tokio::runtime::Handle,
    settings: SchemeSettings,
    deadline: &EvalDeadline,
) -> Result<Engine, SteelErr> {
    let mut engine = Engine::new_sandboxed();
    seal_sandbox(&mut engine)?;
    bindings::register_all(&mut engine, transport, allow_shell, rt, settings, deadline);
    load_prelude(&mut engine)?;
    Ok(engine)
}

/// Evaluate the compiled-in Scheme prelude against `engine`.
///
/// Exposed at `pub(crate)` so unit tests can share this code path
/// instead of re-stubbing the prelude load — if the prelude or this
/// helper change, the tests pick the change up automatically.
pub(crate) fn load_prelude(engine: &mut Engine) -> Result<(), SteelErr> {
    engine.run(include_str!("prelude.scm"))?;
    Ok(())
}

/// Overwrite every binding exported by modules that provide host-level
/// side effects (process spawning, filesystem reads via `load`, etc.).
///
/// The approach: instantiate each dangerous module, call `.names()` to
/// enumerate its exports, and generate Scheme `define` expressions
/// that replace each binding with a variadic error-raising function.
///
/// Once the global binding is overwritten, the original Rust function
/// is unreachable — Steel's environment is a mutable map, so the old
/// value loses all references and is collected.  Even if user code
/// later redefines the name, it cannot recover the original capability.
///
/// This is robust against upstream Steel changes: if a new function is
/// added to `process_module()`, `.names()` returns it and the
/// generated `define` seals it automatically.
///
/// Exposed at `pub(crate)` for the same reason as [`load_prelude`].
#[allow(clippy::format_push_string)] // write! alternative requires ignoring an infallible Result
pub(crate) fn seal_sandbox(engine: &mut Engine) -> Result<(), SteelErr> {
    // Modules whose bindings grant host side-effects beyond what
    // our `(mi ...)` primitive intentionally exposes.
    let dangerous_modules: &[(&str, steel::steel_vm::builtin::BuiltInModule)] =
        &[("process", steel::primitives::process::process_module())];

    let mut seal_code = String::new();

    for (module_label, module) in dangerous_modules {
        for name in module.names() {
            seal_code.push_str(&format!(
                "(define ({name} . args) (error \"{name} (from steel/{module_label}) is disabled in the framewalk sandbox\"))\n"
            ));
        }
    }

    // `load` reads arbitrary files as Scheme source — the sandbox
    // blocks `open-input-file` but `load` bypasses that check.
    seal_code
        .push_str("(define (load . args) (error \"load is disabled in the framewalk sandbox\"))\n");

    engine.run(seal_code)?;
    Ok(())
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &dyn std::any::Any) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}
