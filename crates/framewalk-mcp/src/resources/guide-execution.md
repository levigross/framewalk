# Execution control: stepping, finishing, jumping

## When to use this guide

Read this when `run` and `cont` are not enough — when you need to advance the
target by a single source line, step into or over a call, bail out of a
function, skip ahead past a loop, or force an early return. It covers the
full forward-execution toolbox plus `jump` and `return_from_function` for
manual control-flow manipulation, and briefly points at reverse execution.
For the asynchronous semantics of all these tools read
`framewalk://guide/execution-model` first; for the full catalog see
`framewalk://reference/execution`.

## Source-level stepping

**`step`** advances one source line. If that line contains a function call
with debug info, `step` descends into the callee. Use it when you want to
see what a function does.

```json
{"name": "step", "arguments": {}}
```

**`next`** advances one source line but treats calls as atomic — the target
runs through the called function and stops at the next line of the *current*
frame. Use it to skim past calls you trust.

```json
{"name": "next", "arguments": {}}
```

Both accept a repeat count via a `count` argument where applicable; omitting
it means one step.

## Instruction-level stepping

When you need to watch register changes, debug a prologue, or walk through
optimized code where source lines span many instructions, drop to machine
level.

```json
{"name": "step_instruction", "arguments": {}}
{"name": "next_instruction", "arguments": {}}
```

`step_instruction` descends into calls; `next_instruction` steps over them.
A typical workflow: `next_instruction` to the `call` opcode, then
`step_instruction` once to enter, or `next_instruction` to skip.

## Leaving the current frame

**`finish`** runs until the current function returns, then stops at the
caller with the return value available in the stop record.

```json
{"name": "finish", "arguments": {}}
```

Use `finish` when you have stepped into a function and seen enough — it is
much faster than repeated `next` calls and always lands on the correct line.

**`until`** runs until execution leaves the current source line in the
forward direction. Its killer feature is the optional location argument: it
runs until that location is reached *without* stopping at earlier iterations
of the enclosing loop.

```json
{"name": "until", "arguments": {}}
{"name": "until", "arguments": {"location": "server.c:200"}}
```

Without a location: exit the current loop iteration. With a location: run
past the loop to a specific line. Contrast with `cont` to a temporary
breakpoint — `until` is cleaner when you just want to skip forward.

## Forcing control flow

**`return_from_function`** pops the current frame without executing the
remainder of the function. Optionally supply a return value expression.

```json
{"name": "return_from_function", "arguments": {}}
{"name": "return_from_function", "arguments": {"expression": "-1"}}
```

This stops at the caller with the supplied value as if the function had
returned normally. The remaining statements in the function are skipped —
locks held will not be released, allocations will not be freed, destructors
will not run. Use for error-injection experiments, not routine stepping.

**`jump`** sets the program counter to an arbitrary location in the current
function and resumes.

```json
{"name": "jump", "arguments": {"location": "server.c:150"}}
```

This is dangerous in exact proportion to how much code you skip over.
Locals declared in the skipped region are uninitialized. C++ objects whose
constructors were skipped are in indeterminate states. Use `jump` to retry
a piece of code after patching data, or to skip a known-buggy line during a
live-debug experiment.

## Reverse execution

`reverse_step`, `reverse_next`, `reverse_continue`, and `reverse_finish` are
the mirror of their forward counterparts. They require a recording backend —
most commonly GDB's built-in `target record-full`, which must be enabled via
a raw MI command before you start executing the code you want to replay.
Without a record target every reverse call fails with "Target child does not
support this command".

See `framewalk://guide/execution-model` for the recording-backend prerequisite
and how stop records flow during replay.

## Interrupting a runaway

`interrupt` sends the target a stop signal. Use it when `cont` lands the
target in an infinite loop or blocked syscall and you want control back.

```json
{"name": "interrupt", "arguments": {}}
```

After the stop arrives, `backtrace` tells you where the target was — often
the fastest way to find a hang.

## Common pitfalls

- **Stepping into libc.** `step` at a line containing `printf` descends into
  glibc — hundreds of instructions of format parsing before you see your
  code again. If you just wanted to verify the call completes, use `next`,
  or if you are already inside, use `finish`. For opaque third-party calls,
  set a breakpoint *after* the call and use `cont`.

- **Stepping inlined code.** Optimized builds inline aggressively. `next`
  can appear to jump backward, stay on the same line for several calls, or
  skip lines entirely. This is faithful to what the CPU is doing — the
  instruction pointer really is bouncing through inlined bodies. If it is
  too confusing, rebuild with `-O0 -g` or switch to `next_instruction`.

- **`finish` at `main`.** Finishing out of `main` leaves the target in libc
  startup code with no useful source. Use `cont` if you want the program to
  exit cleanly.

- **`until` without a location in a tight loop.** Without an argument, `until`
  only skips the current iteration. If you need to escape the whole loop,
  supply the location of the line *after* the loop.

- **`jump` across declarations.** Jumping forward past `T obj;` leaves `obj`
  with whatever was on the stack, and GDB will still call its destructor
  when the frame unwinds. Prefer `return_from_function` when you want to
  abort rather than skip.

- **`return_from_function` leaks resources.** Locks, file descriptors, and
  heap allocations whose cleanup lived in the skipped tail are leaked for
  the remainder of the session. Acceptable for probing, not for long runs.

- **Reverse execution on unrecorded history.** You can only reverse over code
  that executed while recording was on. Enabling `record-full` *after* a
  crash does not let you rewind into the crash.

## Example session

```json
{"name": "load_file", "arguments": {"path": "/tmp/parse"}}
{"name": "set_breakpoint", "arguments": {"location": "parse_input"}}
{"name": "run", "arguments": {}}
{"name": "backtrace", "arguments": {}}
{"name": "next", "arguments": {}}
{"name": "step", "arguments": {}}
{"name": "finish", "arguments": {}}
{"name": "until", "arguments": {"location": "parse.c:310"}}
{"name": "return_from_function", "arguments": {"expression": "0"}}
{"name": "cont", "arguments": {}}
```

The session stops at `parse_input`, advances one line with `next`, descends
into the next call with `step`, returns from it with `finish`, skips past
a loop to line 310 with `until`, forces the outer function to return 0, and
continues. Each transition was picked to show a distinct tool — a real
session uses only the ones needed for the question at hand.

## See also

- `framewalk://reference/execution` — full catalog including reverse tools
- `framewalk://guide/execution-model` — async semantics, stop reasons, `wait-for-stop`
- `framewalk://guide/breakpoints` — where to stop before stepping
- `framewalk://guide/inspection` — what to look at at each stop
- `framewalk://recipe/debug-segfault` — end-to-end use of step + finish + backtrace
