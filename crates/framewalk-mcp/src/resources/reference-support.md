# Support tool reference

Introspection tools that tell you what the GDB/MI interpreter
supports, plus the generic `gdb-set` / `gdb-show` settings pair.
Use these to negotiate capabilities at session start or to tune GDB
behaviour (pagination, print options, etc.) mid-session.

---

## `list_features`

**MI command:** `-list-features`
**Signature:** `list_features()` — no arguments
**Description:** List GDB/MI interpreter features. Returns the set of
MI features this GDB build reports, such as `python`, `frozen-varobjs`,
or `undefined-command-error-code`. Use at startup to detect optional
capabilities.

```json
{"name": "list_features", "arguments": {}}
```

**Related:** `list_target_features`, `gdb_version`

---

## `list_target_features`

**MI command:** `-list-target-features`
**Signature:** `list_target_features()` — no arguments
**Description:** List target-specific features (e.g. async, reverse).
Returns features the currently-connected target supports — most
importantly `reverse` for reverse debugging and `async` for
background execution.

```json
{"name": "list_target_features", "arguments": {}}
```

**Related:** `list_features`, `framewalk://reference/execution`

---

## `info_mi_command`

**MI command:** `-info-gdb-mi-command`
**Signature:** `info_mi_command(command: String)`
**Description:** Query whether a specific MI command exists. Pass the
command name with or without the leading `-`. Useful before calling
`mi_raw_command` to probe for a command that may not be in older GDB
builds.

```json
{"name": "info_mi_command", "arguments": {"command": "trace-frame-collected"}}
```

**Related:** `mi_raw_command`, `list_features`

---

## `gdb_set`

**MI command:** `-gdb-set`
**Signature:** `gdb_set(variable: String)`
**Description:** Set a GDB variable (e.g. `pagination off`). The
`variable` argument is the full `name value` string the underlying
`set` command would take.

```json
{"name": "gdb_set", "arguments": {"variable": "pagination off"}}
```

**Related:** `gdb_show`

---

## `gdb_show`

**MI command:** `-gdb-show`
**Signature:** `gdb_show(variable: String)`
**Description:** Show a GDB variable's current value. Pair with
`gdb_set` to round-trip settings.

```json
{"name": "gdb_show", "arguments": {"variable": "pagination"}}
```

**Related:** `gdb_set`

---

## See also

- `framewalk://reference/session` — `gdb_version` as a liveness probe
- `framewalk://reference/allowed-mi` — canonical raw-MI allowlist
- `framewalk://reference/raw` — `mi_raw_command` and the guard
- `framewalk://reference/execution` — reverse / async features gate here
- `framewalk://guide/getting-started` — first session checklist
