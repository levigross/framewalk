# Repository Guidelines

Operational guide for agents and contributors working in `framewalk`. Read this before changing code. If something here conflicts with what you observe, trust the code and update this file.

## Mindset

**Always reason from first principles.** Break every problem down to its fundamental truths — the MI spec, the observed GDB behavior, the type system, the invariants in this file — and build the reasoning up from there. Do not pattern-match to "the way it's usually done," do not default to conventional wisdom, and do not carry over assumptions from other debuggers or MCP servers without checking them against the facts in this codebase.

Show the reasoning chain. Make each step challengeable. When a convention here, a lint rule, or a common idiom conflicts with what first-principles reasoning suggests, flag the tension explicitly and ask rather than quietly defer — the tiebreaker is evidence from the spec and the code, not habit.

## Project layout

Virtual Rust workspace. Five crates under `crates/`, layered from the
sans-IO core outward:

| Crate | Role | Allowed runtime deps |
| --- | --- | --- |
| `framewalk-mi-wire` | Byte framer — turns stream bytes into MI records | **none** |
| `framewalk-mi-codec` | Parser/encoder for MI records | **none** |
| `framewalk-mi-protocol` | State machine, token correlation, event fan-out | `thiserror`, `tracing` |
| `framewalk-mi-transport` | `tokio` subprocess pump (owns the GDB process) | `tokio`, `parking_lot`, `thiserror`, `tracing` |
| `framewalk-mcp` | MCP server binary, tool surface, Steel Scheme runtime, embedded resources | `rmcp`, `serde`, `schemars`, `clap`, `anyhow`, `steel-core`, `tracing-subscriber`, plus the framewalk crates |

Everything else:

- `crates/*/tests/` — integration tests. Unit tests live inline in `#[cfg(test)]` modules.
- `crates/framewalk-mcp/src/resources/` — 28 markdown files (guides, references, recipes) embedded into the binary and served over MCP `resources/list`/`resources/read`.
- `docs/` — user-facing docs: `getting-started.md`, `modes.md`, `scheme-reference.md`, `no-source.md`.
- `examples/` — demo debug targets (C, C++, Rust) plus Scheme sessions.
- `nix/` — `flake-module.nix`, `per-system/*.nix`, `modules/downstream-flake-module.nix` (flake-parts module for consumers).
- `scripts/` — `validate.sh` (full validation flow), `syzbot-fetch` (fetch syzbot repros).
- `rust-toolchain.toml` pins the stable channel with `musl` targets; the nix dev shell is authoritative.

## Toolchain & commands

**Always run Rust/cargo commands through `nix develop -c`.** The flake pins the exact toolchain, targets (including `x86_64-unknown-linux-musl`), and `gdb`. Using the ambient toolchain silently diverges from CI.

Rust edition 2024, MSRV 1.85, resolver v3. Apache-2.0 licensed.

Core commands:

```sh
nix develop -c cargo build --workspace
nix develop -c cargo test --workspace                       # unit + non-ignored integration
nix develop -c cargo test --workspace --locked              # what CI runs
nix develop -c cargo clippy --workspace --all-targets -- -D warnings
nix develop -c cargo fmt --all -- --check
nix develop -c cargo deny check                             # license + advisory policy
./scripts/validate.sh                                        # full suite incl. ignored GDB/MCP tests
```

`validate.sh` self-enters the nix shell via a sentinel env var; running it outside `nix develop` still works.

CI (`.github/workflows/pr-unit-tests.yml`) runs `nix develop -c cargo test --workspace --locked` on PRs and pushes to `main`. The `--ignored` suites below are developer-local.

## Architecture invariants

These are expensive to get wrong. Respect them before adding deps or moving code.

1. **Layering is strict.** `mi-wire` → `mi-codec` → `mi-protocol` are sans-IO. Only `mi-transport` and `mcp` may do I/O. Do not pull `tokio` into protocol or below.
2. **`mi-wire` and `mi-codec` have zero runtime deps.** Hand-write errors. Do not reach for `thiserror` — that constraint is what keeps downstream consumers' dep graphs small and makes property-testing feasible.
3. **`unsafe_code = "deny"` at workspace scope.** If a specific site genuinely needs `unsafe`, allow it per-crate/per-block with a justifying comment.
4. **MI-first API.** Tool surfaces mirror GDB/MI semantics; do not invent convenience wrappers that diverge from the MI command names or result shapes.
5. **Clean-room discipline.** Do not copy from GDB, LLDB, or other MI implementations. Derive from the MI spec and testable GDB output.
6. **No unnecessary flake deps.** Nix packaging uses nixpkgs `buildRustPackage`, not crane. Do not add flake inputs casually.

## Code style

Rust 2024 with default `rustfmt`. Naming: `snake_case` / `CamelCase` / `SCREAMING_SNAKE_CASE`.

