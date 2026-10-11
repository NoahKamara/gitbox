# Gitbox

Gitbox hosts bare Git repositories behind a restricted OpenSSH account. The
current slice has one administrator. The administrator can create folders and
repositories with the `gitbox` client; ordinary Git handles push, clone, and
fetch.

## Nix

The flake exports `packages.<system>.gitbox` and `default`,
`nixosModules.default`, `homeManagerModules.default`, and `overlays.default`.
The package includes both binaries and their Git/OpenSSH runtime dependencies.

```sh
nix build .#gitbox
nix run . -- --help
```

Add Gitbox as an input in `~/.nix-config/flake.nix`, using your Git remote:

```nix
inputs.gitbox.url = "git+ssh://noah@chimaera/~noah/gitbox?ref=main";
inputs.gitbox.inputs.nixpkgs.follows = "nixpkgs";
```

### NixOS server

Import the module in the server's configuration:

```nix
{ inputs, lib, ... }:
{
  imports = [ inputs.gitbox.nixosModules.default ];

  services.gitbox = {
    enable = true;
    dataDir = "/var/lib/gitbox";
    user = "gitbox";
    group = "gitbox";
    admin = {
      name = "noah";
      sshKeys = [
        "ssh-ed25519 AAAA... noah@laptop"
        "ssh-ed25519 AAAA... noah@desktop"
        # Or: (lib.trim (builtins.readFile ./keys/noah.pub))
      ];
    };
  };
}
```

The module creates the account and `dataDir/repos`, enables the system OpenSSH
server, and restricts the Gitbox account to Gitbox commands and its declared
admin keys. It disables forwarding, interactive sessions, and user SSH startup
scripts for that account. Configure ports, listen addresses, firewall behavior,
and any host-wide SSH allowlists through `services.openssh`. Include the Gitbox
account if you use `AllowUsers` or `AllowGroups`.

Server configuration and the public keys live in the Nix store. The module
installs a root-owned copy of the authorized keys at `/etc/ssh/gitbox-authorized_keys`
so OpenSSH's permission checks do not traverse the group-writable Nix store.
Rebuilds apply key changes and select the current package; no `gitboxd init`
step is needed.
Repositories persist under `dataDir/repos`. Changing `dataDir`, `user`, or `group`
does not migrate existing repositories or recursively change their ownership.
Declare at least one key. Every key grants the same administrator access.
Each list entry must contain one public key, optionally followed by a comment,
without line breaks or `authorized_keys` options. Removing a key and rebuilding
revokes its access for new SSH connections.

Replace the former `admin.sshKey` option with `admin.sshKeys = [ ... ];` when
upgrading. Nix generates the current runtime configuration automatically.

Replace any older local Gitbox module and `noah.services.gitbox` configuration
with this import and `services.gitbox`. The older `gitbox --root ... ssh`
interface is no longer supported. Move existing bare repositories into
`dataDir/repos` before switching if they currently use a different layout.

### Home Manager client

Import the client module in your Home Manager configuration, on Linux or macOS:

```nix
{ inputs, ... }:
{
  imports = [ inputs.gitbox.homeManagerModules.default ];

  programs.gitbox = {
    enable = true;
    settings.default_server = "home-gitbox";
  };

  programs.ssh = {
    enable = true;
    matchBlocks.home-gitbox = {
      hostname = "chimaera.noahkamara.com";
      user = "gitbox";
      identityFile = "~/.ssh/id_ed25519";
      identitiesOnly = true;
    };
  };
}
```

Pass `inputs` through `home-manager.extraSpecialArgs` when using Home Manager
inside NixOS or nix-darwin, or through `extraSpecialArgs` in
`homeManagerConfiguration`. Both modules accept a `package` override.

Home Manager owns `$XDG_CONFIG_HOME/gitbox/config.toml`. Change
`programs.gitbox.settings.default_server` and rebuild to change the default;
`gitbox --server <alias>` still overrides it for one command. The client refuses
to overwrite a symlink with `gitbox server set-default`.

After publishing the Gitbox changes to your input's remote, run
`nix flake update gitbox` in `~/.nix-config`, then rebuild the server and client
configurations using your usual deployment commands.

## Install and initialize

Install `gitboxd` on the server at a stable absolute path and `gitbox` on the
client. The server also needs `git`, `git-upload-pack`, `git-receive-pack`, and
`ssh-keygen` on its `PATH`.

Create a dedicated Unix account with `/var/lib/gitbox` as its home, and run
initialization as that account:

```sh
gitboxd init \
  --data-dir /var/lib/gitbox \
  --admin alice \
  --ssh-key /path/to/alice-laptop.pub \
  --ssh-key /path/to/alice-desktop.pub \
  --executable /usr/local/bin/gitboxd
```

`init` creates `config.toml`, `repos/`, and `.ssh/authorized_keys` under the
data directory. Repeat `--ssh-key` for each administrator key, with at least
one required. Each file must contain one public key without `authorized_keys`
options. All keys are validated before initialization writes any state.
Each key entry runs `gitboxd` as a restricted forced command.
The `--executable` path must remain valid after upgrades. If omitted, Gitbox
uses the path of the running executable.

Runtime configuration uses version 2 and stores only the administrator name:

