{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.services.gitbox;
  format = pkgs.formats.toml { };
  serverConfig = format.generate "gitbox-server.toml" {
    version = 1;
    admin = {
      name = cfg.admin.name;
      ssh_key = cfg.admin.sshKey;
    };
  };
  publicKey = pkgs.writeText "gitbox-admin.pub" (cfg.admin.sshKey + "\n");
  authorizedKeys =
    pkgs.runCommand "gitbox-authorized-keys" { nativeBuildInputs = [ pkgs.openssh ]; }
      ''
        ssh-keygen -lf ${publicKey} >/dev/null
        printf 'restrict ' > "$out"
        cat ${publicKey} >> "$out"
      '';
  forcedCommand = pkgs.writeShellScript "gitbox-ssh" ''
    exec ${cfg.package}/bin/gitboxd ssh \
      --data-dir ${lib.escapeShellArg cfg.dataDir} \
      --config ${serverConfig} --key-id admin
  '';
in
{
  options.services.gitbox = {
    enable = lib.mkEnableOption "Gitbox SSH repository hosting";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./package.nix { };
      defaultText = lib.literalExpression "pkgs.callPackage ./package.nix { }";
      description = "Package containing gitbox and gitboxd.";
    };
    dataDir = lib.mkOption {
      type = lib.types.strMatching "/([a-zA-Z0-9_-]+/)*[a-zA-Z0-9_-]+";
      default = "/var/lib/gitbox";
      description = "Persistent data directory. Repositories live in its repos subdirectory. Components may contain letters, digits, underscores and hyphens.";
    };
    user = lib.mkOption {
      type = lib.types.strMatching "[a-z_][a-z0-9_-]*";
      default = "gitbox";
      description = "Dedicated SSH account created for Gitbox.";
    };
    group = lib.mkOption {
      type = lib.types.strMatching "[a-z_][a-z0-9_-]*";
      default = "gitbox";
      description = "Group owning Gitbox repositories.";
    };
    admin.name = lib.mkOption {
      type = lib.types.strMatching "[a-zA-Z0-9_-]+";
      default = "admin";
      description = "Gitbox administrator name.";
    };
    admin.sshKey = lib.mkOption {
      type = lib.types.strMatching "(ssh-|ecdsa-|sk-)[^\n\r]+";
      example = "ssh-ed25519 AAAA... alice@laptop";
      description = "Administrator public key, without authorized_keys options. This public key is stored in the Nix store.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.user != "root" && cfg.group != "root";
        message = "Gitbox requires a dedicated non-root account and group.";
      }
    ];
    environment.systemPackages = [ cfg.package ];
    users.groups.${cfg.group} = { };
    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      shell = pkgs.bashInteractive;
    };
    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 0700 ${cfg.user} ${cfg.group} - -"
      "d ${cfg.dataDir}/repos 0700 ${cfg.user} ${cfg.group} - -"
    ];
    services.openssh = {
      enable = true;
      extraConfig = ''
        Match User ${cfg.user}
          AuthorizedKeysFile ${authorizedKeys}
          AuthorizedKeysCommand none
          TrustedUserCAKeys none
          AuthenticationMethods publickey
          PubkeyAuthentication yes
          PasswordAuthentication no
          KbdInteractiveAuthentication no
          ForceCommand ${forcedCommand}
          DisableForwarding yes
          PermitTTY no
          PermitTunnel no
          PermitUserRC no
        Match all
      '';
    };
  };
}
