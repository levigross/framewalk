# Breakpoint tool reference

Tools for creating, listing, conditioning, and deleting breakpoints,
dynamic-printf breakpoints, and watchpoints. For the conceptual model
(what a breakpoint id is, how conditions are evaluated, the difference
between a breakpoint and a catchpoint) read
`framewalk://guide/breakpoints`.

Catchpoints live in `framewalk://reference/catchpoints`. Tracepoints
(collect-only breakpoints) live in `framewalk://reference/tracepoints`.

---

## `set_breakpoint`

**MI command:** `-break-insert`
**Signature:** `set_breakpoint(location: String, condition: Option<String>, temporary: bool)`
**Description:** Insert a breakpoint. `location` can be a function name
(`main`), a file:line (`hello.c:42`), or a raw address (`*0x400500`).
Returns the breakpoint id GDB assigned. Pass `temporary: true` for a
one-shot breakpoint that auto-deletes on first hit.

```json
{"name": "set_breakpoint", "arguments": {"location": "main", "condition": "argc > 1", "temporary": false}}
```

**Related:** `break_condition`, `delete_breakpoint`, `framewalk://recipe/conditional-breakpoint`

---

## `list_breakpoints`

**MI command:** `-break-list`
**Signature:** `list_breakpoints()` — no arguments
**Description:** List all currently-defined breakpoints. Returns each
breakpoint with its id, type, location, enabled state, hit count, and
condition.

```json
{"name": "list_breakpoints", "arguments": {}}
```

**Related:** `break_info`, `set_breakpoint`

---

## `delete_breakpoint`

**MI command:** `-break-delete`
**Signature:** `delete_breakpoint(id: String)`
**Description:** Delete a breakpoint by id. The id is the number
returned by `set_breakpoint`.

```json
{"name": "delete_breakpoint", "arguments": {"id": "2"}}
```

**Related:** `disable_breakpoint`, `list_breakpoints`

---

## `enable_breakpoint`

**MI command:** `-break-enable`
**Signature:** `enable_breakpoint(id: String)`
**Description:** Enable a disabled breakpoint by id. Pairs with
`disable_breakpoint` for temporarily silencing a breakpoint without
losing its condition or hit count.

```json
{"name": "enable_breakpoint", "arguments": {"id": "2"}}
```

**Related:** `disable_breakpoint`, `list_breakpoints`

---

## `disable_breakpoint`

**MI command:** `-break-disable`
**Signature:** `disable_breakpoint(id: String)`
**Description:** Disable a breakpoint by id without deleting it.

```json
{"name": "disable_breakpoint", "arguments": {"id": "2"}}
```

**Related:** `enable_breakpoint`, `delete_breakpoint`

---

## `break_condition`

**MI command:** `-break-condition`
**Signature:** `break_condition(id: String, condition: String)`
**Description:** Set or modify a breakpoint's condition expression. Pass
an empty string to remove the condition. The condition is evaluated in
the hit frame on every hit; the target stops only if it evaluates to
true.

```json
{"name": "break_condition", "arguments": {"id": "2", "condition": "i == 42"}}
```

**Related:** `set_breakpoint`, `break_after`, `framewalk://recipe/conditional-breakpoint`

---

## `break_after`

**MI command:** `-break-after`
**Signature:** `break_after(id: String, count: u32)`
**Description:** Set a breakpoint's ignore count (skip the next N hits).
After `count` hits the breakpoint behaves normally again.

```json
{"name": "break_after", "arguments": {"id": "2", "count": 100}}
```

**Related:** `break_condition`, `break_passcount`

---

## `break_commands`

**MI command:** `-break-commands`
**Signature:** `break_commands(id: String, commands: Vec<String>)`
**Description:** Set CLI commands to execute when a breakpoint is hit.
Each command runs in GDB's CLI interpreter when the breakpoint fires;
useful for auto-printing state then continuing.

```json
{"name": "break_commands", "arguments": {"id": "2", "commands": ["print x", "continue"]}}
```

**Related:** `set_breakpoint`, `dprintf_insert`

---

## `break_passcount`

**MI command:** `-break-passcount`
**Signature:** `break_passcount(id: String, passcount: u32)`
**Description:** Set a tracepoint's passcount (auto-stop after N
collections). Applies to tracepoints only — see
`framewalk://reference/tracepoints`.

```json
{"name": "break_passcount", "arguments": {"id": "1", "passcount": 1000}}
```

**Related:** `trace_insert`, `break_after`

---

## `break_info`

**MI command:** `-break-info`
**Signature:** `break_info(id: String)`
**Description:** Show info for a single breakpoint by id. Use instead of
`list_breakpoints` when you already have the id and want one record.

```json
{"name": "break_info", "arguments": {"id": "2"}}
```

**Related:** `list_breakpoints`, `set_breakpoint`

---

## `dprintf_insert`

**MI command:** `-dprintf-insert`
**Signature:** `dprintf_insert(location: String, format: String, args: Vec<String>, temporary: bool, condition: Option<String>, ignore_count: Option<u32>, thread_id: Option<String>)`
**Description:** Insert a dynamic printf breakpoint that prints at a
location without stopping (unless a condition fails). `format` is a
printf-style format string; `args` are expressions evaluated in the hit
frame. Ideal for non-intrusive logging.

```json
{"name": "dprintf_insert", "arguments": {"location": "main.c:42", "format": "x=%d\n", "args": ["x"], "temporary": false}}
```

**Related:** `set_breakpoint`, `break_commands`

---

## `set_watchpoint`

**MI command:** `-break-watch`
**Signature:** `set_watchpoint(expression: String, watch_type: WatchType)`
**Description:** Set a watchpoint on an expression. `watch_type` is
`"Write"` (stop on write), `"Read"`, or `"Access"` (read or write). The
target stops whenever the expression's value changes (or is read,
depending on type).

```json
{"name": "set_watchpoint", "arguments": {"expression": "*ptr", "watch_type": "Write"}}
```

**Related:** `set_breakpoint`, `framewalk://guide/breakpoints`

---

## See also

- `framewalk://guide/breakpoints` — concepts and workflows
- `framewalk://recipe/conditional-breakpoint` — worked example
- `framewalk://reference/catchpoints` — library/exception breakpoints
- `framewalk://reference/tracepoints` — non-stop data collection
- `framewalk://reference/execution` — running after setting a breakpoint
