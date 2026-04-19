# Execution model: how commands complete and how stops arrive

## When to use this guide

Read this before calling any execution tool (`run`, `cont`, `step`, `next`,
`finish`, `until`, `jump`, `return_from_function`). It explains the one fact
that most commonly trips up agents new to framewalk: **execution tools return
immediately when the target starts running, not when it stops.** If you treat
them like synchronous calls, you will inspect stale state.

## Core fact

GDB/MI has two kinds of responses:

1. **`^done` / `^connected`** — the command has completed synchronously. The
   tool call returns with the result and the target state is final.
2. **`^running`** — the target has been dispatched and is now executing
   asynchronously. The tool call returns *immediately* with a "running"
   result. The eventual stop (breakpoint hit, signal, step finished, exit)
   arrives later on a separate async channel.

Every tool in `framewalk://reference/execution` is in the second category.
`run`, `cont`, `step`, `next`, `finish`, `until`, and the `reverse_*` family
all complete as soon as GDB accepts the command — before the target actually
stops.

## Observe the target without poking GDB (Full / Core / Scheme)

framewalk boots GDB with `mi-async on`, `pagination off`, and
`confirm off`. By default it also enables `non-stop on` — but this is
configurable via `--no-non-stop` (or `FRAMEWALK_NON_STOP=false`) for
remote stubs that only speak all-stop (e.g. QEMU's gdbstub, many JTAG
probes). In non-stop mode, the consequence is important: while the
target is running, MI queries are no longer a reliable proxy for "wait
until the next stop". They can return immediately with running-state
data.

The canonical observability pattern is now:

1. Call `run` (or `cont`, `step`, etc.). The tool returns `running`.
2. Call `target_state`. This reads framewalk's local event journal, not
   GDB, and returns immediately with `running`, `stopped`,
   `disconnected`, or `exited`.
3. If you need more detail, call `drain_events` with the last cursor to
   read new async records (`*running`, `*stopped`, console output,
   thread/library notifications, parse errors).
4. Once `target_state` says `stopped`, inspect frames, locals, memory,
   or registers. If the target never stops and you need to break in,
   call `interrupt_target`.

```json
{"name": "run", "arguments": {}}
{"name": "target_state", "arguments": {}}
{"name": "drain_events", "arguments": {"cursor": 0}}
{"name": "list_locals", "arguments": {}}
```

## Scheme mode: `wait-for-stop`

Scheme mode offers a primitive the full/core tool modes do not:
`(wait-for-stop)` blocks the script until a `*stopped` async record
arrives and returns a hash-map with `reason`, `thread`, and `raw`
keys. If the target is already stopped, it returns the current stop
immediately from the transport's sequenced event journal. This makes
multi-step workflows trivial:

```scheme
(load-file "/tmp/hello")
(set-breakpoint "main")
(run)
(wait-for-stop)        ; blocks here
(backtrace)
```

`(wait-for-stop)` uses the server's default wait timeout (30 seconds unless
overridden via `--wait-for-stop-timeout-secs` or
`FRAMEWALK_WAIT_FOR_STOP_TIMEOUT_SECS`). You can also override per call with
`(wait-for-stop 300)` or `(wait-for-stop timeout: 300)`. Timeout errors
carry framewalk's locally observed target state, last-seen async record,
and any recent GDB `warning:` log entries (e.g. failed SW breakpoint
installs on unmapped addresses).

> **Timeout budget interaction:** `wait-for-stop` and `cont-and-wait`
> honour the smaller of their per-call timeout and the remaining
> `scheme_eval` budget. If a per-call timeout exceeds the remaining
> budget, the wait raises an error immediately rather than being killed
> silently at the `scheme_eval` boundary. For long waits, bump
> `--scheme-eval-timeout-secs` (default 60s) or set
> `FRAMEWALK_SCHEME_EVAL_TIMEOUT_SECS`.

Use it for any workflow that needs to observe several stops in a row (loop
of `cont` + `wait-for-stop`).

Read `framewalk://guide/scheme` for more on the Scheme primitives.

## Stop reasons

When a stop arrives, its reason is one of:

- `breakpoint-hit` — target hit a breakpoint (payload includes `bkptno`)
- `watchpoint-trigger` — a watched expression changed
- `end-stepping-range` — step/next completed
- `function-finished` — `finish` returned from the current frame
- `location-reached` — `until` reached its target
- `signal-received` — target received a signal (payload includes `signal-name`)
- `exited-normally` — target completed execution
- `exited` — target exited with a code (payload includes `exit-code`)
- `exited-signalled` — target died from a signal
- `fork`, `vfork`, `exec`, `syscall-entry`, `syscall-return` — catchpoints

Branch on the reason when deciding what to do next. A `signal-received`
SIGSEGV probably wants a backtrace and local inspection; an `end-stepping-range`
probably wants another `step` or `cont`.

## Common pitfalls

- **Treating `run` as synchronous.** Do not `run` then immediately
  `list_breakpoints` assuming the breakpoint has been hit. `list_breakpoints`
  returns the bookkeeping state of the breakpoint table, not "was it hit".
  Use `target_state`, `drain_events`, or Scheme `*-and-wait` helpers to
  observe the stop, then inspect frames once the target is stopped.

- **Running after a crash.** If the target exited or died, the next `cont`
  will fail with a GDB error. Check the stop reason before issuing another
  execution command. If the target is in `exited*` or `signal-received`
  (fatal signal) state, the session needs a fresh `run` or a new `load_file`.

- **Reverse execution without the recording target.** `reverse_step`,
  `reverse_next`, `reverse_continue`, `reverse_finish` all require GDB's
  reverse-debugging backend (typically enabled via
  `target record-full` as a raw CLI/MI command). Without it you get an
  "Target child does not support this command" error.

- **Stepping after `return_from_function`.** `return_from_function` does not
  execute the return — it sets the return value and pops the frame. The
  target is still stopped at the call site of the popped frame. Issue
  `cont` or `step` to actually leave.

- **`jump` skips initialization.** `jump` sets the PC without running the
  intervening code. Local variables created in the skipped region are
  uninitialized; C++ destructors for skipped-over objects will not run.
  Use sparingly and only when you understand the target's memory layout.

## Example session

```json
{"name": "load_file", "arguments": {"path": "/tmp/crash"}}
{"name": "set_breakpoint", "arguments": {"location": "main"}}
{"name": "run", "arguments": {}}
{"name": "target_state", "arguments": {}}
{"name": "next", "arguments": {}}
{"name": "next", "arguments": {}}
{"name": "inspect", "arguments": {"expression": "argc"}}
{"name": "cont", "arguments": {}}
{"name": "drain_events", "arguments": {"cursor": 0}}
```

Each tool call's result is read by the agent before the next is issued. The
first `target_state` confirms whether the breakpoint at `main` has been hit
yet. The two `next` calls advance two source lines. `inspect` reads a local.
The final `cont` runs to either another breakpoint or the end of the program;
the trailing `drain_events` call surfaces the async record that tells you
which.

## See also

- `framewalk://reference/execution` — full tool catalog (run, cont, step, …)
- `framewalk://guide/breakpoints` — how to place the stops you want to observe
- `framewalk://guide/inspection` — what to look at once stopped
- `framewalk://guide/scheme` — composition patterns using `wait-for-stop`
- `framewalk://recipe/debug-segfault` — end-to-end crash diagnosis
