# Execution tool reference

Tools that drive the inferior forward, backward, or one instruction at a
time. Every execution tool returns `^running` immediately. Observe the
eventual stop via `target_state`, `drain_events`, or the Scheme
`*-and-wait` helpers described in `framewalk://guide/execution-model`.
See `framewalk://guide/execution` for a walk-through.

Call `load_file` (and optionally `set_breakpoint`) before any of these.
Reverse variants require the target to be under `target record-full` (or
an equivalent reverse-debug provider).

---

## Forward execution

## `run`

**MI command:** `-exec-run`
**Signature:** `run()` — no arguments
**Description:** Run the loaded program from the start. Returns
immediately once GDB has started the target. Use `target_state` or
`drain_events` to observe the later stop.

```json
{"name": "run", "arguments": {}}
```

**Related:** `load_file`, `set_args`, `cont`

---

## `cont`

**MI command:** `-exec-continue`
**Signature:** `cont()` — no arguments
**Description:** Continue execution from the current stop. Use after a
breakpoint hit, signal, or manual `interrupt` to resume the target.

```json
{"name": "cont", "arguments": {}}
```

**Related:** `run`, `interrupt`, `until`

---

## `step`

**MI command:** `-exec-step`
**Signature:** `step()` — no arguments
**Description:** Step into the next source line, descending into
function calls if any.

```json
{"name": "step", "arguments": {}}
```

**Related:** `next`, `step_instruction`, `finish`

---

## `next`

**MI command:** `-exec-next`
**Signature:** `next()` — no arguments
**Description:** Execute the next source line, stepping over function
calls.

```json
{"name": "next", "arguments": {}}
```

**Related:** `step`, `next_instruction`, `until`

---

## `finish`

**MI command:** `-exec-finish`
**Signature:** `finish()` — no arguments
**Description:** Run until the current function returns, then stop at
the caller. Use to "exit" a function you accidentally stepped into.

```json
{"name": "finish", "arguments": {}}
```

**Related:** `reverse_finish`, `return_from_function`

---

## `interrupt`

**MI command:** `-exec-interrupt`
**Signature:** `interrupt()` — no arguments
**Description:** Interrupt all running target threads, causing them to
stop at the next safe point. Use when a call to `cont` or `run` is
still in-flight and you need to regain control. In scheme mode, the
out-of-band equivalent is `interrupt_target`.

```json
{"name": "interrupt", "arguments": {}}
```

**Related:** `cont`, `framewalk://guide/execution-model`

---

## `interrupt_target`

**MI command:** `-exec-interrupt --all`
**Signature:** `interrupt_target()` — no arguments
**Description:** Out-of-band interrupt helper that is available in all
modes, including scheme mode. Use this when the target is still
running and you need an operator escape hatch without going through
another `scheme_eval`.

```json
{"name": "interrupt_target", "arguments": {}}
```

**Related:** `interrupt`, `target_state`, `drain_events`

---

## Reverse execution

## `reverse_step`

**MI command:** `-exec-step --reverse`
**Signature:** `reverse_step(reverse: bool)`
**Description:** Step backward to the previous source line. Requires
reverse debugging (`target record-full`). Pass `reverse: true` to
actually run in reverse — `false` is equivalent to forward `step`.

```json
{"name": "reverse_step", "arguments": {"reverse": true}}
```

**Related:** `step`, `reverse_next`

---

## `reverse_next`

**MI command:** `-exec-next --reverse`
**Signature:** `reverse_next(reverse: bool)`
**Description:** Step backward over the previous source line. Requires
reverse debugging (`target record-full`).

```json
{"name": "reverse_next", "arguments": {"reverse": true}}
```

**Related:** `next`, `reverse_step`

---

## `reverse_continue`

**MI command:** `-exec-continue --reverse`
**Signature:** `reverse_continue(reverse: bool)`
**Description:** Continue execution backward. Requires reverse debugging
(`target record-full`). Stops at the previous breakpoint, watchpoint, or
recording boundary.

```json
{"name": "reverse_continue", "arguments": {"reverse": true}}
```

**Related:** `cont`, `reverse_finish`

---

## `reverse_finish`

**MI command:** `-exec-finish --reverse`
**Signature:** `reverse_finish(reverse: bool)`
**Description:** Run backward until the current function's caller.
Requires reverse debugging (`target record-full`).

```json
{"name": "reverse_finish", "arguments": {"reverse": true}}
```

**Related:** `finish`, `reverse_continue`

---

## Instruction-level

## `step_instruction`

**MI command:** `-exec-step-instruction`
**Signature:** `step_instruction()` — no arguments
**Description:** Step one machine instruction (into calls). Use for
assembly-level debugging or inside code without line info.

```json
{"name": "step_instruction", "arguments": {}}
```

**Related:** `step`, `next_instruction`, `disassemble`

---

## `next_instruction`

**MI command:** `-exec-next-instruction`
**Signature:** `next_instruction()` — no arguments
**Description:** Step one machine instruction (over calls).

```json
{"name": "next_instruction", "arguments": {}}
```

**Related:** `next`, `step_instruction`, `disassemble`

---

## Control flow

## `until`

**MI command:** `-exec-until`
**Signature:** `until(location: Option<String>)`
**Description:** Run until a location is reached, or until the next
source line if omitted. Useful for stepping out of a loop: pass the line
just past the loop body.

```json
{"name": "until", "arguments": {"location": "main.c:42"}}
```

**Related:** `cont`, `set_breakpoint`

---

## `return_from_function`

**MI command:** `-exec-return`
**Signature:** `return_from_function(expression: Option<String>)`
**Description:** Make the current function return immediately, optionally
with a value. The return value expression is evaluated in the current
frame. Skips remaining code in the function.

```json
{"name": "return_from_function", "arguments": {"expression": "0"}}
```

**Related:** `finish`, `jump`

---

## `jump`

**MI command:** `-exec-jump`
**Signature:** `jump(location: String)`
**Description:** Jump to a location without stopping. WARNING: skips
intervening code — locals, initialisation, and side effects between the
current PC and the target location are all bypassed.

```json
{"name": "jump", "arguments": {"location": "main.c:100"}}
```

**Related:** `return_from_function`, `until`

---

## See also

- `framewalk://guide/execution-model` — the async run/stop contract
- `framewalk://guide/execution` — walk-through for stepping and continuing
- `framewalk://reference/session` — `load_file`, `attach`, `detach`
- `framewalk://reference/breakpoints` — where to stop
- `framewalk://reference/stack` — inspecting state after a stop
