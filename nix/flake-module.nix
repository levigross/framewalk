{ inputs, ... }:
let
  mkPerSystemContext = import ./per-system/context.nix;
  mkPackages = import ./per-system/packages.nix;
  mkKernelDebug = import ./per-system/kernel-debug.nix;
  mkChecks = import ./per-system/checks.nix;
  mkDevShell = import ./per-system/dev-shell.nix;
in
{
  systems = [ "x86_64-linux" "aarch64-linux" ];

  perSystem = { system, ... }:
    let
      ctx = mkPerSystemContext { inherit inputs system; };
      packages = mkPackages ctx;
      kernelDebug = mkKernelDebug ctx;
    in
    {
      packages = packages // kernelDebug;
      apps.validate = {
        type = "app";
        program = "${packages.validate}/bin/framewalk-validate";
      };

      devShells.default = mkDevShell (ctx // { inherit packages; });
      checks = mkChecks (ctx // { inherit packages; });
    };
}
