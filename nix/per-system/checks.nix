{ advisoryDb, mkRustPackage, packages, pkgs, rustToolchain, ... }:
{
  inherit (packages) framewalk;

  framewalk-static-binary = pkgs.runCommand "framewalk-static-binary-check" {
    nativeBuildInputs = [ pkgs.file ];
  } ''
    output=$(file ${packages.framewalk-mcp-static}/bin/framewalk-mcp)
    echo "$output"
    echo "$output" | grep -q "statically linked\|static-pie linked"
    mkdir -p $out
  '';

  framewalk-nextest = mkRustPackage {
    pname = "framewalk-nextest";
    nativeBuildInputs = [ rustToolchain pkgs.cargo-nextest ];
    checkPhase = ''
      cargo nextest run --no-tests=pass
    '';
  };

  framewalk-clippy = mkRustPackage {
    pname = "framewalk-clippy";
    nativeBuildInputs = [ rustToolchain pkgs.clippy ];
    checkPhase = ''
      cargo clippy --workspace --all-targets -- -D warnings
    '';
  };

  framewalk-fmt = mkRustPackage {
    pname = "framewalk-fmt";
    nativeBuildInputs = [ rustToolchain pkgs.rustfmt ];
    buildPhase = "true";
    checkPhase = ''
      cargo fmt --all --check
    '';
    installPhase = "mkdir -p $out";
  };

  framewalk-deny = mkRustPackage {
    pname = "framewalk-deny";
    nativeBuildInputs = [ rustToolchain pkgs.cargo-deny pkgs.git ];
    buildPhase = "true";
    checkPhase = ''
      export HOME="$TMPDIR"
      export CARGO_HOME="$TMPDIR/cargo"
      mkdir -p "$CARGO_HOME/advisory-dbs"
      db="$CARGO_HOME/advisory-dbs/advisory-db-3157b0e258782691"
      cp -R --no-preserve=mode,ownership ${advisoryDb} \
        "$db"
      chmod -R u+w "$db"
      git -C "$db" init -q
      git -C "$db" add .
      git -C "$db" \
        -c user.name=framewalk \
        -c user.email=framewalk@example.invalid \
        commit -q -m advisory-db
      cargo deny check --disable-fetch
    '';
    installPhase = "mkdir -p $out";
  };

  # Coverage runs an instrumented workspace test build via cargo-llvm-cov.
  # No threshold is enforced yet: the check only asserts that the coverage
  # toolchain is wired up and produces a summary. Local devs can run
  # `cargo llvm-cov --workspace --html` from the dev shell for a browsable
  # report.
  framewalk-coverage = mkRustPackage {
    pname = "framewalk-coverage";
    nativeBuildInputs = [ rustToolchain pkgs.cargo-llvm-cov ];
    buildPhase = "true";
    checkPhase = ''
      cargo llvm-cov --workspace --summary-only
    '';
    installPhase = "mkdir -p $out";
  };
}
