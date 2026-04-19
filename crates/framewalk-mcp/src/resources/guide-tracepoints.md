# Tracepoints: observing without stopping

## When to use this guide

Read this when stopping the target is unacceptable — production services,
real-time systems, systems under external timing constraints — but you
still need to collect data at specific points in the code. Tracepoints are
"breakpoints that do not stop": when execution reaches them, the target
captures a configurable snapshot (locals, globals, registers, memory
regions) into a trace buffer and keeps running. You then navigate the
buffer offline. For the full tool catalog see
`framewalk://reference/tracepoints`.

## Target requirements

Tracepoints are *not* a GDB-only feature. They require a tracing-capable
target on the far end of the debug connection:

- **gdbserver** with tracing support enabled (built with `--trace-mode`, not
  all distros ship this). Launch gdbserver on the target and `target_select`
  to it before setting tracepoints.
- **In-process agent (`libinproctrace.so`)** for fast tracepoints that
  avoid trap-based entry overhead.
- **Remote stubs** that implement the qTStatus / qTfP protocol subset.

A plain local `attach` or `load_file` session against a native GDB backend
does **not** support tracepoints — `trace_insert` will fail. Run
`trace_status` first to confirm the backend reports tracing support.

## Placing a tracepoint

```json
{"name": "trace_insert", "arguments": {"location": "server.c:142"}}
```

Like breakpoints, tracepoints have a numeric ID. The location syntax is
identical to `set_breakpoint` — function names, file:line, `*0xADDR`.
A freshly inserted tracepoint collects nothing by default. You must attach
a collection action (see the raw MI `-break-commands` with `collect`
actions, or use `break_commands` on the tracepoint's ID to specify what
to capture).

## Controlling collection

```json
{"name": "trace_start", "arguments": {}}
{"name": "trace_stop", "arguments": {}}
{"name": "trace_status", "arguments": {}}
```

`trace_start` flushes the trace buffer and arms every enabled tracepoint.
From this point the target records a trace frame every time a tracepoint
is hit, up to the configured buffer size. `trace_stop` disarms them.
`trace_status` returns running/stopped state, the number of collected
frames, buffer usage, and the stop reason if collection stopped itself
(e.g., buffer full, passcount reached).

## Pass counts

`break_passcount` applies only to tracepoints (despite its name's
breakpoint flavor). After the specified number of hits, tracing stops
automatically.

```json
{"name": "break_passcount", "arguments": {"id": "1", "passcount": 1000}}
```

This is the primary way to bound a tracing session — collect 1000 samples,
then stop, then analyze.

## Trace state variables

A trace state variable is a small named integer maintained by the tracing
agent, writable from collection actions, and visible to tracepoint
conditions. Think of them as counters and flags that live in the agent's
address space, not the target's.

```json
{"name": "trace_define_variable", "arguments": {"name": "$counter", "value": "0"}}
{"name": "trace_list_variables", "arguments": {}}
```

Common uses: count how many times a branch was taken, OR bits together to
record which code paths were seen, maintain a rolling min/max of a value.

## Navigating collected frames

After `trace_stop` (or `trace_status` shows collection halted), use
`trace_find` to position a "trace frame cursor" at a specific collected
frame, then read data from it.

```json
{"name": "trace_find", "arguments": {"mode": {"FrameNumber": {"number": 0}}}}
{"name": "trace_find", "arguments": {"mode": {"TracepointNumber": {"number": 3}}}}
{"name": "trace_find", "arguments": {"mode": {"Pc": {"address": "0x4011a0"}}}}
{"name": "trace_find", "arguments": {"mode": {"Line": {"location": "server.c:142"}}}}
{"name": "trace_find", "arguments": {"mode": "None"}}
```

`mode: "None"` clears the cursor and returns you to live target state.

Once positioned, `trace_frame_collected` returns the data captured at that
frame: collected variables, memory ranges, and register values.

```json
{"name": "trace_frame_collected", "arguments": {}}
```