Workspace lint policy (see `Cargo.toml`): `clippy::all = deny`, `clippy::pedantic = warn`, with four noise lints allowed (`module_name_repetitions`, `missing_errors_doc`, `missing_panics_doc`, `must_use_candidate`). Treat clippy warnings as real work.

Idioms to respect:

- **No `.unwrap()` in production code.** Use `?`, `.ok_or(...)`, or `.expect("invariant: ...")` where the invariant is spelled out.
- **No `let _ = result`.** Use `.ok()`, `if let Err(e) = ...`, or `drop(...)`.
- **No `Ok(Ok(x))` stutter.** Unwrap the transport/control layer and the domain layer with two sequential `let Ok(x) = ... else` statements.
- Comments explain WHY: protocol invariants, state-machine assumptions, security boundaries. Do not narrate obvious control flow.

## Testing

- **Unit tests** inline in `#[cfg(test)]` modules next to the code they exercise.
- **Integration tests** in `crates/*/tests/`. Name by behavior, e.g. `scheme_mode_only_has_scheme_eval`.
- **Property tests** via `proptest` in the sans-IO layers (`mi-wire`, `mi-codec`) — cover arbitrary byte sequences and chunk splits.
- **GDB-/MCP-backed tests** are `#[ignore]` by default because they spawn real `gdb` or a full MCP server. Run them via `./scripts/validate.sh` before submitting transport, routing, resources, or Scheme changes.
- **Async tests** use `#[tokio::test]`.
- `Cargo.lock` is committed; CI uses `--locked`. Do not let it drift without intent.

## Docs & MCP resources

Two doc surfaces serve different audiences:

1. **`docs/` and `README.md`** — human-facing install, quickstart, modes, Scheme reference.
2. **`crates/framewalk-mcp/src/resources/*.md`** — agent-facing. Embedded into the binary via `resources.rs` and served over MCP. Split into:
   - `guide-*.md` — topic overviews (execution model, inspection, raw MI, etc.)
   - `reference-*.md` — per-category tool reference (one file per tool family)
   - `recipe-*.md` — end-to-end workflows

When you change the tool surface:

- Update the matching `reference-*.md` — tool names, parameters, result shapes.
- Update affected `guide-*.md` or `recipe-*.md` if the workflow they describe changes.
- Update `docs/scheme-reference.md` when Scheme bindings or prelude change.
- Update `README.md` tool counts and mode descriptions if those shift.

The `resources_roundtrip` integration test verifies that every advertised resource is readable. Run it after adding or renaming resources.

## Commits & PRs

Conventional commits: `feat:`, `fix:`, `docs:`, `chore:`, `ci:`, `refactor:`. Short imperative subject (≤70 chars); brief body if the "why" isn't obvious.

- **One logical change per commit.**
- **Never add AI/model attribution** (no `Co-Authored-By: Claude`, no "Generated by ..." footers). Commits must be indistinguishable from human-authored ones.
- Pre-commit hooks are not configured; run `cargo fmt`, `cargo clippy`, and the relevant test subset before pushing.
- PRs: describe the behavioral change, list validation commands run, and include sample MCP payloads or resource URIs when tool surfaces or docs change.

## Security boundaries

See `SECURITY.md` for the full threat model. Key guards:

- **`--allow-shell` / `FRAMEWALK_ALLOW_SHELL`** — off by default. Gates GDB shell-escape vectors (`-interpreter-exec console "shell ..."`, `!...`, `target-exec-command`). Leave off unless testing in a contained environment.
- **`raw_guard.rs`** enforces the raw-MI allowlist used by `mi_raw_command`. Any new raw passthrough must be in the allowlist and documented in `reference-raw.md`.
- **Scheme sandbox** — `scheme/bindings.rs` is the sole bridge between Steel and the framewalk runtime. Do not expose filesystem, process, or network primitives to the Scheme side without a review of the threat model.
- **Mode gates** — `full`, `core`, `scheme`. Tool registration in `tools/*.rs` declares `profiles: FULL_ONLY` or `FULL_CORE`. Adding a tool to core mode is a product decision; default to `FULL_ONLY`.

## Nix packaging

- `nix build` / `nix build .#framewalk-mcp` — wrapped binary (`FRAMEWALK_GDB` set to nixpkgs `gdb`).
- `nix build .#framewalk-mcp-static` — static musl binary.
- `nix flake check` — runs per-system checks (`nix/per-system/checks.nix`).
- `nix develop` — dev shell with pinned toolchain, `gdb`, and tooling.
- `nix/modules/downstream-flake-module.nix` — flake-parts module published as `flakeModules.default` for downstream consumers; keep option names stable across releases.

## When in doubt

- If the task is non-trivial, write a short plan first and confirm scope before coding.
- If architecture invariants (layering, zero-dep cores, clean-room) might be at risk, flag it and ask rather than guess.
- If a change affects the tool surface, resource catalog, or security boundaries, update the docs in the same commit as the code.
