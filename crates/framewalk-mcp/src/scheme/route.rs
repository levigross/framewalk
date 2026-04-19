//! Dynamic MCP tool route for `scheme_eval`.
//!
//! The `#[tool_router]` proc macro does not support `#[cfg]` on
//! individual methods — it collects all `#[tool]` methods at macro
//! expansion time.  Instead of fighting the macro we build the
//! `scheme_eval` tool route programmatically and add it to the router
//! at construction time via [`ToolRouter::add_route`].

use std::sync::Arc;
use std::time::Duration;

use rmcp::handler::server::router::tool::ToolRoute;
use rmcp::handler::server::tool::{parse_json_object, schema_for_type};
use rmcp::model::{CallToolResult, Content, Tool};
use rmcp::ErrorData as McpError;

use crate::scheme::worker::SchemeHandle;
use crate::server::FramewalkMcp;

/// Upper bound on the per-call `budget_secs` override. Chosen to keep a
/// runaway script from pinning the single-threaded Steel worker longer
/// than any realistic kernel-boot wait.
const MAX_BUDGET_SECS: u64 = 3600;

/// Input schema for `scheme_eval`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct SchemeEvalArgs {
    /// Steel Scheme source code to evaluate.
    pub code: String,
    /// Override the default `--scheme-eval-timeout-secs` for this call
    /// only. Use when a single call is expected to wait longer than the
    /// server-wide default (e.g. `(wait-for-stop 600)` during a kernel
    /// boot). While the override is active, other `scheme_eval` callers
    /// are serialised behind this one, so use the smallest value that
    /// covers the workload. Must be > 0; capped at 3600.
    #[serde(default)]
    pub budget_secs: Option<u64>,
    /// When `true` (default), stream-class events (`console`,
    /// `target-output`, `log`) produced during this eval are appended
    /// to the response as a `streams` array.  Saves a round-trip
    /// against `drain-events` in the common "wait then inspect"
    /// pattern.  Disable if you are already draining events elsewhere
    /// or want the smallest possible response payload.
    #[serde(default = "default_include_streams")]
    pub include_streams: bool,
}

fn default_include_streams() -> bool {
    false
}

/// Tool description shown to the LLM.  Kept in a constant so the
/// route builder stays readable.
const DESCRIPTION: &str = "\
Evaluate Steel Scheme (R5RS) code that can compose multiple GDB \
operations in a single call.  The engine state persists across calls \
— `define`d variables and functions survive between invocations.

Registered primitives (Rust):
  (mi cmd)          Submit a raw GDB/MI command string.  Returns a \
lossless result-entry list.  Shell-adjacent commands are rejected unless the \
server was started with --allow-shell.
  (wait-for-stop)   Block until GDB reports a *stopped event.  \
Returns a hash-map with \"reason\", \"thread\", and \"raw\" keys.

Prelude helpers (Scheme):
  (gdb-version), (run), (step), (next), (cont), (finish), (interrupt),
  (set-breakpoint loc), (set-temp-breakpoint loc), (delete-breakpoint id),
  (backtrace), (inspect expr), (list-locals), (list-threads),
  (load-file path), (until loc),
  (result-field name result) — extract a unique field from a result,
  (result-fields name result) — extract all matching fields in order,
  (step-n n) — step n times and collect results,
  (run-to loc) — set a temporary breakpoint, run, and wait for stop.

Example:
  (begin
    (load-file \"/path/to/binary\")
    (set-breakpoint \"main\")
    (run)
    (wait-for-stop)
    (step-n 5)
    (backtrace))

Optional arguments:
  budget_secs — Extend the per-call eval budget past \
--scheme-eval-timeout-secs for this call only.  Useful for long waits \
during kernel boot, e.g. `{\"code\": \"(wait-for-stop 600)\", \
\"budget_secs\": 620}`.  While the override is active, concurrent \
scheme_eval calls queue behind this one, so use the smallest value that \
covers the workload.  Must be > 0; capped at 3600.
  include_streams — When true, appends stream-class events \
(console, target-output, log) captured during this eval to the response \
as a `streams` array so you don't need a separate `drain-events` call.  \
Defaults to false for the smallest possible payload; leave it off unless \
you specifically need inline logs or console output.

Response shape:
  Success → `{\"result\": <json>, \"streams\": [...]}` \
where `streams` is omitted when empty or `include_streams` is false; \
`truncated_streams`/`truncated_journal` flags appear only when true.";

/// Build a [`ToolRoute`] for `scheme_eval` that dispatches to the
/// given [`SchemeHandle`].
pub(crate) fn scheme_eval_route(scheme: Arc<SchemeHandle>) -> ToolRoute<FramewalkMcp> {
    let tool = Tool::new(
        "scheme_eval",
        DESCRIPTION,
        schema_for_type::<SchemeEvalArgs>(),
    );

    ToolRoute::new_dyn(tool, move |mut context| {
        let scheme = Arc::clone(&scheme);
        Box::pin(async move {
            let args: SchemeEvalArgs =
                parse_json_object(context.arguments.take().unwrap_or_default())?;

            let budget = match args.budget_secs {
                Some(0) => {
                    return Err(McpError::invalid_params(
                        "budget_secs must be greater than zero",
                        None,
                    ));
                }
                Some(secs) => Some(Duration::from_secs(secs.min(MAX_BUDGET_SECS))),
                None => None,
            };

            match scheme.eval(args.code, budget, args.include_streams).await {
                Ok(reply) => Ok(CallToolResult::success(vec![Content::text(
                    render_reply_json(&reply, args.include_streams),
                )])),
                Err(err) => Ok(CallToolResult::error(vec![Content::text(err)])),
            }
        })
    })
}

/// Render a [`crate::scheme::worker::SchemeEvalReply`] as a compact JSON
/// string matching the shape documented in [`DESCRIPTION`].  Omits
/// `streams` when empty or when the caller opted out, and omits the
/// truncation flags unless set.
fn render_reply_json(
    reply: &crate::scheme::worker::SchemeEvalReply,
    include_streams: bool,
) -> String {
    use serde_json::{json, Map, Value};
    let mut obj = Map::new();
    obj.insert("result".into(), reply.result.clone());
    if include_streams && !reply.streams.is_empty() {
        obj.insert("streams".into(), json!(reply.streams));
    }
    if reply.truncated_streams {
        obj.insert("truncated_streams".into(), Value::Bool(true));
    }
    if reply.truncated_journal {
        obj.insert("truncated_journal".into(), Value::Bool(true));
    }
    Value::Object(obj).to_string()
}
