# Getting Started

framewalk-mcp is an MCP server that gives LLM agents full GDB debugging
capabilities. This guide gets you from zero to a working debug session.

## Install

### From nix flake (recommended)

```sh
nix build github:levigross/framewalk
./result/bin/framewalk-mcp --help
```

Or enter a dev shell with GDB and all tooling:

```sh
nix develop github:levigross/framewalk
```

### From source

```sh
git clone https://github.com/levigross/framewalk
cd framewalk
nix develop --command cargo build --release -p framewalk-mcp
# Binary at target/release/framewalk-mcp
```

## Wire into your MCP client

### Claude Code

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "framewalk": {
      "command": "framewalk-mcp",
      "args": []
    }
  }
}
```

Or for scheme-only mode (smaller context window footprint):

```json
{
  "mcpServers": {
    "framewalk": {
      "command": "framewalk-mcp",
      "args": ["--mode", "scheme"]
    }
  }
}
```

### Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "framewalk": {
      "command": "/path/to/framewalk-mcp",
      "args": []
    }
  }
}
```

### Any MCP client

framewalk-mcp speaks MCP over stdio. Point your client at the binary
with no special transport configuration.

## Choose a mode

framewalk-mcp has three operating modes:

| | Full (default) | Core | Scheme |
|---|---|---|---|
| **Tools exposed** | 129 (complete MI surface + operator tools + `scheme_eval`) | Curated MI subset + `mi_raw_command` + `scheme_eval` | 5 (`scheme_eval` + operator tools) |
| **Context cost** | Highest | Medium | Lowest |
| **Best for** | General-purpose debugging, complete discoverability | Everyday debugging with lower tool-count pressure | Complex multi-step workflows, context-constrained agents |
| **Flag** | `--mode full` (default) | `--mode core` | `--mode scheme` |

In **full mode**, the LLM can call individual tools like
`set_breakpoint`, `run`, `backtrace` — one per turn — or use
`scheme_eval` for multi-step composition.

In **core mode**, the LLM still calls individual tools, but the server
only advertises the common debugger subset plus the raw and Scheme
escape hatches. This keeps the product MI-first while trimming the
initial tool payload.

In **scheme mode**, the LLM writes Steel Scheme code that composes
multiple GDB operations in a single `scheme_eval` call. The mode still
keeps `interrupt_target`, `target_state`, `drain_events`, and
`reconnect_target` available as recovery and observability helpers.
This is ideal when tool definitions would consume too much of the
context window, or when a workflow requires tight loops (step N times,
collect data at each stop).

## First debug session

Compile a test program with debug info:

```sh
gcc -g -O0 -o /tmp/hello examples/hello.c
```

### Full / Core mode

Ask your LLM:

> Load `/tmp/hello`, set a breakpoint at main, run it, and show me the backtrace.

The LLM will call `load_file`, `set_breakpoint`, `run`, then `backtrace`.
That works in both full mode and core mode.

### Scheme mode

Ask your LLM:

> Load `/tmp/hello`, break at `sum_array`, run to it, and show me the local variables.

The LLM will write a single `scheme_eval` call:

```scheme
(begin
  (load-file "/tmp/hello")
  (set-breakpoint "sum_array")
  (run-and-wait)
  (list-locals))
```

## CLI reference

```
framewalk-mcp [OPTIONS]

Options:
    --gdb <PATH>         Path to gdb binary [default: gdb] [env: FRAMEWALK_GDB]
    --cwd <DIR>          Working directory for the GDB child
    --mode <MODE>        full | core | scheme [default: full] [env: FRAMEWALK_MODE]
    --non-stop           Enable GDB non-stop mode during bootstrap (default: true)
    --no-non-stop        Disable non-stop mode for all-stop-only remote stubs
    --allow-shell        Allow shell-adjacent MI commands (dangerous)
    --log <FILTER>       tracing filter [default: framewalk=info,rmcp=warn]
    --scheme-eval-timeout-secs <SECS>
                         Default timeout for one `scheme_eval` call [default: 60]
    --wait-for-stop-timeout-secs <SECS>
                         Default timeout for Scheme wait helpers [default: 30]
    --help               Print help
    --version            Print version
```

Use `--no-non-stop` (or `FRAMEWALK_NON_STOP=false`) when connecting to
all-stop-only remote stubs such as QEMU's gdbstub or many JTAG probes.

## Reading these docs from an MCP client

Once framewalk-mcp is wired into your MCP client, every page in `docs/`
plus a library of topic guides and workflow recipes is reachable as a
**resource**. The agent can call:

```json
{"jsonrpc":"2.0","id":1,"method":"resources/list"}
```

to discover the full catalog (33 entries: 12 guides, 16 per-category
tool references, 5 end-to-end recipes), then read any of them with:

```json
{"jsonrpc":"2.0","id":2,"method":"resources/read",
 "params":{"uri":"framewalk://guide/getting-started"}}
```

This file is reachable as `framewalk://guide/getting-started`. The
modes guide is `framewalk://guide/modes`. The Scheme reference is
`framewalk://guide/scheme`. For stripped binaries or when you do not
have source access, read `framewalk://guide/no-source`.

## Verifying the install

The repo ships a single validation entrypoint that runs the workspace
tests plus the ignored GDB-backed integration suites. From any shell:

```sh
./scripts/validate.sh
```

The script re-enters `nix develop` automatically when needed, then runs
the fast workspace tests and the ignored transport / MCP integration
suites against a real `gdb`.

## Next steps

- [Scheme Reference](scheme-reference.md) — all available functions
- [Modes](modes.md) — detailed comparison of full, core, and scheme modes
- `framewalk://guide/no-source` — working without source access or with a stripped binary
- [Security](../SECURITY.md) — threat model and sandboxing
- [Examples](../examples/) — programs and Scheme scripts to try