```toml
version = 2

[admin]
name = "alice"
```

OpenSSH authenticates keys through `authorized_keys`; Gitbox does not duplicate
them in runtime configuration. For an existing standalone installation, replace
the version 1 configuration with this shape and remove `--key-id admin` from
each forced command when upgrading the binary. Preserve the key restrictions
and executable/data-directory paths. Existing repositories stay in place.
Manage subsequent key additions and removals in the authorized-keys file,
using the same restricted forced command for each entry.

Configure OpenSSH to use that authorized keys file for the dedicated account
and to allow only public-key authentication:

```text
Match User gitbox
    AuthorizedKeysFile /var/lib/gitbox/.ssh/authorized_keys
    AuthenticationMethods publickey
    PasswordAuthentication no
    KbdInteractiveAuthentication no
    PermitTTY no
    DisableForwarding yes
```

The account must not have another authentication path that grants a shell.
Gitbox does not edit `sshd_config` or create the Unix account.

## Use

Add an alias to the client's SSH configuration:

```text
Host home-gitbox
    HostName git.example.net
    User gitbox
    IdentityFile ~/.ssh/id_ed25519
```

Save that alias as the default server, then create and list folders and
repositories:

```sh
gitbox server set-default home-gitbox
gitbox folder create projects/archive
gitbox repo create projects/compiler
gitbox repo inspect projects/compiler
gitbox folder list
gitbox repo list
git clone home-gitbox:projects/compiler.git
```

Repository creation and inspection print the logical path, selected server,
Git remote URL, default branch, and whether the repository has any refs.
Remote URLs use the SCP-style form `<server>:<path>.git`, including for
repositories at the root, such as `home-gitbox:htk.git`.

To create a repository from an existing local Git working tree and push its
current branch:

```sh
gitbox repo create htk --push
# Choose another remote name when origin is already configured:
gitbox repo create projects/compiler --push --remote gitbox
```

`--push` adds the local remote, named `origin` by default, and pushes only the
current branch with upstream tracking. The remote repository's default branch
matches the local branch. Plain `repo create` creates an empty repository with
`main` as its default branch and leaves local Git configuration alone.

Publishing requires an existing working tree, a checked-out branch, and at least
one commit. An existing remote with the selected name is an error; use `--remote`
to choose another name. These checks run before server creation. Gitbox does not
stage or commit files. If the push fails, the server repository and configured
remote remain available, and the error includes a command to retry the push.

The default is stored as `default_server = "home-gitbox"` in
`~/.config/gitbox/config.toml`, or under `$XDG_CONFIG_HOME/gitbox/config.toml`
when that variable is set. `--server <alias>` overrides it for a command.
Lists are recursive, sorted, and print one logical path per line.

Both create commands make missing parent folders. An existing folder is
accepted; an existing repository is an error. A folder and repository cannot
have the same logical path. The `.git` suffix appears only in Git remote URLs
and on disk.

Only the configured administrator keys are accepted in this slice. Folder and
repository permissions will follow.

Run `scripts/e2e-ssh` to test initialization, remote management, push, clone,
and fetch against a temporary local OpenSSH server. It also verifies access
with two administrator keys, unrelated-key rejection, command restrictions,
and revocation of one key while the other remains usable.

## Docker test server

Create a local `.env` file with the absolute path to your SSH public key:

```text
GITBOX_ADMIN_KEY_FILE=/absolute/path/to/id_ed25519.pub
```

Then run `docker compose up --build -d`. Git ignores the local `.env` file.
The first start initializes the admin and creates a persistent `gitbox-data`
volume. SSH is published on `127.0.0.1:2222` by default. Set `GITBOX_SSH_PORT`
to change the host port, or `GITBOX_ADMIN` to change the admin name.

Add this SSH alias, using the matching private key:

```text
Host docker-gitbox
    HostName 127.0.0.1
    User gitbox
    Port 2222
    IdentityFile ~/.ssh/id_ed25519
```

Run the client from this workspace and use ordinary Git:

```sh
cargo run -p gitbox -- --server docker-gitbox folder create projects
cargo run -p gitbox -- --server docker-gitbox repo create projects/example
cargo run -p gitbox -- --server docker-gitbox repo list
git clone docker-gitbox:projects/example.git
```

`docker compose logs gitbox` shows server errors. `docker compose down` stops the
container and keeps its repositories. `docker compose down -v` also deletes the
test repositories and admin configuration.

## Code structure

The workspace keeps three crates. `gitbox-core` defines validated logical paths
and the shared administrative protocol. In `gitboxd`, `store` owns repository
filesystem operations and locking, `admin` executes administrative requests,
`rpc` handles JSON framing, and `ssh` parses and runs restricted SSH commands.
The client keeps SSH request/response handling in `transport`.

Administrative requests and responses use protocol version 4 and a JSON envelope
with `version` and `body` fields. Requests carry an operation and its required
arguments; paths validate during decoding. Errors include a code
(`invalid_request`, `unsupported_version`, or `operation_failed`) and a message.
Client and server must use the same protocol version.

Repository creation takes an exclusive tree lock. Listing and Git transport take
shared locks, with Git transport holding its lock until the Git process exits.
These locks coordinate Gitbox processes; external filesystem writers must be
coordinated separately.
