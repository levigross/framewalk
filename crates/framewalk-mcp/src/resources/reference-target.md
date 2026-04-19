# Target tool reference

Tools for connecting to, downloading to, and disconnecting from remote
targets (gdbserver, simulators, bare-metal JTAG probes). For
attaching to a running local process by PID, see `attach` / `detach`
in `framewalk://reference/session`. For the workflow read
`framewalk://guide/attach`.

---

## `target_select`

**MI command:** `-target-select`
**Signature:** `target_select(transport: String, parameters: String)`
**Description:** Connect to a remote target (e.g. gdbserver).
`transport` is the transport type (`"remote"`, `"extended-remote"`,
`"sim"`, etc.); `parameters` is the transport-specific address (e.g.
`"localhost:3333"`).

```json
{"name": "target_select", "arguments": {"transport": "remote", "parameters": "localhost:3333"}}
```

**Related:** `target_disconnect`, `target_download`, `framewalk://guide/attach`

---

## `target_download`

**MI command:** `-target-download`
**Signature:** `target_download()` — no arguments
**Description:** Download the executable to the remote target. Used
after `load_file` + `target_select` to flash firmware or push a
binary to a stub.

```json
{"name": "target_download", "arguments": {}}
```

**Related:** `target_select`, `target_flash_erase`, `load_file`

---

## `target_disconnect`

**MI command:** `-target-disconnect`
**Signature:** `target_disconnect()` — no arguments
**Description:** Disconnect from the remote target. Use this instead
of `detach` for remote targets.

```json
{"name": "target_disconnect", "arguments": {}}
```

**Related:** `target_select`, `detach`

---

## `target_flash_erase`

**MI command:** `-target-flash-erase`
**Signature:** `target_flash_erase()` — no arguments
**Description:** Erase all known flash memory regions on the target.
Typically called before `target_download` when flashing embedded
firmware.

```json
{"name": "target_flash_erase", "arguments": {}}
```

**Related:** `target_download`

---

## Attach / detach

Attaching to an already-running *local* process uses `attach` and
`detach` from `framewalk://reference/session`, not `target_select`.
The two workflows are distinct:

- **Local pid attach:** `attach(pid)` → inspect/step → `detach()`
- **Remote target:** `target_select(transport, parameters)` → `target_download` → run/step → `target_disconnect()`

See `framewalk://recipe/attach-running` for a walk-through of the
local-pid case.

---

## See also

- `framewalk://guide/attach` — local attach and remote connect
- `framewalk://reference/session` — `attach`, `detach`, `load_file`
- `framewalk://reference/file-transfer` — host ↔ target file copies
- `framewalk://recipe/attach-running` — worked example for local pid
- `framewalk://reference/execution` — running on the connected target
