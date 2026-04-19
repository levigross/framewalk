{ inputs, system }:
let
  repoRoot = ../..;

  pkgs = import inputs.nixpkgs {
    inherit system;
    overlays = [ (import inputs.rust-overlay) ];
  };

  rustToolchain =
    pkgs.rust-bin.fromRustupToolchainFile (repoRoot + "/rust-toolchain.toml");

  rustPlatform = pkgs.makeRustPlatform {
    cargo = rustToolchain;
    rustc = rustToolchain;
  };

  muslTarget = {
    "x86_64-linux" = "x86_64-unknown-linux-musl";
    "aarch64-linux" = "aarch64-unknown-linux-musl";
  }.${system};

  src = import ../lib/source-filter.nix {
    inherit pkgs;
    src = repoRoot;
  };

  commonArgs = {
    inherit src;
    pname = "framewalk";
    version = "0.1.0";
    cargoLock.lockFile = repoRoot + "/Cargo.lock";
  };

  mkRustPackage = args:
    rustPlatform.buildRustPackage (commonArgs // args);
in
{
  inherit commonArgs mkRustPackage muslTarget pkgs rustPlatform rustToolchain src;
}
