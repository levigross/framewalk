# framewalk

[![PR Unit Tests](https://github.com/levigross/framewalk/actions/workflows/pr-unit-tests.yml/badge.svg)](https://github.com/levigross/framewalk/actions/workflows/pr-unit-tests.yml)

A clean-room Rust implementation of the GDB/MI v3 protocol, designed
to give LLM agents native debugging capabilities over
[MCP](https://modelcontextprotocol.io).

framewalk-mcp connects to a live GDB session and exposes the full
MI-first debugging surface as MCP tools, a curated core profile, or a
single Scheme scripting tool that lets the agent compose arbitrary
multi-step workflows in one call.

## Why

LLMs are surprisingly good at debugging when given the right tools.
But driving GDB through a shell is fragile — the output is
unstructured, state is hard to track, and one malformed command can
derail a session.

framewalk solves this by implementing the GDB/MI v3 protocol from
scratch in Rust and wrapping it as an MCP server. Every tool call
returns structured data. The protocol layer tracks threads,
breakpoints, stack frames, and variable objects automatically. The
agent just asks for what it needs.

For complex workflows — step 50 times and collect a trace, chase a
linked list, or count allocations vs frees — the embedded
[Steel](https://github.com/mattwparas/steel) Scheme interpreter lets
the agent do it all in a single round-trip instead of 50 separate
tool calls.

## Quick start

### Install

```sh
nix build github:levigross/framewalk
./result/bin/framewalk-mcp --help
```

Or from source:

```sh
git clone https://github.com/levigross/framewalk
cd framewalk
nix develop --command cargo build --release -p framewalk-mcp
```

### Add framewalk to your own flake

The flake exports packages for `x86_64-linux` and `aarch64-linux`.
The wrapped package is `framewalk-mcp`, which sets `FRAMEWALK_GDB`
to the `gdb` from `nixpkgs`. The static binary is exported as
`framewalk-mcp-static`.

#### Plain flake

Use the package outputs directly when you want to pull framewalk into a
regular flake without importing any modules:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    framewalk.url = "github:levigross/framewalk";
  };

  outputs = { nixpkgs, framewalk, ... }:
    let
      system = "x86_64-linux";
      pkgs = import nixpkgs { inherit system; };
    in
    {
      packages.${system}.framewalk-mcp = framewalk.packages.${system}.framewalk-mcp;

      devShells.${system}.default = pkgs.mkShell {
        packages = [ framewalk.packages.${system}.framewalk-mcp ];
      };
    };
}
```

If you just want the default package, `framewalk.packages.${system}.default`
is the same wrapped `framewalk-mcp` binary.

#### flake-parts

For `flake-parts`, framewalk now exports `flakeModules.default`. Import it,
enable it per system, and use the exposed package handles:

```nix
{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-parts.url = "github:hercules-ci/flake-parts";
    framewalk.url = "github:levigross/framewalk";
  };

  outputs = inputs @ { flake-parts, ... }:
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.framewalk.flakeModules.default
      ];

      systems = [ "x86_64-linux" ];

      perSystem = { config, pkgs, ... }: {
        framewalk.enable = true;

        devShells.default = pkgs.mkShell {
          packages = [ config.framewalk.package ];
        };
      };
    };
}
```

When `framewalk.enable = true`, the module exposes:

- `config.framewalk.package` — the wrapped `framewalk-mcp` package for the current system
- `config.framewalk.staticPackage` — the static `framewalk-mcp-static` package
- `packages.framewalk-mcp` and `packages.framewalk-mcp-static` by default

If you want different package names in your downstream flake, set
`framewalk.packageName` and `framewalk.staticPackageName`.

### Configure your MCP client

Add to `.mcp.json` (Claude Code) or `claude_desktop_config.json`
(Claude Desktop):

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

### Debug something

Compile a program with debug info and ask your agent:

> Load `/tmp/my_program`, set a breakpoint at `process_request`, run
> it, and show me the local variables when it stops.

The agent calls `load_file`, `set_breakpoint`, `run`, and
`list_locals` — each returning structured GDB/MI results.

## Modes

framewalk-mcp has three operating modes that control the trade-off
between tool granularity and context window cost.

### Full mode (default)

```sh
framewalk-mcp
framewalk-mcp --mode full
```

Exposes **129 tools** covering the full GDB/MI surface plus
`scheme_eval`: session
management, execution control (including reverse debugging),
breakpoints, watchpoints, catchpoints, stack inspection, thread
management, variable objects, memory and register access, disassembly,
symbol queries, tracepoints, remote target operations, and the
`scheme_eval` scripting tool.

Each tool maps to one GDB/MI operation with typed parameters. The
agent calls them individually, one per turn.

### Core mode

```sh
framewalk-mcp --mode core
```

Exposes a curated MI-first subset for day-to-day debugging plus the
`mi_raw_command` and `scheme_eval` escape hatches. Core mode keeps the
same debugger model as full mode, but trims lower-frequency operations
from the advertised tool list so clients spend less context budget on
the long tail.

### Scheme mode

```sh
framewalk-mcp --mode scheme
```

Exposes **5 tools**: `scheme_eval` plus `interrupt_target`,
`target_state`, `drain_events`, and `reconnect_target`. The agent writes
[Steel Scheme](https://github.com/mattwparas/steel) code that
composes multiple GDB operations in a single call:

```scheme
(begin
  (load-file "/tmp/binary")
  (set-breakpoint "main")
  (run)
  (wait-for-stop)
  (step-n 5)
  (backtrace))
