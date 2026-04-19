# Session tool reference

Tools for starting, loading, observing, and recovering a debug session.
Call `load_file` before any execution tool in
`framewalk://reference/execution`. For the conceptual workflow read
`framewalk://guide/getting-started` first; for attaching to an
already-running process read `framewalk://guide/attach`.

---

## `gdb_version`

**MI command:** `-gdb-version`
**Signature:** `gdb_version()` — no arguments
**Description:** Query GDB's version string. Use this as a liveness probe —
if this call succeeds, the GDB child is running and MI is responsive.
The tool result includes the banner text directly under `version`; no
follow-up `drain_events` call is required.

```json
{"name": "gdb_version", "arguments": {}}
```

**Related:** `list_features` — capability introspection

---

## `load_file`

**MI command:** `-file-exec-and-symbols`
**Signature:** `load_file(path: String)`
**Description:** Load an executable *and* its symbol table. This is the
standard entry point for source-level debugging. `path` must be an absolute
path to a binary built with debug info (`-g`). Must be called before `run`.

```json
{"name": "load_file", "arguments": {"path": "/tmp/hello"}}
```

**Related:** `exec_file` (rarely what you want; executable only, no symbols), `symbol_file`
(symbols only), `framewalk://guide/getting-started`

---

## `attach`

**MI command:** `-target-attach`
**Signature:** `attach(pid: u32)`
**Description:** Attach to a running process by PID. The target process is
paused on attachment; use `cont` to resume it. When you are done, call
`detach` to release the process without killing it.

```json
{"name": "attach", "arguments": {"pid": 12345}}
```

**Related:** `detach`, `framewalk://recipe/attach-running`

---

## `detach`

**MI command:** `-target-detach`
**Signature:** `detach()` — no arguments
**Description:** Detach from the currently-attached process, leaving it
running. This is the clean exit path for `attach` — use it instead of
killing the GDB session if you want the target to continue.

```json
{"name": "detach", "arguments": {}}
```

**Related:** `attach`, `target_disconnect` (for remote targets)

---

## `target_state`

**MI command:** none (local transport journal)
**Signature:** `target_state()` — no arguments
**Description:** Return framewalk's locally observed target state
without sending any MI command to GDB. The payload includes
`running`, `stopped`, `disconnected`, or `exited`, plus the current
journal cursor and the last-seen async MI record.

```json
{"name": "target_state", "arguments": {}}
```

**Related:** `drain_events`, `interrupt_target`,
`framewalk://guide/execution-model`

---

## `drain_events`

**MI command:** none (local transport journal)
**Signature:** `drain_events(cursor?: u64)`
**Description:** Return retained async and stream records seen since
the supplied cursor, without touching GDB. Use this to watch
`*running`, `*stopped`, console output, thread notifications, and
transport parse errors while the target is live.

```json
{"name": "drain_events", "arguments": {"cursor": 0}}
```

**Related:** `target_state`, `interrupt_target`,
`framewalk://guide/execution-model`

---

## `reconnect_target`

**MI command:** `-target-disconnect` + remembered `-target-select`
**Signature:** `reconnect_target()` — no arguments
**Description:** Disconnect and reconnect to the most recently
selected remote target inside the same GDB session, preserving symbol
files and breakpoint definitions. Fails if no successful
`-target-select` operation has been recorded yet.

```json
{"name": "reconnect_target", "arguments": {}}
```

**Related:** `target_select`, `target_disconnect`, `target_state`

---

## See also

- `framewalk://guide/getting-started` — minimal first debug session
- `framewalk://guide/attach` — attaching to a running process
- `framewalk://guide/execution-model` — run/stop semantics and async-state observation
- `framewalk://reference/target` — remote target connection tools
- `framewalk://reference/execution` — what to call after `load_file`
