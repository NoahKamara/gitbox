{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.gitbox;
  format = pkgs.formats.toml { };
in
{
  options.programs.gitbox = {
    enable = lib.mkEnableOption "Gitbox client";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./package.nix { }";
      description = "Package containing the gitbox client.";
    };
    settings = lib.mkOption {
      type = lib.types.submodule {
        freeformType = format.type;
        options.default_server = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "home-gitbox";
          description = "Default SSH host or alias. Null leaves server selection to --server.";
        };
      };
      default = { };
      description = "Settings written to the client TOML configuration.";
    };
  };
  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
    xdg.configFile."gitbox/config.toml".source = format.generate "gitbox-client.toml" (
      lib.filterAttrs (_: value: value != null) cfg.settings
    );
  };
}
