# Raw MI tool reference

The escape hatch for MI commands framewalk does not expose as a
semantic tool. Pass a raw MI command line; the transport forwards it
verbatim (after guard validation) and returns the full result. Prefer
the semantic tools in the other references whenever one exists —
they are faster for the model to reason about and come with typed
arguments. Use `mi_raw_command` only when no semantic tool fits.

For the conceptual explanation of when and why to reach for raw MI
read `framewalk://guide/raw-mi`. For the canonical allowlist surface,
read `framewalk://reference/allowed-mi`.

---

## `mi_raw_command`

**MI command:** any (see guard below)
**Signature:** `mi_raw_command(command: String)`
**Description:** Send a raw MI command and return the full result.
`command` is the full command line without a leading token and
without a trailing newline — e.g. `"-break-insert main"`. Shell-
adjacent commands are rejected unless framewalk-mcp was started with
`--allow-shell`.

```json
{"name": "mi_raw_command", "arguments": {"command": "-break-insert -f main"}}
```

**Related:** `info_mi_command`, `framewalk://guide/raw-mi`

---

## Security model: the shell guard

`mi_raw_command` is the only MCP surface that lets an LLM send
arbitrary MI commands to GDB. GDB's MI grammar contains several
escape hatches that pivot from "just a debugger" into "arbitrary
shell on the host":

- `-interpreter-exec console "shell rm -rf /"`
- `-target-exec-command ...`
- `shell ...` and `!...` CLI forms

To defend against these, framewalk's guard uses an **allowlist** of
MI command families, not a denylist of dangerous ones. Only commands
whose operation name begins with a recognised family are permitted.
This is structurally safer: a new GDB command family cannot bypass
the guard unless it is explicitly added.

### Always rejected

- Empty input.
- Input that does not start with `-` followed by an ASCII letter.
  Raw CLI commands (`shell ls`, `!ls`, `info break`) are always
  rejected, even with `--allow-shell`, because bypassing MI would
  defeat every other framewalk guarantee.

### Allowed command families (default, `--allow-shell` off)

The canonical allowlist is generated from `raw_guard.rs` and published
at `framewalk://reference/allowed-mi`. That resource distinguishes
prefix families from exact commands, so the discoverability surface and
the implementation cannot drift apart.

In particular, `target-*` is **not** a blanket family. Commands such as
`-target-file-put`, `-target-file-get`, and `-target-file-delete` are
not in the raw-MI allowlist and require `--allow-shell` to dispatch
through `mi_raw_command` (the semantic tools in
`framewalk://reference/file-transfer` route around this). `-target-exec-command`
is likewise rejected by default.

### Rejected under default

- `-interpreter-exec ...` (any interpreter, including `mi2`, `mi3`,
  `console`, `python`) — the primary shell escape.
- `-target-exec-command ...` — direct shell exec on some targets.
- Any unknown MI family — future-proofing against new escape hatches.

### Bypassing with `--allow-shell`

Starting framewalk-mcp with `--allow-shell` disables the allowlist
check. The MI-prefix rule (input must start with `-` + letter) is
still enforced. Only use `--allow-shell` in trusted environments
where the LLM client is already authorised to run arbitrary code on
the host.

---

## See also

- `framewalk://guide/raw-mi` — when and why to reach for `mi_raw_command`
- `framewalk://reference/allowed-mi` — canonical generated allowlist
- `framewalk://reference/support` — `info_mi_command` for probing commands
- `framewalk://reference/session` — semantic tools for common flows
- `framewalk://reference/execution` — prefer semantic execution tools
