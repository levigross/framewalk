# Scheme tool reference

framewalk-mcp's Scheme mode exposes GDB primarily through a Steel
Scheme scripting layer. Instead of a large menu of per-command MCP
tools, there is one composition tool — `scheme_eval` — plus a small
operator surface (`interrupt_target`, `target_state`, `drain_events`,
`reconnect_target`). The engine is preloaded with a prelude that wraps
the most common MI commands as Scheme functions.

Use Scheme mode when you want to compose multi-step workflows in a
single round-trip (e.g. "set a breakpoint, run, wait for the stop,
print locals"). For the conceptual guide read
`framewalk://guide/scheme`; for mode selection read
`framewalk://guide/modes`.

---

## `scheme_eval`

**MI command:** n/a (dispatches via `mi` / `mi-quote` primitives)
**Signature:** `scheme_eval(code: String, budget_secs?: u64, include_streams?: bool)`
**Description:** Evaluate Steel Scheme code against the live GDB
session. Engine state persists across calls — `define` a helper in
one call and use it in the next. The prelude (below) provides safe
wrappers around common MI commands. The `(mi "...")` primitive
submits a raw MI command line; `(mi-quote s)` escapes a string for
use as an MI parameter; `(wait-for-stop)` returns the current stop if
the target is already halted, otherwise waits for the next stop. Use
the operator tools when you need an out-of-band interrupt, a local
target-state probe, recent async events, or a reconnect.

Raw MI results in Scheme are lossless result-entry lists. Each entry is
a tiny hash-map with `"name"` and `"value"` keys, matching the JSON
tool-result shape. Use `result-field` for unique fields and
`result-fields` for repeated MI keys like `frame=...`.

Successful `scheme_eval` replies now carry structured JSON under the
top-level `result` field. `include_streams` defaults to `false`; keep it
off for the leanest payload and use `drain_events` when you need logs,
warnings, or console output after the fact.

```json
{"name": "scheme_eval", "arguments": {"code": "(load-file \"/tmp/hello\") (set-breakpoint \"main\") (run) (wait-for-stop) (backtrace)"}}
```

**Related:** `framewalk://guide/scheme`, `framewalk://guide/modes`

---

## Prelude functions

Every `scheme_eval` call has the following prelude loaded. All names
are defined at the top level; redefine them in a session to extend
or override behaviour.

### Safe command builder

#### `(mi-cmd operation . params)`

Build and submit an MI command with properly quoted parameters.
`operation` is the MI operation name without the leading `-`. All
`params` are individually quoted via `mi-quote`, so paths with
spaces and expressions with quotes are handled correctly. Example:
`(mi-cmd "file-exec-and-symbols" "/path/with spaces/a.out")` sends
`-file-exec-and-symbols "/path/with spaces/a.out"`.

### Session

#### `(gdb-version)`
Query GDB's version string. Returns a result-entry list with a synthetic
`"version"` field, so `(result-field "version" (gdb-version))` yields the
banner text directly.

#### `(load-file path)`
Load an executable and its symbol table. `path` is a string.

#### `(attach pid)`
Attach to a running process. `pid` may be a number or a numeric string.

#### `(detach)`
Detach from the currently attached process.

### Execution control

#### `(run)`
Run the loaded program from the start.

#### `(cont)`
Continue execution from the current stop.

#### `(step)`
Step into the next source line.

#### `(next)`
Step over the next source line.

#### `(finish)`
Run until the current function returns.

#### `(interrupt)`
Interrupt all running target threads.

#### `(until loc)`
Run until the given location. `loc` is a string.

#### `(step-instruction)`
Step one machine instruction (into calls).

#### `(next-instruction)`
Step one machine instruction (over calls).

#### `(reverse-step)`
Step backward one source line. Requires `target record-full`.

#### `(reverse-next)`
Step backward over one source line. Requires `target record-full`.

#### `(reverse-continue)`
Continue execution backward. Requires `target record-full`.

#### `(reverse-finish)`
Run backward until the current function's caller. Requires
`target record-full`.

### Breakpoints

#### `(set-breakpoint loc)`
Insert a breakpoint at `loc` (string).

#### `(set-temp-breakpoint loc)`
Insert a temporary (one-shot) breakpoint at `loc`.

#### `(set-hw-breakpoint loc)`
Insert a hardware breakpoint at `loc`. Uses a CPU debug register instead
of patching a software INT3 instruction. Required for early-boot kernel
addresses that aren't paged in yet, and for read-only memory regions.

#### `(set-temp-hw-breakpoint loc)`
Insert a temporary (one-shot) hardware breakpoint at `loc`. Auto-deleted
after the first hit.

#### `(delete-breakpoint id)`
Delete a breakpoint. `id` may be the numeric breakpoint id or the string
id returned by MI.

#### `(enable-breakpoint id)`
Enable a disabled breakpoint. `id` may be a number or string id.

#### `(disable-breakpoint id)`
Disable a breakpoint without deleting it. `id` may be a number or string
id.

#### `(list-breakpoints)`
List all breakpoints.

### Stack inspection

#### `(backtrace)`
Return the current thread's call stack as a plain list of frame values.

#### `(list-locals)`
List locals of the selected frame (with values).

#### `(list-arguments)`
List arguments of each frame (with values).

#### `(stack-depth)`
Return the depth of the current stack.

#### `(select-frame level)`
Select a stack frame by level. `level` may be a number or numeric string.

### Threads

#### `(list-threads)`
List threads in the target inferior.

#### `(select-thread id)`
Select a thread by id. `id` may be a number or string id.

### Variables

#### `(inspect expr)`
Evaluate an expression in the current frame and return its value.
`expr` is a string.

### Event journal

#### `(drain-events)`
Return all retained events from the transport's event journal as a list
of hash-maps. Each entry has `"seq"`, `"kind"`, and optional `"text"`,
`"class"`, `"thread"`, `"reason"` keys. Use after commands to inspect
GDB warnings, console output, and async notifications.

#### `(drain-events after-seq)` / `(drain-events after: seq)`
Return events strictly after the given sequence number. Pass the `"seq"`
of the last event you processed to get only new events.

### Result accessors

#### `(result-field name result)`
Extract a uniquely named field from a result-entry list. Returns `#f`
when absent and raises if multiple values exist. Use this for unique
fields like `(result-field "value" (inspect "argc"))`.

#### `(result-fields name result)`
Extract all matching fields from a result-entry list, preserving order.
Use this for repeated MI keys such as the `frame=...` entries inside a
raw `-stack-list-frames` response.

### Composition helpers

#### `(step-n n)`
Step `n` times, collecting each result into a list. Useful for
"step past the next 5 lines" flows.

#### `(next-n n)`
Step-over `n` times, collecting each result into a list.

#### `(run-to loc)`
Set a temporary breakpoint at `loc`, run, and wait for the stop.
The canonical "jump to this function and inspect" one-liner.

### Error handling

#### `(with-handler handler expr ...)`
Catch Scheme or GDB errors without aborting the whole `scheme_eval`
block. Use this around loops or sampling helpers when you want partial
results to survive a late target exit or a failed `inspect`.

Example:

```scheme
(with-handler
  (lambda (err) 'error)
  (inspect "nonexistent_variable"))
```

---

## See also

- `framewalk://guide/scheme` — scripting workflows and the engine model
- `framewalk://guide/modes` — when to use full, core, or Scheme mode
- `framewalk://reference/session` — underlying semantic tools
- `framewalk://reference/execution` — the MI commands the prelude wraps
- `framewalk://reference/breakpoints` — the MI commands the prelude wraps
