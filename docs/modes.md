# Operating Modes

framewalk-mcp supports three operating modes that control which tools
are exposed to the MCP client. Choose based on your agent's context
window budget and the complexity of your debugging workflows.

## Full mode (default)

```sh
framewalk-mcp
framewalk-mcp --mode full
```

Exposes **129 tools**: the complete MI-first surface, the operator
helpers (`interrupt_target`, `target_state`, `drain_events`,
`reconnect_target`), plus `scheme_eval`.

Each tool maps to one GDB/MI operation with typed parameters and
structured results. The LLM calls tools individually, one per turn:

```
Turn 1: load_file("/tmp/binary")
Turn 2: set_breakpoint("main")
Turn 3: run()
Turn 4: backtrace()
```

**Advantages:**
- Familiar tool-call pattern — no Scheme knowledge needed
- Each tool has its own schema — the LLM gets parameter validation
- Works with any MCP client that supports tool calling

**Disadvantages:**
- Tool definitions consume ~12k tokens in the context window
- Multi-step workflows require one round-trip per operation
- No loops, conditionals, or variables across tool calls

**When to use:** general-purpose debugging, one-off inspections, when
your agent has ample context window, or when the task is simple enough
that individual tool calls suffice.

## Core mode

```sh
framewalk-mcp --mode core
```

Exposes a curated MI-first subset for high-frequency debugging work,
plus the `mi_raw_command` and `scheme_eval` escape hatches.

Core mode intentionally drops lower-frequency operations from `tools/list`
without changing the underlying model. `run`, `cont`, `step`, `next`,
breakpoints, backtraces, locals, variables, memory, registers, symbols,
and the raw / Scheme escape hatches are still present. Reverse execution,
tracepoints, catchpoints, remote file transfer, and other long-tail tools
move out of the default advertised set.

**Advantages:**
- Keeps the familiar per-tool workflow
- Costs materially less context than full mode
- Still provides expert escape hatches when the curated surface is not enough

**Disadvantages:**
- Lower-frequency tools are no longer discoverable from `tools/list`
- Clients that need the full MI surface regularly should use full mode

**When to use:** everyday debugging where tool count matters, but you
still want direct MCP tools instead of a scripting-only interface.

## Scheme mode

```sh
framewalk-mcp --mode scheme
```

Exposes **5 tools**: `scheme_eval` plus `interrupt_target`,
`target_state`, `drain_events`, and `reconnect_target`.

The LLM writes Steel Scheme code that composes multiple GDB operations
in a single tool call, while still keeping a narrow operator escape
hatch for interrupt, state inspection, event draining, and reconnect:

```
Turn 1: scheme_eval("
  (begin
    (load-file \"/tmp/binary\")
    (set-breakpoint \"main\")
    (run)
    (wait-for-stop)
    (backtrace))
")
```

**Advantages:**
- Tool definition costs ~500 tokens (vs 12k)
- Multi-step workflows execute in a single round-trip
- Full programming: loops, conditionals, variables, recursion
- State persists across calls — build up helper functions over time

**Disadvantages:**
- LLM must generate valid Scheme (higher generation complexity)
- Errors in Scheme code require the LLM to debug its own script
- Single monolithic output — harder for the LLM to reason incrementally

**When to use:** context-constrained agents, complex multi-step
workflows (step N times and collect data), tight debugging loops,
reverse engineering sessions that require pointer chasing or
structure traversal.

## Full and Core include `scheme_eval`

In full mode and core mode, `scheme_eval` is available alongside the
advertised MI tools. This lets the LLM choose the right approach per-task:

- Simple inspection? Call `backtrace` directly.
- Step 50 times and collect a trace? Use `scheme_eval`.

## Choosing in `.mcp.json`

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

You can configure both modes as separate servers and let the user
(or the agent) pick:

```json
{
  "mcpServers": {
    "framewalk": {
      "command": "framewalk-mcp",
      "args": []
    },
    "framewalk-scheme": {
      "command": "framewalk-mcp",
      "args": ["--mode", "scheme"]
    }
  }
}
```

## Environment variables

Both modes respect the same environment variables:

| Variable | Default | Description |
|---|---|---|
| `FRAMEWALK_GDB` | `gdb` | Path to GDB binary |
| `FRAMEWALK_MODE` | `full` | Operating mode (`full`, `core`, or `scheme`; `standard` is accepted as an alias for `full`) |
| `FRAMEWALK_NON_STOP` | `true` | Enable non-stop mode; set `false` for all-stop-only remote stubs |
| `FRAMEWALK_ALLOW_SHELL` | `false` | Allow shell-adjacent MI commands |
| `FRAMEWALK_LOG` | `framewalk=info,rmcp=warn` | Tracing filter |
| `FRAMEWALK_SCHEME_EVAL_TIMEOUT_SECS` | `60` | Default timeout for one `scheme_eval` call |
| `FRAMEWALK_WAIT_FOR_STOP_TIMEOUT_SECS` | `30` | Default timeout for Scheme wait helpers |
