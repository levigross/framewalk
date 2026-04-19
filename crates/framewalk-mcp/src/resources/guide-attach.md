# Attach: debugging a running process

## When to use this guide

Read this when the process you want to debug is already running — a
service that misbehaves only in production, a long-lived daemon, a
child spawned by a harness, a deadlocked program you want to inspect
without restarting. Attach is also the entry point for remote debugging:
use `target_select` to connect to a gdbserver instance, then treat the
session like a local attach. For the full worked example see
`framewalk://recipe/attach-running`; for the session-management tool
catalog see `framewalk://reference/session` and
`framewalk://reference/target`.

## The attach model

`attach` takes ownership of a running PID via `ptrace(PTRACE_ATTACH)`
(on Linux). The target is paused the moment the attach succeeds — every
thread is stopped at whatever instruction it was executing. From that
point the session behaves like any post-`run` state: you can inspect,
set breakpoints, step, continue.

```json
{"name": "attach", "arguments": {"pid": 4242}}
```

After a successful attach, `backtrace` and `list_threads` show where
every thread actually was. This is the fastest way to diagnose a hang:
attach, dump all thread backtraces, look for the one stuck in a lock.

## First-look workflow

```json
{"name": "attach", "arguments": {"pid": 4242}}
{"name": "list_threads", "arguments": {}}
{"name": "backtrace", "arguments": {}}
{"name": "select_thread", "arguments": {"thread_id": "2"}}
{"name": "backtrace", "arguments": {}}
```

The first `backtrace` shows the thread that received the attach signal
(often whichever happened to be scheduled). `list_threads` enumerates
the rest. Walking each thread's backtrace catches deadlocks where two
threads are each blocked on a lock the other holds.

## Loading symbols

If the binary on disk matches the running process, GDB usually loads
its symbols automatically at attach time. If not, or if the process
dlopened libraries since startup, use `load_file` to point GDB at the
symbol file explicitly:

```json
{"name": "load_file", "arguments": {"path": "/usr/bin/myservice"}}
```

`load_file` after `attach` loads symbols but does not restart the
target. Confirm with `gdb_version` and a `backtrace` — resolved
function names mean symbols are present.

## Resuming and detaching

After inspection, resume the target with `cont`. It runs exactly as
before the attach, with no side effects beyond the time spent paused.

```json
{"name": "cont", "arguments": {}}
```

When you are done, **detach** rather than killing the session. `detach`
releases the ptrace attachment and the target continues running on its
own:

```json
{"name": "detach", "arguments": {}}
```

Killing the framewalk process also releases ptrace (the kernel
auto-detaches when the debugger exits), so the target continues in that
case too — but the clean `detach` path is the one to prefer: it lets
framewalk flush any pending commands, properly remove breakpoints, and
report errors. A kill can leave the target with stale breakpoints
patched into its text section until it next reloads the page.

## Remote attach: gdbserver

For debugging across a network or into a container, `target_select`
points framewalk at a gdbserver instance the remote host is running.

```json
{"name": "target_select", "arguments": {"target": "remote:host.example:9999"}}
{"name": "target_select", "arguments": {"target": "extended-remote:host.example:9999"}}
```

`remote` is a one-shot connection: the gdbserver session is bound to
the one target it was launched against. `extended-remote` keeps the
gdbserver running after detach, allowing you to attach to additional
PIDs on the same host via subsequent `attach` calls.

Clean disconnect:

```json
{"name": "target_disconnect", "arguments": {}}
```

This leaves the remote process running (in the remote case) and the
gdbserver either exited (`remote`) or still alive (`extended-remote`).

## Common pitfalls

- **ptrace permission errors.** On Linux, attaching to a process you
  do not own requires either `CAP_SYS_PTRACE` or running as the same
  UID. Additionally, `kernel.yama.ptrace_scope` may be set to 1 or
  higher, which blocks attach to anything other than a direct child —
  even for the same UID. Either run framewalk as root, grant
  `CAP_SYS_PTRACE` to the binary, or lower `yama.ptrace_scope` (system
  administrator decision). The error message is usually "ptrace:
  Operation not permitted".

- **Attaching to a stopped process.** A process that was sent `SIGSTOP`
  before attach will be paused when you attach (as expected) but
  resuming it requires `SIGCONT` in addition to `cont` — framewalk
  cannot tell the difference between "stopped by attach" and "already
  stopped by signal". If `cont` does not resume execution, the process
  was externally stopped; detach leaves it in that state.

- **Detaching mid-syscall.** If you detach while a thread is blocked
  in a long syscall (e.g., `read` on a socket), the thread resumes the
  syscall cleanly on most kernels, but on older kernels the syscall
  may return with `EINTR`. Target code should handle `EINTR` on all
  blocking calls; if it does not, detaching at the wrong moment can
  cause user-visible errors.

- **Symbols do not match the running binary.** If `/usr/bin/myservice`
  was rebuilt since the running process started, GDB's auto-loaded
  symbols correspond to the *new* binary but the running code is the
  *old* one. Backtraces show wrong function names. Use
  `/proc/PID/exe` to refer to the exact binary the process is running:
  `{"name": "load_file", "arguments": {"path": "/proc/4242/exe"}}`.

- **Breakpoints persist after detach if detach fails.** If `detach`
  errors out (e.g., transport dropped), any breakpoints inserted as
  `int3` opcodes remain patched into the target's text. Retry
  `detach`; if that fails, re-attach and issue `delete_breakpoint` for
  every ID before detaching again.

- **Extended-remote confusion.** After `target_select` with
  `extended-remote`, you are *connected* but not yet *attached* — the
  session has no target. You must call `attach` with a PID next.
  Forgetting this produces errors like "No process" on every
  inspection call.

- **Remote symbol transfer is slow.** gdbserver does not send symbols;
  you need a local copy of the binary with debug info, pointed at via
  `load_file`, for any source-level debugging. Raw register and
  instruction-level debugging works without symbols.

## Example session

```json
{"name": "attach", "arguments": {"pid": 4242}}
{"name": "gdb_version", "arguments": {}}
{"name": "list_threads", "arguments": {}}
{"name": "backtrace", "arguments": {}}
{"name": "select_thread", "arguments": {"thread_id": "2"}}
{"name": "backtrace", "arguments": {}}
{"name": "inspect", "arguments": {"expression": "g_state.counter"}}
{"name": "set_breakpoint", "arguments": {"location": "handle_request"}}
{"name": "cont", "arguments": {}}
{"name": "backtrace", "arguments": {}}
{"name": "delete_breakpoint", "arguments": {"id": "1"}}
{"name": "detach", "arguments": {}}
```

The session attaches to PID 4242, confirms the backend version,
enumerates threads, walks two thread backtraces to look for a hang,
inspects a global, sets a breakpoint on the request handler, resumes
and waits for it to fire, captures a backtrace at the stop, removes
the breakpoint cleanly, and detaches so the service keeps running.

## See also

- `framewalk://reference/session` — `attach`, `detach`, `load_file`, `gdb_version`
- `framewalk://reference/target` — `target_select`, `target_disconnect`, remote modes
- `framewalk://reference/threads` — `list_threads`, `select_thread`
- `framewalk://guide/inspection` — walking thread state post-attach
- `framewalk://recipe/attach-running` — complete worked example
