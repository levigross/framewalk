#!/usr/bin/env bash
set -euo pipefail

script_dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
repo_root="$(dirname -- "$script_dir")"
cd "$repo_root"

# Do not trust ambient `IN_NIX_SHELL`: this process can inherit it from
# some unrelated parent shell that does not have framewalk's dev-shell
# PATH (notably `gdb`) active. Instead, use a script-owned sentinel so
# direct `./scripts/validate.sh` always re-enters the correct dev shell
# exactly once.
if [[ "${1:-}" == "--inside-nix" ]]; then
  shift
elif [[ "${FRAMEWALK_VALIDATE_IN_NIX:-}" != "1" ]]; then
  exec env FRAMEWALK_VALIDATE_IN_NIX=1 nix develop -c "$0" --inside-nix "$@"
fi

echo "==> cargo test --workspace"
cargo test --workspace "$@"

echo "==> cargo test -p framewalk-mi-transport --test gdb_conformance -- --ignored"
cargo test -p framewalk-mi-transport --test gdb_conformance -- --ignored

echo "==> cargo test -p framewalk-mi-transport --test handle_lifecycle -- --ignored"
cargo test -p framewalk-mi-transport --test handle_lifecycle -- --ignored

echo "==> cargo test -p framewalk-mcp --test stdio_roundtrip -- --ignored"
cargo test -p framewalk-mcp --test stdio_roundtrip -- --ignored

echo "==> cargo test -p framewalk-mcp --test resources_roundtrip -- --ignored"
cargo test -p framewalk-mcp --test resources_roundtrip -- --ignored

echo "==> cargo test -p framewalk-mcp --test scheme_mcp_roundtrip -- --ignored"
cargo test -p framewalk-mcp --test scheme_mcp_roundtrip -- --ignored

echo "==> cargo test -p framewalk-mcp --test scheme_integration -- --ignored"
cargo test -p framewalk-mcp --test scheme_integration -- --ignored
