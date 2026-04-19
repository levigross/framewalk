# Breakpoints: placing, conditioning, and managing stops

## When to use this guide

Read this when you need the target to pause at a specific place — a function
entry, a source line, an instruction address, a data access, or a dynamic
event like a library load. It covers the whole placement/modification/removal
lifecycle plus the GDB features that turn a plain breakpoint into a precise
diagnostic probe (conditions, ignore counts, commands, dprintf). For the full
tool catalog with argument schemas see `framewalk://reference/breakpoints`.

## Locating code

Every placement tool takes a `location` string. GDB accepts several forms and
framewalk passes them through unchanged:

- **Function name** — `"main"`, `"MyClass::run"`, `"ns::func(int)"`. GDB
  resolves across all loaded object files.
- **File and line** — `"server.c:142"`, `"src/lib.rs:57"`. The file name is
  matched against the debug-info source paths; basename match is usually
  enough.
- **Line in current file** — `":142"` works once you are stopped in a file.
- **Instruction address** — `"*0x4011a0"`. The leading `*` is required.
- **Offset from a symbol** — `"*main+16"`.
- **Label** — `"mylabel"` for assembly-level labels.

A single source location can resolve to multiple code locations (templates,
inlined functions, multiple instantiations). GDB creates one logical
breakpoint with multiple sub-locations; `list_breakpoints` shows them all.

## Placing a plain breakpoint

```json
{"name": "set_breakpoint", "arguments": {"location": "main"}}
```

The result includes the numeric breakpoint ID. Save it — every subsequent
modification tool takes that ID.

For a one-shot stop (auto-deletes after the first hit) pass `temporary`:

```json
{"name": "set_breakpoint", "arguments": {"location": "server.c:142", "temporary": true}}
```

To stop the moment a function is entered from anywhere — even before its
prologue — some callers prefer function-name placement because GDB will
automatically skip the prologue.

## Conditions and counts

A **condition** is an expression evaluated in the target's context each time
the breakpoint is reached. The target only stops if the expression is true.

```json
{"name": "break_condition", "arguments": {"id": "2", "condition": "n > 100"}}
```

Clear a condition by passing an empty string. Conditions have full access to
locals, globals, and function calls — but see the pitfalls section.

An **ignore count** skips the next N hits unconditionally, then resumes
stopping:

```json
{"name": "break_after", "arguments": {"id": "2", "count": 500}}
```

**Commands on hit** run a list of GDB/MI commands automatically when the
breakpoint fires. Useful for print-and-continue patterns without using
`dprintf`:

```json
{"name": "break_commands", "arguments": {"id": "2", "commands": ["print x", "print y", "continue"]}}
```

A trailing `"continue"` in the command list makes the breakpoint effectively
silent — the target keeps running after logging.

## Watchpoints

`set_watchpoint` stops the target when a memory location is written (or read,
or accessed) rather than when execution reaches an address.

```json
{"name": "set_watchpoint", "arguments": {"expression": "g_counter"}}
{"name": "set_watchpoint", "arguments": {"expression": "*p", "watch_type": "Read"}}
```

Hardware watchpoints are fast but limited in number by the CPU (typically 4
on x86). GDB falls back to software watchpoints (very slow) when hardware
slots are exhausted — check `list_breakpoints` to see which kind you got.

## Dynamic printf

`dprintf_insert` places a breakpoint whose sole job is to format and log a
message, then continue. It is the right tool when you would otherwise add a
`printf` and rebuild.

```json
{"name": "dprintf_insert", "arguments": {"location": "server.c:142", "format": "req=%d path=%s\\n", "args": ["req_id", "path"]}}
```

## Disabling vs deleting

Disable when you want to keep the breakpoint's ID, condition, and command
list for later:

```json
{"name": "disable_breakpoint", "arguments": {"id": "2"}}
{"name": "enable_breakpoint", "arguments": {"id": "2"}}
```

Delete when you are done with it:

```json
{"name": "delete_breakpoint", "arguments": {"id": "2"}}
```

`list_breakpoints` shows every breakpoint, watchpoint, catchpoint, and
tracepoint with hit counts and current condition. `break_info` returns the
same data for a single number.

## Common pitfalls

- **Unresolved locations in not-yet-loaded code.** Setting a breakpoint in a
  shared library before the library is mapped produces a "pending"
  breakpoint — it resolves automatically on load. If you need to act *at*
  load time (to set other breakpoints in newly-available symbols) use a
  catchpoint: see `catch_load` in `framewalk://reference/catchpoints`.

- **Conditions that throw.** If a condition expression segfaults or
  dereferences a null pointer, GDB disables the breakpoint and prints a
  warning. Keep conditions defensive: `p != 0 && p->n > 100`, not
  `p->n > 100`.

- **Inlined functions.** A breakpoint on an inlined function resolves to
  every inline site. Stopping at each may be what you want, or it may flood
  you — use `list_breakpoints` to count sub-locations, then narrow with a
  file:line location instead.

- **Conditions call functions.** A condition like `strcmp(s, "target") == 0`
  makes GDB *call* `strcmp` in the target. If the target is in a signal
  handler or holding a lock this can deadlock. Prefer pure comparisons.

- **Hardware watchpoint silent fallback.** If GDB cannot install a hardware
  watchpoint it falls back to software and performance collapses by 100x.
  Check the watchpoint type after creation.

- **Multi-location ambiguity.** `"foo"` with multiple overloads creates
  breakpoints on all of them. Use a signature: `"foo(int, int)"`.

## Example session

```json
{"name": "load_file", "arguments": {"path": "/tmp/server"}}
{"name": "set_breakpoint", "arguments": {"location": "main"}}
{"name": "set_breakpoint", "arguments": {"location": "handle_request", "temporary": true}}
{"name": "break_condition", "arguments": {"id": "2", "condition": "req->size > 1024"}}
{"name": "set_watchpoint", "arguments": {"expression": "g_conn_count"}}
{"name": "dprintf_insert", "arguments": {"location": "server.c:142", "format": "accepted fd=%d\\n", "args": ["fd"]}}
{"name": "list_breakpoints", "arguments": {}}
{"name": "run", "arguments": {}}
{"name": "backtrace", "arguments": {}}
```

The temporary breakpoint fires once on the first large request, then
auto-deletes. The watchpoint fires every time `g_conn_count` changes. The
dprintf logs accepted connections without stopping the server. The plain
breakpoint on `main` remains available for a re-run.

## See also

- `framewalk://reference/breakpoints` — full tool catalog and argument schemas
- `framewalk://reference/catchpoints` — `catch_load`, fork/exec, syscalls
- `framewalk://guide/execution` — how to advance the target after a stop
- `framewalk://guide/execution-model` — why execution tools return before the stop
- `framewalk://recipe/conditional-breakpoint` — worked end-to-end example
