# Recipe: diagnose a segmentation fault

## Goal

Load a binary that crashes with SIGSEGV, run it, and identify the faulting
line plus the value of the bad pointer.

## Prerequisites

- A binary compiled with debug info (`gcc -g -O0 -o /tmp/crash crash.c`)
- The source file is accessible at the path GDB knows about (compiled path),
  or use `environment_directory` to add source search paths

## Steps

1. **Load the binary.**
   ```json
   {"name": "load_file", "arguments": {"path": "/tmp/crash"}}
   ```
   Expected: `^done`. If GDB complains about missing debug info, rebuild
   with `-g -O0`.

2. **Run to the crash.** No breakpoint needed — SIGSEGV is a hard stop.
   ```json
   {"name": "run", "arguments": {}}
   ```
   Expected: `^running`, then the target executes until it faults.

3. **Take a backtrace.** This is the first thing to do at any unexpected
   stop — it tells you where the faulting instruction is in the call
   hierarchy.
   ```json
   {"name": "backtrace", "arguments": {}}
   ```
   Expected: a list of frames with the innermost frame (frame 0) being the
   faulting site. Confirm the stop reason in the previous tool call's
   output mentions `signal-received` with `signal-name: "SIGSEGV"`.

4. **Inspect the faulting frame.** Frame 0 is selected by default.
   ```json
   {"name": "frame_info", "arguments": {}}
   {"name": "list_locals", "arguments": {}}
   ```
   Expected: the function name, source file, line number, and every local
   variable in scope. Look for a pointer that GDB reports as `0x0` or an
   obviously garbage address.

5. **Evaluate the suspect pointer.** If the backtrace shows a dereference
   like `ptr->field`, evaluate `ptr` directly:
   ```json
   {"name": "inspect", "arguments": {"expression": "ptr"}}
   ```
   Expected: a concrete value. `0x0` confirms a null deref; an unaligned
   or unmapped address suggests use-after-free or a corrupted pointer.

6. **Walk outward to find the origin.** Select the caller frame to see how
   the bad value arrived:
   ```json
   {"name": "select_frame", "arguments": {"level": 1}}
   {"name": "list_locals", "arguments": {}}
   {"name": "list_arguments", "arguments": {}}
   ```
   Repeat for frame 2, 3, ... until you find the function that produced
   the bad pointer.

## Troubleshooting

- **`No stack.` in backtrace output**: the crash destroyed the stack (deep
  recursion, stack smashing). Try `read_registers` with format `"x"` to
  look at SP/BP and neighboring memory via `read_memory`.

- **Frame 0 is in libc or ld-linux**: the crash is in a library call from
  your code. Use `select_frame` to walk up until the frame's source file
  is in your project.

- **Line numbers look wrong**: the binary was built with optimizations.
  Rebuild with `-O0`.

- **Source not found**: GDB can't locate the source file. Use
  `environment_directory` to add the directory, or use
  `list_exec_source_file` to see what path GDB expected.

- **`signal-received` with something other than SIGSEGV**: different
  fault. SIGABRT usually means an assertion or `abort()` call; SIGBUS
  means unaligned access on architectures that care; SIGFPE is a
  floating-point / integer-divide error. The backtrace workflow is the
  same — only the interpretation differs.

## Variants

- **Reproduce under reverse execution.** If the crash is intermittent,
  enable recording before the run: send a raw MI command
  `target record-full` via `mi_raw_command`, then `run`, observe the
  crash, then use `reverse_step` / `reverse_next` to walk backward
  through the events that led to the bad pointer. See
  `framewalk://guide/execution-model` for the reverse commands.

- **Use Scheme to automate the frame walk.** In scheme mode, write a
  loop that calls `select-frame` from 0 upward, collects locals at each
  level, and returns the first frame whose source file matches your
  project. Read `framewalk://guide/scheme` for the primitives.

- **Core dumps instead of live run.** If you have a core file, use
  `target_select` with a `core` target type instead of `run`. The rest
  of the inspection workflow is identical.

## See also

- `framewalk://guide/execution-model` — stop reasons and async-state observation
- `framewalk://guide/inspection` — frame, locals, registers, memory
- `framewalk://guide/breakpoints` — conditional breakpoints for harder-to-reach crashes
- `framewalk://reference/stack` — frame and local inspection tools
- `framewalk://reference/data` — memory and register reads
