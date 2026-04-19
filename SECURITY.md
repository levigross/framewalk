# Security

framewalk exposes GDB's full debugging power to MCP clients (typically
LLM agents). This document describes the security model and the
boundaries that protect the host.

## Threat model

The primary threat is an LLM agent — or a prompt-injected instruction
inside one — issuing a command that escapes from GDB's debugging
context into arbitrary shell execution on the host.

GDB has several shell-escape vectors:

- `-interpreter-exec console "shell <cmd>"` — runs a shell command
- `-interpreter-exec console "!<cmd>"` — shorthand for the above
- `-target-exec-command` — sends a command to the target's shell

## The `--allow-shell` flag

By default, framewalk-mcp blocks raw MI commands that are outside its
generated allowlist surface.
The `mi_raw_command` tool (the only escape hatch that accepts
arbitrary MI syntax) validates every command against an **allowlist**
of known-safe MI operations before forwarding it to GDB. Some entries
are whole families and some are exact commands:

| Allowed entries | Examples |
|---|---|
| Prefix families such as `break-`, `exec-`, `data-`, `stack-`, `var-`, `file-`, `thread-`, `symbol-`, `trace-`, `catch-`, `gdb-`, `list-`, `environment-`, `enable-`, `info-`, `ada-`, `add-`, `remove-`, `dprintf-`, `inferior-`; plus exact target commands such as `-target-attach`, `-target-detach`, `-target-select`, `-target-download`, and exact commands like `-complete` | `-break-insert`, `-exec-run`, `-data-evaluate-expression`, `-target-select remote ...` |

Commands **not** in the allowlist — including `-interpreter-exec`,
`-target-exec-command`, and any future shell-adjacent additions —
are rejected. Commands not starting with `-` (raw CLI) are always
rejected regardless of `--allow-shell`.

The canonical published version of that allowlist is
`framewalk://reference/allowed-mi`, which is generated from the Rust
guard implementation so the docs and code cannot drift.

When `--allow-shell` is explicitly passed at server startup, the
allowlist check is bypassed for commands that start with `-`. This is
intended for trusted local use only — never expose a `--allow-shell`
server to untrusted clients.

**Semantic tools are safe by construction.** Tools like `set_breakpoint`,
`backtrace`, `inspect`, etc. go through typed command builders that
produce only well-formed MI commands. They cannot be coerced into
shell escapes regardless of their input arguments.

## Scheme scripting layer

The `scheme_eval` tool runs Steel Scheme code in a sandboxed engine.
The same MI validation applies — Scheme code calling `(mi cmd)` goes
through the identical `validate_raw_mi_command` guard that protects
`mi_raw_command`.

### Sandbox boundaries

The Steel engine is created with `Engine::new_sandboxed()`, which
provides the following restrictions compared to a full engine:

| Capability | Sandboxed? |
|---|---|
| TCP / HTTP networking | Blocked |
| Filesystem access | Restricted (sandboxed FS module) |
| I/O ports | Restricted (no filesystem-backed ports) |
| Dynamic library loading | Blocked |
| Core Scheme (define, lambda, map, etc.) | Available |
| `(mi cmd)` GDB access | Available (through raw_guard) |

### Known limitations

The `steel/process` module (which provides `command`,
`spawn-process`, `wait`, etc.) is sealed at engine startup: every
export is overwritten with an error-raising stub. This sealing is
robust against upstream Steel changes — `module.names()` enumerates
all exports dynamically, so new functions added in future Steel
releases are automatically sealed.

### Engine isolation

- The Scheme engine runs on a dedicated OS thread, isolated from
  the tokio runtime.
- Each `scheme_eval` call has a 60-second timeout. Infinite loops
  return a timeout error rather than blocking the server.
- Engine panics are caught and recovered — the engine is rebuilt
  from scratch without restarting the server.
- Serialised output is truncated to 256 KB to prevent context
  window exhaustion.

## Logging

- Tool calls are logged at `info` level with metadata only (tool name,
  breakpoint ids, etc.) — never argument contents that might leak
  variable values or memory contents.
- Raw MI commands submitted via `mi_raw_command` or `(mi ...)` are
  logged at `warn` level to flag escape-hatch usage in production.
- Raw MI bytes on the wire are logged at `debug` level only, never
  `info`, to prevent variable values from appearing in production logs.

## Recommendations

| Scenario | Mode | `--allow-shell` |
|---|---|---|
| Local development | Either | No |
| CI / automated testing | Scheme | No |
| Shared / remote server | Scheme | **Never** |
| Trusted reverse engineering | Full or Scheme | Optional |

## Reporting vulnerabilities

If you find a way to bypass the shell-escape guard or otherwise
escalate from MCP tool calls to arbitrary execution, please report
it via a GitHub security advisory on the framewalk repository.
