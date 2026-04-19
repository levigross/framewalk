{ mkRustPackage, muslTarget, pkgs, ... }:
let
  framewalk = mkRustPackage {
    doCheck = false;
  };

  validate = pkgs.writeShellApplication {
    name = "framewalk-validate";
    runtimeInputs = [ pkgs.nix ];
    text = ''
      set -euo pipefail

      search_dir="$PWD"
      repo_root=""
      while [[ "$search_dir" != "/" ]]; do
        if [[ -f "$search_dir/flake.nix" && -x "$search_dir/scripts/validate.sh" ]]; then
          repo_root="$search_dir"
          break
        fi
        search_dir="$(dirname "$search_dir")"
      done

      if [[ -z "$repo_root" ]]; then
        echo "framewalk-validate: could not find a framewalk checkout from $PWD" >&2
        echo "run \`nix run .#validate\` from the repository root (or a subdirectory of it)" >&2
        exit 1
      fi

      cd "$repo_root"
      exec ./scripts/validate.sh "$@"
    '';
  };

  framewalk-mcp-static = mkRustPackage {
    pname = "framewalk-mcp-static";

    nativeBuildInputs = [ pkgs.cargo-nextest ];

    buildPhase = ''
      runHook preBuild
      export CARGO_PROFILE_RELEASE_STRIP=false
      cargo build \
        --release \
        --target ${muslTarget} \
        --offline \
        -j $NIX_BUILD_CORES
      runHook postBuild
    '';

    checkPhase = ''
      cargo nextest run \
        --target ${muslTarget} \
        --offline \
        --no-tests=pass
    '';

    installPhase = ''
      runHook preInstall
      mkdir -p $out/bin
      cp target/${muslTarget}/release/framewalk-mcp $out/bin/
      runHook postInstall
    '';
  };

  # Batteries-included variant: the static binary with FRAMEWALK_GDB pinned
  # to a nixpkgs gdb so `nix shell .#framewalk-mcp` gives a working MCP
  # server without requiring gdb on the ambient PATH. `--set-default`
  # (not `--set`) means users can still override via env.
  framewalk-mcp = pkgs.symlinkJoin {
    name = "framewalk-mcp-${framewalk-mcp-static.version}";
    paths = [ framewalk-mcp-static ];
    nativeBuildInputs = [ pkgs.makeWrapper ];
    postBuild = ''
      wrapProgram $out/bin/framewalk-mcp \
        --set-default FRAMEWALK_GDB ${pkgs.gdb}/bin/gdb
    '';
  };
in
{
  default = framewalk-mcp;
  inherit framewalk framewalk-mcp framewalk-mcp-static validate;
}
