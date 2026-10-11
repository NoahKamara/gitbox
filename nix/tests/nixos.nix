{ pkgs, gitbox }:
let
  keys = import (pkgs.path + "/nixos/tests/ssh-keys.nix") pkgs;
in
pkgs.testers.runNixOSTest {
  name = "gitbox";
  nodes.server = { lib, ... }: {
    imports = [ ../nixos.nix ];
    virtualisation.writableStore = true;
    boot.nixStoreMountOpts = [
      "nodev"
      "nosuid"
    ];
    services.gitbox = {
      enable = true;
      package = gitbox;
      user = "repositories";
      group = "repositories";
      dataDir = "/srv/gitbox";
      admin = {
        name = "alice";
        sshKeys = [
          keys.snakeOilPublicKey
          keys.snakeOilEd25519PublicKey
        ];
      };
    };
    specialisation.rotated.configuration.services.gitbox.admin.sshKeys = lib.mkForce [
      keys.snakeOilEd25519PublicKey
    ];
  };
  nodes.client = {
    environment.systemPackages = [
      gitbox
      pkgs.git
    ];
    programs.ssh.extraConfig = ''
      Host box
        HostName server
        User repositories
        IdentityFile /root/key
        IdentitiesOnly yes
        StrictHostKeyChecking accept-new
        BatchMode yes
    '';
  };
  testScript = ''
    start_all()
    server.wait_for_unit("sshd.service")
    # Match multi-user stores where the nixbld group can write.
    server.succeed("chmod g+w /nix/store")
    server.succeed("test ! -L /etc/ssh/gitbox-authorized_keys && test $(stat -c %a:%u:%g /etc/ssh/gitbox-authorized_keys) = 444:0:0")
    client.succeed("gitbox server set-default box")
    client.succeed("ssh-keygen -q -t ed25519 -N \"\" -f /root/unrelated; install -m600 /root/unrelated /root/key")
    assert "Permission denied (publickey)" in client.fail("gitbox repo list 2>&1")
    client.succeed("install -m600 ${keys.snakeOilPrivateKey} /root/key")
    client.succeed("gitbox repo create projects/example")
    client.succeed("git init -b main /tmp/work; cd /tmp/work; git config user.name Test; git config user.email test@example.invalid; echo hello > hello; git add hello")
    for index, key in enumerate(["${keys.snakeOilPrivateKey}", "${keys.snakeOilEd25519PrivateKey}"]):
        client.succeed(f"install -m600 {key} /root/key")
        client.succeed("gitbox folder create projects/archive")
        client.succeed(f"gitbox repo create projects/extra-{index}")
        assert f"projects/extra-{index}" in client.succeed("gitbox repo list").splitlines()
        client.fail("ssh box id")
        client.succeed(f"git -C /tmp/work commit --allow-empty -m key-{index}; git -C /tmp/work push box:projects/example.git main")
        client.succeed(f"git clone box:projects/example.git /tmp/clone-{index}; test -f /tmp/clone-{index}/hello")
    server.succeed("test ! -e /srv/gitbox/config.toml")
    server.succeed("/run/current-system/specialisation/rotated/bin/switch-to-configuration test")
    server.wait_for_unit("sshd.service")
    client.succeed("install -m600 ${keys.snakeOilPrivateKey} /root/key")
    assert "Permission denied (publickey)" in client.fail("gitbox repo list 2>&1")
    client.succeed("install -m600 ${keys.snakeOilEd25519PrivateKey} /root/key")
    assert client.succeed("gitbox repo list").splitlines() == ["projects/example", "projects/extra-0", "projects/extra-1"]
    client.succeed("git -C /tmp/clone-1 fetch origin")
    client.fail("ssh box id")
  '';
}
