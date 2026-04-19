{
  description = "framewalk — clean-room Rust GDB/MI v3 library + MCP server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = inputs @ { self, flake-parts, ... }:
    let
      downstreamFlakeModule = import ./nix/modules/downstream-flake-module.nix self;
    in
    flake-parts.lib.mkFlake { inherit inputs; } {
      imports = [
        inputs.flake-parts.flakeModules.flakeModules
        ./nix/flake-module.nix
      ];

      flake.flakeModules.default = downstreamFlakeModule;
    };
}
