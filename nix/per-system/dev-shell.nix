{ packages, pkgs, rustToolchain, ... }:
pkgs.mkShell {
  inputsFrom = [ packages.framewalk ];

  nativeBuildInputs = [
    rustToolchain
    pkgs.cargo-nextest
    pkgs.cargo-deny
    pkgs.cargo-audit
    pkgs.cargo-llvm-cov
    pkgs.gdb
    pkgs.jq
    pkgs.socat
    pkgs.qemu
  ];

  RUST_BACKTRACE = "1";
}
