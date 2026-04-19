# Recipe: conditional breakpoint

## Goal

Set a breakpoint that only stops when a specific condition holds,
observe it firing, and tune the condition or ignore count if it turns
out too noisy or too quiet.

## Prerequisites

- A loaded binary with debug info — `load_file` already called.
- A known source location (`file.c:123`) or function name where you
  want the conditional break.
- A variable or expression that is in scope at that location and
  discriminates the case you care about.

## Steps

1. **Set the breakpoint at the target location.** Get it placed first,
   then attach the condition in a separate step — this lets you verify
   GDB resolved the location before you layer logic on top.
   ```json
   {"name": "set_breakpoint", "arguments": {"location": "parser.c:412"}}
   ```
   Expected: `^done` with a `bkpt` object containing a `number`.
   Remember this number — subsequent tool calls reference it.

2. **Attach the condition.** The expression is evaluated in the target
   every time the breakpoint is hit; the target only stops when it
   returns non-zero.
   ```json
   {"name": "break_condition", "arguments": {"id": "3", "condition": "i == 42"}}
   ```
   Expected: `^done`. For string comparisons use the target's own
   functions, e.g. `"strcmp(name, \"target\") == 0"`.

3. **Run (or continue).** Start the target if it isn't already going.
   ```json
   {"name": "run", "arguments": {}}
   ```
   Expected: `^running`, then eventually a stop record with
   `reason: breakpoint-hit` and `bkptno` matching step 1. If the
   target exits without stopping, the condition never held — see
   troubleshooting.

4. **Confirm the condition actually matched what you meant.** It is
   common to write a condition that fires on a different case than
   you expected (off-by-one, sign confusion, wrong variable).
   ```json
   {"name": "backtrace", "arguments": {}}
   {"name": "inspect", "arguments": {"expression": "i"}}
   ```
   Expected: a frame at the target location and a concrete value for
   the discriminating variable. If the value is unexpected, rethink
   the condition.

5. **Add an ignore count if it stops too often.** Useful for "stop on
   iteration 100 of this loop" patterns where the condition is cheap
   but the hit rate is high.
   ```json
   {"name": "break_after", "arguments": {"id": "3", "count": 99}}
   ```
   Expected: `^done`. The next 99 hits (that satisfy the condition)
   are skipped; the 100th stops the target.

6. **Tear it down when done.** Either delete it or just disable it so
   you can re-enable later without retyping the condition.
   ```json
   {"name": "delete_breakpoint", "arguments": {"id": "3"}}
   ```
   Expected: `^done`. Prefer `disable_breakpoint` if you expect to
   reuse the exact same condition later in the session.

## Troubleshooting

- **Breakpoint silently disables itself**: GDB turns off a conditional
  breakpoint after too many evaluation errors (e.g. condition
  references a variable that isn't always in scope). Check with
  `break_info` — look for `enabled: "n"`. Rewrite the condition to be
  safe at every hit, or move the breakpoint to a location where the
  variables are reliably live.

- **Variable out of scope at the breakpoint line**: common when the
  breakpoint is on the declaration line itself — the local doesn't
  exist yet until the next statement. Move the breakpoint one line
  later and retry.

- **Target runs to completion without stopping**: condition never
  held. Sanity-check by removing the condition temporarily
  (`break_condition` with an empty string), re-running, and
  confirming the plain breakpoint fires at all. If it doesn't, the
  location itself isn't reachable.

- **Target is dramatically slower than normal**: the condition is too
  expensive — likely calling a function on every hit. Replace with a
  cheaper predicate, or use a tracepoint instead (see variants).

- **Condition references a macro**: GDB only sees macros if the
  binary was built with `-g3`. Rebuild with `-g3` or expand the macro
  manually in the condition.

## Variants

- **Hit-count-only breakpoint.** Skip the condition entirely and use
  `break_after` with a large count to fire only on the Nth hit. Cheap
  and reliable for "catch the loop on iteration 1000" workflows.

- **Logging breakpoint via `break_commands`.** Attach a CLI command
  list that prints a variable and continues automatically — a
  poor-man's tracepoint that works on any target without trace
  support. Example commands: `["silent", "printf \"i=%d\\n\", i", "cont"]`.

- **Data-triggered break with `set_watchpoint`.** If you care about
  when a value changes rather than when a line is hit, set a
  watchpoint on the variable instead. Useful for corruption hunts
  where you don't know which code path is the culprit.

- **Tracepoint for production-safe collection.** If the target can't
  tolerate stops at all, use a tracepoint (see
  `framewalk://guide/tracepoints`) to record state without pausing.

## See also

- `framewalk://guide/breakpoints` — breakpoint types and condition syntax
- `framewalk://reference/breakpoints` — all break_* tools
- `framewalk://guide/tracepoints` — production-safe alternative
- `framewalk://guide/execution-model` — stop reasons and async-state observation
- `framewalk://guide/inspection` — reading state at a stop
