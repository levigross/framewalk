upstreamFlake:
{ flake-parts-lib, lib, ... }:
let
  inherit (flake-parts-lib) mkPerSystemOption;
in
{
  options.perSystem = mkPerSystemOption (
    { config, system, ... }:
    let
      cfg = config.framewalk;
      supportedSystems = builtins.attrNames upstreamFlake.packages;
      systemSupported = builtins.hasAttr system upstreamFlake.packages;
    in
    {
      options.framewalk = {
        enable = lib.mkEnableOption "expose framewalk packages in the downstream flake";

        packageName = lib.mkOption {
          type = lib.types.str;
          default = "framewalk-mcp";
          description = "Name to use when exposing the wrapped framewalk MCP package in `packages`.";
        };

        staticPackageName = lib.mkOption {
          type = lib.types.str;
          default = "framewalk-mcp-static";
          description = "Name to use when exposing the static framewalk MCP package in `packages`.";
        };

        package = lib.mkOption {
          type = lib.types.package;
          readOnly = true;
          description = "The wrapped `framewalk-mcp` package for the current system.";
        };

        staticPackage = lib.mkOption {
          type = lib.types.package;
          readOnly = true;
          description = "The static `framewalk-mcp` package for the current system.";
        };
      };

      config = lib.mkIf cfg.enable (
        if systemSupported then
          {
            framewalk.package = upstreamFlake.packages.${system}.framewalk-mcp;
            framewalk.staticPackage = upstreamFlake.packages.${system}.framewalk-mcp-static;

            packages.${cfg.packageName} = cfg.package;
            packages.${cfg.staticPackageName} = cfg.staticPackage;
          }
        else
          {
            assertions = [
              {
                assertion = false;
                message = ''
                  framewalk flake module does not support system `${system}`.
                  Supported systems: ${lib.concatStringsSep ", " supportedSystems}
                '';
              }
            ];
          }
      );
    }
  );
}
