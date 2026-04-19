{ pkgs, src }:
pkgs.lib.cleanSourceWith {
  inherit src;

  filter = path: type:
    let
      base = baseNameOf path;
      ext = pkgs.lib.last (pkgs.lib.splitString "." base);
    in
    (type == "directory")
    || ext == "rs"
    || ext == "toml"
    || ext == "lock"
    || ext == "md"
    || ext == "mi"
    || ext == "scm"
    || ext == "sh"
    || base == "deny.toml";
}
