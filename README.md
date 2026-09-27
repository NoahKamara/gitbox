# Gitbox

Gitbox hosts bare Git repositories behind a restricted OpenSSH account. The
current slice has one administrator. The administrator can create folders and
repositories with the `gitbox` client; ordinary Git handles push, clone, and
fetch.

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
  --ssh-key /path/to/alice.pub \
  --executable /usr/local/bin/gitboxd
```

`init` creates `config.toml`, `repos/`, and `.ssh/authorized_keys` under the
data directory. The key entry runs `gitboxd` as a restricted forced command.
The `--executable` path must remain valid after upgrades. If omitted, Gitbox
uses the path of the running executable.

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
gitbox folder list
gitbox repo list
git clone home-gitbox:projects/compiler.git
```

The default is stored as `default_server = "home-gitbox"` in
`~/.config/gitbox/config.toml`, or under `$XDG_CONFIG_HOME/gitbox/config.toml`
when that variable is set. `--server <alias>` overrides it for a command.
Lists are recursive, sorted, and print one logical path per line.

Both create commands make missing parent folders. An existing folder is
accepted; an existing repository is an error. A folder and repository cannot
have the same logical path. The `.git` suffix appears only in Git remote URLs
and on disk.

Only the initialized admin key is accepted in this slice. Folder and
repository permissions will follow.

Run `scripts/e2e-ssh` to test initialization, remote management, push, clone,
and fetch against a temporary local OpenSSH server.

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

Administrative requests and responses use protocol version 3 and a JSON envelope
with `version` and `body` fields. Requests carry an operation and its required
arguments; paths validate during decoding. Errors include a code
(`invalid_request`, `unsupported_version`, or `operation_failed`) and a message.
Client and server must use the same protocol version.

Repository creation takes an exclusive tree lock. Listing and Git transport take
shared locks, with Git transport holding its lock until the Git process exits.
These locks coordinate Gitbox processes; external filesystem writers must be
coordinated separately.