```

Tool definitions drop from ~12k tokens to ~500. The engine state
persists across calls, so the agent can build up helper functions
over a session.

Choose scheme mode when context window space is tight, or when the
task involves loops, conditionals, or data collection across many
stops.

## Live resources

framewalk-mcp exposes **33 instructional resources** over the standard
MCP `resources/list` and `resources/read` methods. Instead of cramming
every usage hint into the initial `instructions` string, the server
advertises a library of topic guides, per-category tool references, and
end-to-end workflow recipes that an agent can pull in on demand when it
needs them:

- **12 guides** — `framewalk://guide/getting-started`,
  `framewalk://guide/execution-model`, `framewalk://guide/breakpoints`,
  `framewalk://guide/execution`, `framewalk://guide/inspection`,
  `framewalk://guide/variables`, `framewalk://guide/tracepoints`,
  `framewalk://guide/raw-mi`, `framewalk://guide/attach`,
  `framewalk://guide/no-source`, `framewalk://guide/modes`,
  `framewalk://guide/scheme`
- **16 tool references** — one per category under
  `framewalk://reference/*` (session, execution, breakpoints,
  catchpoints, stack, threads, data, variables, symbols, tracepoints,
  target, file-transfer, support, raw, scheme), plus
  `framewalk://reference/allowed-mi` — the canonical raw-MI allowlist
  generated from the guard implementation
- **5 workflow recipes** — `framewalk://recipe/debug-segfault`,
  `framewalk://recipe/attach-running`,
  `framewalk://recipe/conditional-breakpoint`,
  `framewalk://recipe/tracepoint-session`,
  `framewalk://recipe/kernel-debug`

All resources are `text/markdown`. Discover them with a single call:

```json
{"jsonrpc":"2.0","id":1,"method":"resources/list"}
```

Read any guide by URI:

```json
{"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":"framewalk://guide/getting-started"}}
```

Agents typically call `resources/list` once on connection, then
`resources/read` whichever guide is relevant to the current task — a
segfault investigation triggers a read of
`framewalk://recipe/debug-segfault`; a tracepoint workflow triggers
`framewalk://guide/tracepoints`.

## Scheme scripting

The Scheme environment provides three Rust-level primitives and a
prelude of convenience functions.

### Primitives

**`(mi command-string)`** — submit a raw GDB/MI command string.
Returns a lossless result-entry list (or the symbol `running` for
`^running`). Commands are validated against an
allowlist of known-safe MI operations; unrecognised commands are
rejected unless `--allow-shell` is set.

```scheme
(mi "-gdb-version")
(mi "-break-insert main")
```

**`(mi-quote string)`** — apply MI c-string quoting to a parameter.
Returns the string unchanged if it needs no quoting, or wraps it in
a c-string literal with escapes. Used internally by `mi-cmd`.

```scheme
(mi-quote "main")                   ;; => "main"
(mi-quote "/path/with spaces/file") ;; => "\"/path/with spaces/file\""
```

**`(wait-for-stop)`** — block until GDB reports a `*stopped` event
(breakpoint hit, signal, step completed). Returns a hash-map with
the stop reason, thread ID, and raw MI fields.

### Prelude

The prelude defines `mi-cmd` — a variadic wrapper that builds MI
commands with properly quoted parameters — and wraps common operations
so you don't have to remember MI syntax:

```
Safe builder: (mi-cmd operation param ...) — quotes all parameters
Session:      (gdb-version) (load-file path) (attach pid) (detach)
Execution:    (run) (cont) (step) (next) (finish) (interrupt) (until loc)
Breakpoints:  (set-breakpoint loc) (set-temp-breakpoint loc)
              (delete-breakpoint id) (enable-breakpoint id)
              (disable-breakpoint id) (list-breakpoints)
Stack:        (backtrace) (list-locals) (list-arguments)
              (stack-depth) (select-frame n)
Threads:      (list-threads) (select-thread id)
Variables:    (inspect expr)
Helpers:      (step-n n) (next-n n) (run-to loc)
              (result-field name result) (result-fields name result)
```

