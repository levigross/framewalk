# Stack tool reference

Tools for inspecting the call stack and listing variables at specific
frames. Call these after a stop to understand where the target is and
what its local state looks like. For the conceptual model read
`framewalk://guide/inspection`.

Variables from `list_locals` / `list_arguments` / `list_variables` are
one-shot snapshots — use `watch_create`
(`framewalk://reference/variables`) for expressions you want to track
across stops.

---

## `backtrace`

**MI command:** `-stack-list-frames`
**Signature:** `backtrace()` — no arguments
**Description:** Return the current thread's call stack as a list of
frames. Each frame contains level, address, function name, file, and
line. Start here after any stop.

```json
{"name": "backtrace", "arguments": {}}
```

**Related:** `frame_info`, `stack_depth`, `select_frame`

---

## `frame_info`

**MI command:** `-stack-info-frame`
**Signature:** `frame_info()` — no arguments
**Description:** Return info about the currently selected stack frame.
Use after `select_frame` to confirm you're at the expected location.

```json
{"name": "frame_info", "arguments": {}}
```

**Related:** `backtrace`, `select_frame`

---

## `stack_depth`

**MI command:** `-stack-info-depth`
**Signature:** `stack_depth(max_depth: Option<u32>)`
**Description:** Return the depth (number of frames) of the current
stack. Pass `max_depth` to cap the probe — useful for detecting
runaway recursion without walking the whole chain.

```json
{"name": "stack_depth", "arguments": {"max_depth": 1000}}
```

**Related:** `backtrace`, `frame_info`

---

## `select_frame`

**MI command:** `-stack-select-frame`
**Signature:** `select_frame(level: u32)`
**Description:** Select a stack frame by level within the current
thread. Level 0 is the innermost (current) frame; higher levels are
callers. Subsequent `inspect`, `list_locals`, and `list_arguments`
calls run in the selected frame.

```json
{"name": "select_frame", "arguments": {"level": 2}}
```

**Related:** `backtrace`, `frame_info`, `inspect`

---

## `list_locals`

**MI command:** `-stack-list-locals`
**Signature:** `list_locals(print_values: PrintValues, skip_unavailable: bool)`
**Description:** List local variables of the selected frame.
`print_values` is one of `"NoValues"`, `"AllValues"`, `"SimpleValues"`.
`skip_unavailable: true` omits entries GDB cannot read (e.g.
optimised-out locals).

```json
{"name": "list_locals", "arguments": {"print_values": "AllValues", "skip_unavailable": true}}
```

**Related:** `list_arguments`, `list_variables`, `inspect`

---

## `list_arguments`

**MI command:** `-stack-list-arguments`
**Signature:** `list_arguments(print_values: PrintValues, skip_unavailable: bool, low_frame: Option<u32>, high_frame: Option<u32>)`
**Description:** List arguments of each frame. Pass `low_frame` and
`high_frame` together to restrict to a frame range; omit for all
frames.

```json
{"name": "list_arguments", "arguments": {"print_values": "AllValues", "skip_unavailable": false, "low_frame": 0, "high_frame": 4}}
```

**Related:** `list_locals`, `list_variables`, `backtrace`

---

## `list_variables`

**MI command:** `-stack-list-variables`
**Signature:** `list_variables(print_values: PrintValues, skip_unavailable: bool)`
**Description:** List all variables (locals + arguments) of the
selected frame. The one-call alternative to running `list_locals` and
`list_arguments` separately.

```json
{"name": "list_variables", "arguments": {"print_values": "AllValues", "skip_unavailable": true}}
```

**Related:** `list_locals`, `list_arguments`

---

## `enable_frame_filters`

**MI command:** `-enable-frame-filters`
**Signature:** `enable_frame_filters()` — no arguments
**Description:** Enable frame filter support in stack commands. Once
enabled, Python frame filters registered in GDB can elide or rewrite
frames returned by `backtrace`.

```json
{"name": "enable_frame_filters", "arguments": {}}
```

**Related:** `backtrace`, `enable_pretty_printing`

---

## See also

- `framewalk://guide/inspection` — reading state after a stop
- `framewalk://reference/threads` — selecting a thread before a frame
- `framewalk://reference/variables` — persistent watch objects
- `framewalk://reference/data` — registers and raw memory
- `framewalk://reference/symbols` — looking up functions and types
