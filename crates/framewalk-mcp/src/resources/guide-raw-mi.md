# Raw MI: the escape hatch for unwrapped commands

## When to use this guide

Read this when no semantic tool exists for what you need to do — a
GDB/MI command framewalk has not yet wrapped, a command whose behavior
depends on a CLI-style option like `--thread N`, or an extension provided
by a specific GDB plugin. `mi_raw_command` passes a literal MI command
string to GDB and returns the unparsed reply. It is powerful and sharp;
the golden rule is **prefer the semantic tool when one exists**. Semantic
tools give you typed arguments, safe escaping, consistent error shapes,
and up-to-date state tracking in framewalk's own model. Raw MI gives you
none of that. For the blocked-command list and security model see
`framewalk://reference/raw` and `framewalk://reference/allowed-mi`.

## Basic usage

```json
{"name": "mi_raw_command", "arguments": {"command": "-gdb-version"}}
{"name": "mi_raw_command", "arguments": {"command": "-data-evaluate-expression argc"}}
{"name": "mi_raw_command", "arguments": {"command": "-break-insert --thread 3 main"}}
```

The `command` string is the full MI command as GDB would receive it:
leading dash, command name, arguments with MI-style quoting. The response
is whatever GDB returns, lightly parsed into a JSON structure but not
interpreted — the caller is responsible for understanding the fields.

Common use cases:

- **Thread-specific variants** that framewalk's typed wrappers do not
  expose: `-exec-continue --thread 2`, `-stack-list-frames --thread 5`.
- **New-in-GDB features** that predate a framewalk release.
- **GDB Python/Guile scripts** invoked via `-interpreter-exec console`.
- **Setting GDB internal variables** via `set` / `show` CLI commands
  wrapped in `-interpreter-exec`.
- **Target-specific extensions** added by a particular gdbserver build.

## Security model

Several command families are blocked by default because they give the
target a shell or the ability to run arbitrary host code. Blocking these
by default keeps an agent with tool access from being trivially turned
into a host-RCE vector by a malicious transcript or prompt.

Blocked families include:

- **Shell execution** — `shell`, `pipe`, and MI equivalents. These run
  arbitrary host commands.
- **`-target-exec-run`** variants that spawn host processes.
- **File write via `-file-exec-and-symbols`** when combined with a path
  under user control.
- **Python/Guile script sources** that read files from disk
  (`source /path/to/script.py`).
- **`set logging redirect on` + `set logging file`** combinations that
  can write arbitrary host paths.

The exact allowlist is generated from the guard implementation and
published at `framewalk://reference/allowed-mi`. When a blocked command
is submitted, `mi_raw_command` returns an error that points back to that
resource.

## Unblocking

If your use case requires a blocked command and you trust the input
source, there are two ways to enable it:

- **CLI flag:** launch `framewalk-mcp` with `--allow-shell`. This lifts
  the block for the lifetime of the process.
- **Environment variable:** set `FRAMEWALK_ALLOW_SHELL=1` before starting
  the server.

Both are process-wide. There is intentionally no per-request unblocking —
the decision to accept shell-adjacent commands is an operator decision,
not a per-prompt decision.

Use this sparingly. If an LLM agent is driving framewalk with
`--allow-shell` set, any prompt-injection vector in the target's data
becomes a host-RCE vector.

## Why raw output is hard to use

Raw MI responses are not rewritten into framewalk's JSON shapes. You get
the GDB/MI tuple/list/varvalue structure more or less as parsed. Fields
may be named `bkptno` in one GDB version and `bkpt_num` in another;
optional fields may or may not appear; numeric fields may be strings. The
semantic tools absorb these differences; raw consumers must handle them.

Additionally, framewalk's internal state model (what breakpoints it
believes exist, what the current thread is, what the target state is)
is updated by the semantic tools. When you bypass them with raw MI, the
internal model lags or diverges. In particular:

- Inserting a breakpoint via raw `-break-insert` creates a breakpoint GDB
  knows about but framewalk's breakpoint table may not reflect until the
  next `list_breakpoints` call.
- Changing the selected thread via raw `-thread-select` will not update
  framewalk's notion of the current thread for subsequent semantic calls
  until a sync happens.
- Async records arriving because of raw-commanded execution are still
  parsed and delivered, but the stop handlers use framewalk's current
  model — meaning a stop from a raw `-exec-continue --thread 5` may be
  interpreted as happening on whichever thread framewalk thought was
  current.

The safe pattern: after a raw command that changes state, issue the
matching semantic query (`list_breakpoints`, `list_threads`, `backtrace`)
to force framewalk to resync.

## Common pitfalls

- **Untrusted input in raw command strings.** If you build a command by
  concatenating agent-supplied text, you invite GDB command injection —
  the same way SQL injection works. Pass data through semantic tools,
  which escape arguments; never splice strings into an
  `-interpreter-exec console` payload.

- **Wrong escaping of MI string arguments.** MI uses backslash-escaped
  double-quoted strings. A literal backslash is `\\`, a literal quote is
  `\"`. Get this wrong and GDB returns a parse error at an unhelpful
  column.

- **Assuming output schemas.** A raw reply from `-stack-list-frames`
  returns a list of frames, but the exact tuple layout differs between
  GDB 11, 13, and 15. Semantic tools paper over this; raw consumers
  must version-detect.

- **Ignoring the async channel.** Raw commands that start execution
  (`-exec-continue`, `-exec-step`) return `^running` and the stop
  arrives asynchronously exactly like the semantic versions. You still
  need to observe it — see `framewalk://guide/execution-model`.

- **Enabling `--allow-shell` by default.** This removes an important
  safety boundary. Only enable it for trusted local sessions that need
  a specific blocked command.

- **`-file-exec-and-symbols` on a stale session.** Changing the binary
  via raw MI without going through `load_file` leaves framewalk's
  symbol cache stale. Use the semantic tool.

## Example session

```json
{"name": "mi_raw_command", "arguments": {"command": "-gdb-version"}}
{"name": "mi_raw_command", "arguments": {"command": "-list-features"}}
{"name": "mi_raw_command", "arguments": {"command": "-break-insert --thread 2 handle_request"}}
{"name": "list_breakpoints", "arguments": {}}
{"name": "mi_raw_command", "arguments": {"command": "-data-read-memory-bytes 0x7fffffffd000 16"}}
{"name": "gdb_show", "arguments": {"variable": "architecture"}}
{"name": "gdb_set", "arguments": {"variable": "pagination off"}}
```

The session queries GDB version and feature list, inserts a
thread-scoped breakpoint that the typed tool does not currently support,
resyncs framewalk's breakpoint model with `list_breakpoints`, reads raw
memory bytes, then uses semantic support tools for architecture and
pagination. Everywhere a semantic tool would have sufficed, the
semantic tool remains preferable.

## See also

- `framewalk://reference/raw` — blocked-command list and full schema
- `framewalk://reference/allowed-mi` — canonical generated allowlist
- `framewalk://reference/breakpoints` — typed breakpoint tools
- `framewalk://reference/execution` — typed execution tools
- `framewalk://guide/execution-model` — async semantics also apply to raw
- `framewalk://guide/getting-started` — standard workflow without raw