Use `mi-cmd` when building commands with dynamic arguments (file paths,
expressions) — it handles quoting automatically so paths with spaces
and expressions with quotes work correctly.

### Composition

The power of Scheme mode is composition. Things that would take
dozens of tool calls become a single expression:

**Trace a sort algorithm:**
```scheme
(begin
  (load-file "/tmp/sort")
  (set-breakpoint "do_swap")
  (run)
  (wait-for-stop)
  (define (collect-swaps n)
    (let loop ((i 0) (acc '()))
      (if (>= i n) (reverse acc)
          (let ((state (list (inspect "arr[0]") (inspect "arr[1]")
                             (inspect "arr[2]") (inspect "arr[3]"))))
            (cont) (wait-for-stop)
            (loop (+ i 1) (cons state acc))))))
  (collect-swaps 10))
```

**Walk a linked list in the target's memory:**
```scheme
(define (walk-list ptr max-depth)
  (let loop ((i 0) (cur ptr) (acc '()))
    (if (>= i max-depth) (reverse acc)
        (let ((val (inspect (string-append cur "->value")))
              (nxt (inspect (string-append cur "->next"))))
          (if (equal? (result-field "value" nxt) "0x0")
              (reverse (cons val acc))
              (loop (+ i 1)
                    (string-append "(" cur "->next)")
                    (cons val acc)))))))
```

**Count allocations vs frees to find a leak:**
```scheme
(begin
  (set-breakpoint "pool_alloc")
  (set-breakpoint "pool_free")
  (run)
  (define allocs 0)
  (define frees 0)
  (define (tally n)
    (let loop ((i 0))
      (if (>= i n) (list "allocs" allocs "frees" frees "leaked" (- allocs frees))
          (begin
            (wait-for-stop)
            (let* ((stack (result-field "stack" (backtrace)))
                   (top-frame (car (result-fields "frame" stack)))
                   (func (result-field "func" top-frame)))
              (cond
                ((equal? func "pool_alloc") (set! allocs (+ allocs 1)))
                ((equal? func "pool_free")  (set! frees (+ frees 1)))))
            (cont)
            (loop (+ i 1))))))
  (tally 30))
```

## Architecture

framewalk is a layered workspace of five crates:

**framewalk-mi-wire** — Byte-level framing for the GDB/MI protocol.
Reassembles OS read chunks into complete MI lines. `#![no_std]`, zero
dependencies.

**framewalk-mi-codec** — Hand-written recursive-descent parser for
the GDB/MI v3 grammar. Produces a typed AST (`Record`, `Value`,
`ResultRecord`, `AsyncRecord`, `StreamRecord`). Encodes `MiCommand`
structs back to wire bytes. `#![no_std]`, zero dependencies.

**framewalk-mi-protocol** — Sans-IO state machine. Consumes parsed
records and maintains live state: target execution status, thread
registry, frame registry, breakpoint registry, variable-object
registry, and feature set. No async runtime, no I/O — pure
input/output transformation.

**framewalk-mi-transport** — Async tokio bridge. Spawns the GDB
subprocess, runs reader/writer/stderr-logger tasks, and exposes a
`TransportHandle` with `submit(command) -> outcome` and
`subscribe() -> event stream` APIs.

**framewalk-mcp** — The MCP server binary. Implements the rmcp
`ServerHandler` trait with full/core/scheme exposure profiles, the
Steel Scheme scripting layer, mode selection, and the security guard
for raw MI commands.

### Clean-room discipline

