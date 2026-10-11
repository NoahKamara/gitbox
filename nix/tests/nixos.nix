{ pkgs, gitbox }:
let
  keys = import (pkgs.path + "/nixos/tests/ssh-keys.nix") pkgs;
in
pkgs.testers.runNixOSTest {
  name = "gitbox";
  nodes.server = { lib, ... }: {
    imports = [ ../nixos.nix ];
    services.gitbox = {
      enable = true;
      package = gitbox;
      user = "repositories";
      group = "repositories";
      dataDir = "/srv/gitbox";
      admin = {
        name = "alice";
        sshKey = keys.snakeOilPublicKey;
      };
    };
    specialisation.rotated.configuration.services.gitbox.admin.sshKey =
      lib.mkForce keys.snakeOilEd25519PublicKey;
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
    client.succeed("install -m600 ${keys.snakeOilPrivateKey} /root/key")
    client.succeed("gitbox server set-default box")
    client.succeed("gitbox repo create projects/example")
    assert client.succeed("gitbox repo list").strip() == "projects/example"
    client.fail("ssh box id")
    client.succeed("git init -b main /tmp/work; cd /tmp/work; git config user.name Test; git config user.email test@example.invalid; echo hello > hello; git add hello; git commit -m initial; git push box:projects/example.git main")
    client.succeed("git clone box:projects/example.git /tmp/clone; test -f /tmp/clone/hello")
    server.succeed("test ! -e /srv/gitbox/config.toml")
    server.succeed("/run/current-system/specialisation/rotated/bin/switch-to-configuration test")
    server.wait_for_unit("sshd.service")
    client.fail("gitbox repo list")
    client.succeed("install -m600 ${keys.snakeOilEd25519PrivateKey} /root/key")
    assert client.succeed("gitbox repo list").strip() == "projects/example"
    client.succeed("git -C /tmp/clone fetch origin")
    client.fail("ssh box id")
  '';
}
