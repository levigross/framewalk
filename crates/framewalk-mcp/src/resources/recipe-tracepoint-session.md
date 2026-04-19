# Recipe: tracepoint session

## Goal

Define a tracepoint at a hot code path, collect data every time it's
hit without stopping the target, then navigate the collected trace
frames to analyze the data offline.

## Prerequisites

- Target supports tracing: typically `gdbserver` with trace support,
  or an in-process agent (`libinproctrace.so`) loaded into the target.
  Plain native debugging does **not** support tracepoints.
- Binary loaded (`load_file`) with debug info.
- A location you want to observe passively — usually a hot path where
  stopping the target is unacceptable.

## Steps

1. **Insert the tracepoint.** Syntactically like a breakpoint, but
   with no-stop semantics.
   ```json
   {"name": "trace_insert", "arguments": {"location": "handler.c:87"}}
   ```
   Expected: `^done` with a `bkpt` object whose `type` is
   `"tracepoint"` and a `number` to reference later. If you get
   `target does not support this command`, see troubleshooting.

2. **Define any trace state variables you need.** These are
   target-side counters/flags that tracepoint actions can manipulate
   — useful for hit counts or conditional collection.
   ```json
   {"name": "trace_define_variable", "arguments": {"name": "$hits", "value": "0"}}
   ```
   Expected: `^done`. The variable lives on the target and is updated
   by collection actions without round-tripping to the host.

3. **Start collection.** Tracepoints are inert until you arm the
   collection engine.
   ```json
   {"name": "trace_start", "arguments": {}}
   ```
   Expected: `^done`. The target's trace buffer is now active.

4. **Run the workload.** Either `run` for a fresh start or `cont` if
   you already attached. The target executes at full speed; the
   tracepoint fires silently each time it's hit.
   ```json
   {"name": "cont", "arguments": {}}
   ```
   Expected: `^running`. No stops at the tracepoint.

5. **Stop collection after the interesting window.** You can let it
   run as long as the buffer holds.
   ```json
   {"name": "trace_stop", "arguments": {}}
   ```
   Expected: `^done`. Collection is frozen; the buffer is preserved
   for inspection.

6. **Check what you got.** `trace_status` reports hit count, buffer
   usage, and whether collection is still running.
   ```json
   {"name": "trace_status", "arguments": {}}
   ```
   Expected: fields like `running: 0`, `frames: 1523`, `buffer-size`,
   `buffer-free`. If `frames` is zero, the tracepoint never fired —
   see troubleshooting.

7. **Navigate to the first collected frame.**
   ```json
   {"name": "trace_find", "arguments": {"mode": {"FrameNumber": {"number": 0}}}}
   ```
   Expected: `^done` with the frame selected. GDB's view of "the
   current state" is now the snapshot taken at that tracepoint hit.

8. **Read the data captured at this frame.**
   ```json
   {"name": "trace_frame_collected", "arguments": {}}
   ```
   Expected: collected expressions, registers, and memory ranges. If
   empty, your tracepoint had no collection actions — see
   troubleshooting.

9. **Step through more frames.** Call `trace_find` with successive
   frame numbers, or use `{"Pc": {...}}` / `{"Line": {...}}` /
   `{"TracepointNumber": {...}}` to seek by location (each mode is a
   tagged object whose key names the selector and whose value carries
   the parameters).
   ```json
   {"name": "trace_find", "arguments": {"mode": {"FrameNumber": {"number": 1}}}}
   ```
   Expected: the next snapshot. `inspect` and `backtrace` work
   normally against the frame's captured state.

10. **Persist the trace for offline analysis.**
    ```json
    {"name": "trace_save", "arguments": {"filename": "/tmp/trace.tf"}}
    ```
    Expected: `^done`. The file can be loaded later via
    `target_select` with a `tfile` target type.

11. **Read back any trace state variables.**
    ```json
    {"name": "trace_list_variables", "arguments": {}}
    ```
    Expected: each defined variable with its final value — e.g.
    `$hits = 1523`.

## Troubleshooting

- **`target does not support this command`**: you're on plain native
  debugging. Relaunch the target under `gdbserver` with trace support
  (recent builds enable it by default), or load the in-process agent.
  See the GDB manual "Tracepoints" chapter.

- **`trace_status` shows `frames: 0` after a clearly-exercised
  workload**: either collection was never started (re-check
  `trace_start` was called before `cont`), or the location isn't
  reachable in the code path you ran. Place an ordinary
  `set_breakpoint` at the same location temporarily to confirm it
  fires at all.

- **`trace_frame_collected` returns empty**: the tracepoint has no
  collection actions — a bare tracepoint records only that it was
  hit, nothing else. Attach actions using raw MI `-break-commands`
  with `collect $regs`, `collect my_var`, etc., or define them via
  CLI before `trace_start`.

- **Trace buffer full early**: your collection rate exceeds the
  buffer. Either reduce collection volume (collect fewer
  expressions), cap hits with `break_passcount`, or increase the
  buffer with `-trace-buffer-size` via `mi_raw_command`.

- **`trace_find` fails with `target failed to find requested trace
  frame`**: you walked off the end of the collected frames. Use
  `trace_status` to see the total frame count first.

- **Tracepoint silently does nothing**: location is in dead code, or
  the function was inlined and has no single address. Try a nearby
  line or a non-inlined caller.

## Variants

- **Fast tracepoint** (`-break-insert -a` via `mi_raw_command`):
  uses a jump instruction instead of a trap, dramatically lowering
  overhead. Requires enough space at the target location for the
  jump trampoline and a compatible target agent.

- **Conditional tracepoint**: combine `break_condition` with the
  tracepoint number to collect only when a predicate holds. Cuts
  buffer usage dramatically when the hot path is mostly uninteresting.

- **Hit-count cap**: use `break_passcount` to stop collection
  automatically after N hits — useful when you only want the first
  few observations of a rare event.

- **Offline analysis**: save with `trace_save`, then in a fresh
  framewalk session use `target_select` with type `tfile` and the
  saved path. The navigation flow (`trace_find`,
  `trace_frame_collected`) is identical against a saved trace.

## See also

- `framewalk://guide/tracepoints` — tracepoint model and agent setup
- `framewalk://reference/tracepoints` — all trace_* tools
- `framewalk://reference/breakpoints` — `break_passcount`, `break_commands`
- `framewalk://guide/execution-model` — why tracepoints don't stop
- `framewalk://guide/inspection` — reading state at a selected frame