The parser and wire crates are implemented directly from the
[GDB/MI BNF grammar](https://sourceware.org/gdb/current/onlinedocs/gdb.html/GDB_002fMI-Output-Syntax.html),
not by translating existing C or Python implementations. They carry
zero runtime dependencies and are `#![no_std]` compatible.

### Sans-IO design

The protocol crate has no knowledge of tokio, async, threads, or I/O.
It is a pure state machine: feed it bytes, it produces events. Hand
it commands, it produces bytes. This makes it trivially testable
(feed canned GDB responses, assert state) and portable to any
async runtime or embedding context.

## Security

framewalk exposes GDB — which can read/write arbitrary memory, set
hardware breakpoints, and (in some modes) execute shell commands.
The security model restricts what the LLM can reach:

- **Shell escapes blocked by default.** The `mi_raw_command` tool and
  Scheme `(mi ...)` primitive both validate commands against an
  **allowlist** of known-safe MI operations. Some entries are whole
  families (e.g. `-break-*`, `-exec-*`, `-data-*`) and some are exact
  commands only. The canonical generated list is published at
  `framewalk://reference/allowed-mi`. Everything outside that surface —
  including `-interpreter-exec` and `-target-exec-command` — is
  rejected unless `--allow-shell` is explicitly set.

- **Semantic tools are safe by construction.** `set_breakpoint`,
  `backtrace`, `inspect`, etc. go through typed command builders that
  produce only well-formed MI. They cannot be coerced into shell
  escapes.

- **Scheme sandbox.** The Steel engine runs in sandboxed mode: no
  TCP/HTTP networking, restricted filesystem access, no dynamic
  library loading. The `(mi ...)` primitive is the only path to GDB,
  and it goes through the same validation as all other tools.

See [SECURITY.md](SECURITY.md) for the full threat model, sandbox
details, and deployment recommendations.

## Configuration

| Flag | Env var | Default | Description |
|---|---|---|---|
| `--gdb` | `FRAMEWALK_GDB` | `gdb` | Path to GDB binary |
| `--cwd` | | server's cwd | Working directory for GDB |
| `--mode` | `FRAMEWALK_MODE` | `full` | `full`, `core`, or `scheme` (`standard` is accepted as an alias for `full`) |
| `--non-stop` / `--no-non-stop` | `FRAMEWALK_NON_STOP` | `true` | Enable GDB non-stop mode; disable it for all-stop-only remote stubs such as QEMU's gdbstub |
| `--allow-shell` | `FRAMEWALK_ALLOW_SHELL` | `false` | Permit shell-adjacent MI commands |
| `--log` | `FRAMEWALK_LOG` | `framewalk=info,rmcp=warn` | Tracing filter |
| `--scheme-eval-timeout-secs` | `FRAMEWALK_SCHEME_EVAL_TIMEOUT_SECS` | `60` | Default timeout for one `scheme_eval` call |
| `--wait-for-stop-timeout-secs` | `FRAMEWALK_WAIT_FOR_STOP_TIMEOUT_SECS` | `30` | Default timeout for Scheme wait helpers such as `wait-for-stop` |

## Building and testing

```sh
# Run the full validation path (workspace tests + ignored GDB-backed suites)
nix run .#validate

# Enter the dev shell (provides Rust toolchain, GDB, and test tools)
nix develop

# Build
cargo build -p framewalk-mcp

# Run the same validation path from inside the dev shell
./scripts/validate.sh

# Clippy + format
cargo clippy --workspace -- -D warnings
cargo fmt --all -- --check

# Build the static musl binary
nix build
```

### Integration tests

The `framewalk-mcp` crate ships four end-to-end integration test
files. Each spawns the compiled `framewalk-mcp` binary as a
subprocess and drives it with real JSON-RPC over stdio, so they all
need a real `gdb` on `PATH` and are gated `#[ignore]` to keep
`cargo test` fast for contributors without the dev shell.

| File | What it covers |
|---|---|
| `tests/stdio_roundtrip.rs` | `initialize` handshake, full `tools/list` against the expected catalog, a real `tools/call gdb_version`, raw-MI shell-guard rejection. |
| `tests/resources_roundtrip.rs` | Resources capability advertised on `initialize`, `resources/list` returns 33 markdown items split 12/16/5 across guides/references/recipes, byte-for-byte body match for `framewalk://guide/getting-started` against `docs/getting-started.md`, recipe and reference reads, unknown-URI error, and an exhaustive "every listed URI is readable" check. |
| `tests/scheme_integration.rs` | Steel Scheme engine integration through `scheme_eval`. |
| `tests/scheme_mcp_roundtrip.rs` | Full MCP-protocol coverage for `scheme_eval` across `--mode full`, `--mode core`, and `--mode scheme`. |

Run them all through the validation entrypoint:

```sh
nix run .#validate
```

Or run a single suite:

```sh
# Just the resources tests
nix develop --command cargo test -p framewalk-mcp \
    --test resources_roundtrip -- --ignored

# Just the tools/initialize roundtrip
nix develop --command cargo test -p framewalk-mcp \
    --test stdio_roundtrip -- --ignored

# A single named test (substring match)
nix develop --command cargo test -p framewalk-mcp \
    --test resources_roundtrip every_listed -- --ignored
```

`cargo nextest` works the same way and parallelises better:

```sh
nix develop --command cargo nextest run -p framewalk-mcp \
    --run-ignored all -j 4
```

## Documentation

- [Getting Started](docs/getting-started.md) — install, configure, first session
- [Scheme Reference](docs/scheme-reference.md) — all Scheme functions and patterns
- [Modes](docs/modes.md) — full vs core vs scheme mode in depth
- [Security](SECURITY.md) — threat model, sandbox, deployment guidance
- [Examples](examples/) — test programs and Scheme scripts

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE) for details.
