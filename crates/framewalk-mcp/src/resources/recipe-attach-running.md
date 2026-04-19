# Recipe: attach to a running process

## Goal

Attach to an already-running process, inspect its current state without
disturbing it unduly, then detach cleanly leaving the target running.

## Prerequisites

- A target process PID to attach to. On the host, `pgrep -f my-daemon`
  (or `pidof`) is the usual way to find one.
- Sufficient privileges: same UID as the target, or `CAP_SYS_PTRACE`, or
  kernel `yama.ptrace_scope=0`. Check with
  `cat /proc/sys/kernel/yama/ptrace_scope` — `0` is permissive, `1`
  requires same UID, `2` requires capability, `3` disables ptrace.
- No other debugger already attached to the PID (ptrace is exclusive).

## Steps

1. **Attach to the PID.** This suspends the target wherever it was
   executing at the moment of attach.
   ```json
   {"name": "attach", "arguments": {"pid": 12345}}
   ```
   Expected: `^done` plus a stop record. The target is now paused and
   under framewalk's control. No assumption about where it stopped —
   it could be mid-syscall, mid-function, or spinning in a loop.

2. **Take a backtrace.** First thing at any unexpected stop: find out
   where you landed.
   ```json
   {"name": "backtrace", "arguments": {}}
   ```
   Expected: a frame list. Frame 0 is wherever the target was when the
   ptrace stop hit. If the innermost frame is in libc/kernel this is
   normal for a process caught in a blocking syscall.

3. **Enumerate threads.** A running daemon usually has several — you
   attached to the whole process, not just one thread.
   ```json
   {"name": "list_threads", "arguments": {}}
   ```
   Expected: one entry per thread with state, frame, and thread id.
   Note the `current-thread-id` so you know which thread the backtrace
   in step 2 belongs to.

4. **Inspect live state.** Read a global, a counter, or any expression
   visible in the current frame. Avoid expressions with side effects.
   ```json
   {"name": "inspect", "arguments": {"expression": "g_request_count"}}
   ```
   Expected: a concrete value. Use this to confirm the target is in the
   state you expected before doing anything invasive.

5. **Optionally let it run, then interrupt again.** Useful for catching
   the target in a different phase of its work.
   ```json
   {"name": "cont", "arguments": {}}
   {"name": "interrupt", "arguments": {}}
   ```
   Expected: `^running` on `cont`, then a stop record with reason
   `signal-received` / `SIGINT` after `interrupt`. Repeat step 2-4 to
   compare state between the two snapshots.

6. **Detach cleanly.** The target resumes normal execution, unaware it
   was paused.
   ```json
   {"name": "detach", "arguments": {}}
   ```
   Expected: `^done`. The process continues from wherever it was last
   stopped. Any breakpoints you set are removed as part of detach.

## Troubleshooting

- **`ptrace: Operation not permitted`**: kernel yama is blocking you.
  Check `/proc/sys/kernel/yama/ptrace_scope`. If it's `1`, confirm you
  are the same UID as the target (`ps -o uid= -p <pid>`). If it's `2`,
  you need `CAP_SYS_PTRACE` (run under `sudo` or grant the capability
  to gdb).

- **`ptrace: No such process`**: the PID died between `pgrep` and
  `attach`, or you typo'd the number. Re-check with `ps -p <pid>`.

- **`ptrace: Operation not permitted` but UID matches**: another
  debugger is already attached. Check with
  `cat /proc/<pid>/status | grep TracerPid` — nonzero means someone
  else owns it. Only one tracer per process.

- **Backtrace is deep in `__libc_read` / `epoll_wait` / kernel frames**:
  normal for an idle daemon blocked on I/O. The process will resume its
  syscall naturally when you `cont` or `detach`.

- **Attaching to a zombie or `T` (stopped) state process**: ptrace will
  fail or produce a misleading attach. Confirm target state on the host
  first: `ps -o stat= -p <pid>` — `Z` is zombie (unattachable), `T` is
  already stopped by a signal.

- **`list_threads` shows only one thread but you expected many**: the
  target may not have finished `clone()`-ing yet, or you attached
  before the threads were spawned. Re-check after a brief `cont` /
  `interrupt` cycle.

## Variants

- **Remote attach via gdbserver.** If the target is running on another
  host under `gdbserver --attach <host>:<port> <pid>`, skip `attach`
  and use `target_select` with type `extended-remote` and the address.
  The rest of the inspection flow is identical.

- **Attach at thread-group granularity.** Use `list_thread_groups` to
  enumerate inferiors before attaching — useful when working with
  multi-process targets or when you want to confirm the target's
  executable path and arguments before you commit to attaching.

- **Non-stop mode.** For daemons where pausing all threads is
  unacceptable, enable non-stop before attach (raw MI
  `-gdb-set non-stop on`) so only the interrupted thread stops while
  the rest keep running. See `framewalk://guide/execution-model`.

## See also

- `framewalk://guide/attach` — attach workflow and permission model
- `framewalk://reference/session` — `attach`, `detach`, `target_select`
- `framewalk://reference/threads` — thread and thread-group tools
- `framewalk://guide/inspection` — reading state without perturbing it
- `framewalk://guide/execution-model` — stop reasons and non-stop mode
