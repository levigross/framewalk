# Tracepoint tool reference

Tracepoints collect data at a location without stopping the target.
They are ideal for low-overhead production-style debugging where a
full stop would perturb timing. Start tracing, let the target run,
stop tracing, then `trace_find` through the collected frames.

For the workflow read `framewalk://guide/tracepoints` and the worked
example in `framewalk://recipe/tracepoint-session`.

---

## `trace_insert`

**MI command:** `-break-insert -a`
**Signature:** `trace_insert(location: String)`
**Description:** Insert a tracepoint at a location. Returns the
tracepoint number GDB assigned. The `-a` flag on `-break-insert`
distinguishes this from a regular breakpoint.

```json
{"name": "trace_insert", "arguments": {"location": "main.c:42"}}
```

**Related:** `trace_start`, `break_passcount`, `set_breakpoint`

---

## `trace_start`

**MI command:** `-trace-start`
**Signature:** `trace_start()` — no arguments
**Description:** Start collecting trace data. All armed tracepoints
become active; the target runs and each tracepoint hit records a
frame without stopping.

```json
{"name": "trace_start", "arguments": {}}
```

**Related:** `trace_stop`, `trace_status`

---

## `trace_stop`

**MI command:** `-trace-stop`
**Signature:** `trace_stop()` — no arguments
**Description:** Stop collecting trace data. Frames already collected
remain available via `trace_find` and `trace_frame_collected`.

```json
{"name": "trace_stop", "arguments": {}}
```

**Related:** `trace_start`, `trace_find`

---

## `trace_status`

**MI command:** `-trace-status`
**Signature:** `trace_status()` — no arguments
**Description:** Query trace collection status. Returns whether
tracing is running, how many frames were collected, and stop reason
if applicable.

```json
{"name": "trace_status", "arguments": {}}
```

**Related:** `trace_start`, `trace_stop`

---

## `trace_save`

**MI command:** `-trace-save`
**Signature:** `trace_save(filename: String, ctf: bool, remote: bool)`
**Description:** Save trace data to a file. `ctf: true` writes the
Common Trace Format instead of GDB's default `tfile` format. `remote:
true` tells the remote target to perform the save.

```json
{"name": "trace_save", "arguments": {"filename": "/tmp/trace.ctf", "ctf": true, "remote": false}}
```

**Related:** `trace_status`, `trace_stop`

---

## `trace_list_variables`

**MI command:** `-trace-list-variables`
**Signature:** `trace_list_variables()` — no arguments
**Description:** List trace state variables. Trace state variables
persist across frames and can be manipulated by tracepoint actions.

```json
{"name": "trace_list_variables", "arguments": {}}
```

**Related:** `trace_define_variable`

---

## `trace_define_variable`

**MI command:** `-trace-define-variable`
**Signature:** `trace_define_variable(name: String, value: Option<String>)`
**Description:** Define a trace state variable. `name` must start
with `$`. Optional `value` seeds the variable.

```json
{"name": "trace_define_variable", "arguments": {"name": "$counter", "value": "0"}}
```

**Related:** `trace_list_variables`

---

## `trace_find`

**MI command:** `-trace-find`
**Signature:** `trace_find(mode: TraceFindMode)`
**Description:** Select a trace frame by various criteria (frame
number, tracepoint number, PC address, source line, etc). After
selection, `trace_frame_collected` and the usual inspection tools
operate on the selected frame's snapshot.

```json
{"name": "trace_find", "arguments": {"mode": {"FrameNumber": {"number": 0}}}}
```

**Related:** `trace_frame_collected`, `trace_status`

---

## `trace_frame_collected`

**MI command:** `-trace-frame-collected`
**Signature:** `trace_frame_collected(var_print_values: Option<PrintValues>, comp_print_values: Option<PrintValues>, registers_format: Option<RegisterFormat>, memory_contents: bool)`
**Description:** Return data collected at the current trace frame.
Controls how variable, expression, and register values are formatted;
`memory_contents: true` includes collected memory blocks.

```json
{"name": "trace_frame_collected", "arguments": {"memory_contents": true}}
```

**Related:** `trace_find`, `read_memory`

---

## See also

- `framewalk://guide/tracepoints` — tracing workflow
- `framewalk://recipe/tracepoint-session` — worked example
- `framewalk://reference/breakpoints` — `break_passcount` for auto-stop
- `framewalk://reference/data` — reading memory from a trace frame
- `framewalk://reference/stack` — walking frames at a trace snapshot