While a trace frame is selected, standard inspection tools like
`list_locals`, `inspect`, and `backtrace` operate on the *recorded* state
at the collection point, not live target state. This is the trick that
makes post-hoc analysis feel like ordinary debugging.

## Saving a trace file

```json
{"name": "trace_save", "arguments": {"filename": "/tmp/session.trace"}}
```

The file contains the collected frames, tracepoint definitions, and state
variables. Reload it in a later session with `target_select` against a
trace-file target to re-walk the frames without the original process.

## Common pitfalls

- **Trace frames are not stack frames.** `trace_find` navigates *recorded
  tracepoint hits*. `select_frame` navigates the stack at the *currently
  selected* trace frame. Do not use `select_frame` to move between
  tracepoint hits — you will land on the caller of the current hit, not
  the next hit.

- **Collecting everything fills the buffer.** A tracepoint with "collect
  all locals + 1KB of surrounding memory" at a hot call site can fill the
  default buffer in milliseconds. Start by collecting only the specific
  variables you need. Use `trace_status` after a short run to check
  buffer usage before doing a long run.

- **Tracepoint without collection action.** An inserted tracepoint with
  no actions still fires — it just records nothing. `trace_frame_collected`
  returns empty data. Remember to attach `collect` actions via
  `break_commands`.

- **`break_passcount` applies only to tracepoints.** Calling it on a
  regular breakpoint's number either errors or is silently ignored
  depending on GDB version. For ordinary breakpoints, use `break_after`.

- **Forgetting to clear the trace frame cursor.** Leaving `trace_find`
  active means subsequent `inspect` calls read historical data. Call
  `trace_find` with `mode: "None"` before issuing any execution control.

- **Backend does not support tracing.** `trace_insert` returns an error
  like "Target does not support tracepoints" on a local native session.
  Confirm the target with `trace_status` first — if it reports the
  feature unsupported, you need gdbserver with tracing enabled.

- **Fast tracepoints require a jump-able instruction.** Fast tracepoints
  replace a 5-byte instruction with a jump; if the tracepoint lands on a
  shorter instruction GDB silently falls back to a slow tracepoint.
  Check `trace_status` output for the type.

## Example session

```json
{"name": "target_select", "arguments": {"target": "remote:localhost:9999"}}
{"name": "trace_status", "arguments": {}}
{"name": "trace_define_variable", "arguments": {"name": "$requests", "value": "0"}}
{"name": "trace_insert", "arguments": {"location": "handle_request"}}
{"name": "break_commands", "arguments": {"id": "1", "commands": ["collect req->path", "collect req->body_len", "collect $requests++"]}}
{"name": "break_passcount", "arguments": {"id": "1", "passcount": 1000}}
{"name": "trace_start", "arguments": {}}
{"name": "trace_status", "arguments": {}}
{"name": "trace_stop", "arguments": {}}
{"name": "trace_find", "arguments": {"mode": {"FrameNumber": {"number": 0}}}}
{"name": "trace_frame_collected", "arguments": {}}
{"name": "list_locals", "arguments": {}}
{"name": "trace_find", "arguments": {"mode": {"FrameNumber": {"number": 42}}}}
{"name": "inspect", "arguments": {"expression": "req->body_len"}}
{"name": "trace_save", "arguments": {"filename": "/tmp/req.trace"}}
{"name": "trace_find", "arguments": {"mode": "None"}}
```

The session connects to a remote gdbserver, confirms tracing support,
defines a request counter, inserts a tracepoint on the request handler,
attaches a collection action that captures two fields and bumps the
counter, bounds the run to 1000 hits, starts, stops once the bound is
reached, walks the first and forty-second collected frames, saves the
result to disk, and clears the cursor so live execution is available
again.

## See also

- `framewalk://reference/tracepoints` — full tool catalog and schemas
- `framewalk://guide/breakpoints` — for regular stop-the-target patterns
- `framewalk://guide/inspection` — tools that work against trace frames too
- `framewalk://guide/attach` — connecting to a remote gdbserver
- `framewalk://recipe/tracepoint-session` — complete worked example
