# Gitbox --- Concept

## 1. Overview

Gitbox is a small, self-hosted Git forge for people who already have a
server and want a simple place to keep their primary Git repositories.

It deliberately focuses on repository hosting and access control rather
than trying to reproduce GitHub, GitLab, or similar platforms. External
forges can instead be used as mirror destinations.

Gitbox uses SSH for both normal Git operations and administrative
operations.

The implementation is written in Rust.

## 2. Goals

Gitbox should:

-   Host ordinary bare Git repositories on a user-controlled server.
-   Support standard Git clone, fetch, pull, and push operations over
    SSH.
-   Authenticate exclusively using SSH public keys.
-   Provide a local `gitbox` CLI for remote administration.
-   Use the same SSH endpoint for Git and Gitbox administration.
-   Allow repositories to be organized into arbitrary folders.
-   Keep folders independent from users or identities.
-   Provide simple read and read/write permissions.
-   Allow permissions to be assigned to individual repositories or
    repository folders.
-   Keep the server understandable, inspectable, and easy to back up.
-   Avoid requiring a web server, web UI, HTTP Git transport, or
    external identity provider.

## 3. Non-goals

The initial Gitbox design does not attempt to provide:

-   Pull requests or merge requests
-   Issues
-   CI/CD
-   A web UI
-   Git-over-HTTP
-   OAuth or password authentication
-   Organizations or GitHub-style user namespaces
-   Social features
-   Package registries
-   Git LFS hosting
-   Built-in code review

These could be added independently later if there is a compelling use
case.

## 4. Components

Gitbox consists primarily of two executables.

### `gitbox`

The client-side CLI.

It runs on a user's workstation and manages a remote Gitbox installation
over SSH.

Example commands:

``` sh
gitbox repo list
gitbox repo inspect projects/compiler
gitbox repo create projects/compiler
gitbox repo delete projects/compiler

gitbox folder list
gitbox folder create projects

gitbox access list projects/compiler
gitbox access grant projects/compiler alice --write
gitbox access revoke projects/compiler alice
```

The CLI should use the user's normal SSH configuration where practical,
so aliases, identity files, agents, ProxyJump, and similar SSH
functionality continue to work.

### `gitboxd`

The server-side Gitbox process.

It is responsible for:

-   Associating an authenticated SSH key with a Gitbox principal.
-   Authorizing requested operations.
-   Resolving and validating repository paths.
-   Executing Git transport commands.
-   Processing Gitbox administrative commands.
-   Managing repository metadata and permissions.

Gitbox must not expose an unrestricted shell as part of its protocol.

## 5. SSH Architecture

SSH is the only required network-facing protocol.

After SSH authentication, the requested command determines whether the
connection is a Git operation or a Gitbox control operation.

Typical Git requests are:

``` text
git-upload-pack 'projects/compiler.git'
git-receive-pack 'projects/compiler.git'
```

`gitboxd` validates the repository path and authorization before
executing the corresponding Git program.

The server does not implement the Git wire protocol itself. It delegates
repository transport to standard Git programs such as:

-   `git-upload-pack`
-   `git-receive-pack`

Administrative connections use a restricted Gitbox command protocol
rather than a shell.

Conceptually:

``` text
gitbox repo list
gitbox repo inspect projects/compiler
gitbox repo create projects/compiler
gitbox repo delete projects/compiler
```

The exact machine-facing protocol does not have to remain identical to
the human CLI syntax. A versioned internal protocol can be introduced
while retaining the public CLI interface.

## 6. Authentication and Identity

Authentication is based on SSH public keys.

Gitbox should distinguish between a **key** and a **principal**.

A principal represents an identity known to Gitbox and may own multiple
SSH keys.

Conceptually:

``` text
Principal
    id
    name

SshKey
    id
    principal_id
    public_key
    fingerprint
    description
```

This avoids treating every replacement laptop or SSH key rotation as a
new authorization identity.

Gitbox itself does not need passwords.

A key that cannot be associated with an authorized principal must not
receive repository or administrative access.

## 7. Repository Layout

Repositories live as bare Git repositories beneath a configured
repository root.

For example:

``` text
/var/lib/gitbox/repos/
├── personal/
│   ├── dotfiles.git/
│   └── website.git/
├── projects/
│   ├── compiler.git/
│   └── gitbox.git/
└── experiments/
    └── parser.git/
```

Folders are purely organizational.

`projects/gitbox` does **not** imply that `projects` is a user, owner,
organization, or security principal.

This distinction is fundamental to Gitbox.

Repository paths therefore form a hierarchy independent of the identity
hierarchy.

## 8. Repository Paths

Gitbox should expose canonical logical paths without requiring callers
to care about the `.git` suffix.

For example:

``` text
projects/gitbox
```

can map internally to:

``` text
/var/lib/gitbox/repos/projects/gitbox.git
```

All repository paths must be normalized and validated before filesystem
access.

Paths containing constructs such as these must be rejected:

``` text
../
./
//
absolute paths
NUL bytes
```

Filesystem access must never be constructed from an unchecked
client-provided path.

Gitbox should also account for symlinks and other filesystem mechanisms
that could escape the configured repository root.

## 9. Permissions

The initial permission model intentionally has only two access levels:

-   `read`
-   `write`

`write` implies `read`.

No applicable permission means access is denied.

Permissions are granted to principals and can target either:

-   A specific repository
-   A folder containing repositories

For example:

``` text
alice -> projects/         write
bob   -> projects/gitbox   read
```

A folder permission applies recursively to repositories beneath that
folder.

Gitbox should define deterministic precedence for more-specific rules. A
reasonable model is that the most specific matching rule takes
precedence over a parent-folder rule.

The permission system should remain small enough that an administrator
can understand why a particular key can or cannot access a repository.

### Git operation mapping

`git-upload-pack` requires `read`.

`git-receive-pack` requires `write`.

Administrative operations have separate administrative authorization and
must not implicitly become available merely because a principal has
repository write access.

## 10. Administration

Gitbox needs a distinction between repository permissions and server
administration.

An administrator can perform operations such as:

-   Create and delete repositories.
-   Create repository folders.
-   Manage principals.
-   Add and remove SSH keys.
-   Grant and revoke permissions.
-   Configure mirrors.

Administration can initially be represented by a simple administrator
flag on a principal.

More elaborate role-based access control is unnecessary for the first
version.

## 11. Metadata

Repository contents remain ordinary Git repositories on disk.

Gitbox-specific metadata should be stored separately.

SQLite is a suitable initial metadata store because it provides:

-   Transactions
-   Referential integrity
-   Simple backups
-   No additional server process
-   Mature Rust support
-   Straightforward inspection and recovery

Likely metadata includes:

``` text
principals
ssh_keys
repositories
permissions
mirrors
configuration metadata
```

The filesystem remains authoritative for Git object data.

The database is authoritative for Gitbox identities and authorization.

Gitbox should explicitly define reconciliation behavior for cases where
repositories are manually added, removed, or moved on disk.

## 12. Repository Lifecycle

Creating a repository should approximately perform:

1.  Validate the requested logical path.
2.  Verify that the destination does not exist.
3.  Check administrative authorization.
4.  Create required organizational directories.
5.  Initialize a bare Git repository.
6.  Record Gitbox metadata.
7.  Return the resulting repository information.

Deletion should be deliberately harder to trigger accidentally.

The protocol should leave room for a confirmation mechanism or an
explicit force flag in the CLI.

Whether deletion immediately removes repository data or first moves it
into a recoverable trash area should be decided before the first stable
release.

## 13. CLI Configuration

A client should be able to define a Gitbox server using familiar SSH
concepts.

For example:

``` toml
[servers.home]
host = "git.example.net"
user = "git"
```

Usage could then look like:

``` sh
gitbox --server home repo list
```

A default server may be configurable.

Gitbox should avoid duplicating SSH configuration unnecessarily. Where
possible, the underlying SSH configuration should remain responsible for
ports, key selection, jump hosts, agents, and host aliases.

## 14. Git Remote Experience

Normal Git should remain normal Git.

A repository might be used as:

``` sh
git clone gitbox:projects/compiler.git
```

or:

``` sh
git remote add origin git@git.example.net:projects/compiler.git
git push -u origin main
```

No custom Git client is required.

The `gitbox` CLI is for forge management, not for replacing the `git`
CLI.

## 15. External Mirrors

GitHub, GitLab, Codeberg, and other Git hosts should be treated as
optional mirror destinations rather than dependencies.

Conceptually:

``` sh
gitbox mirror add projects/compiler git@github.com:example/compiler.git
gitbox mirror list projects/compiler
gitbox mirror push projects/compiler
```

Automatic mirroring can be introduced later.

Mirror failures must not prevent the primary Gitbox push from succeeding
unless explicitly configured otherwise.

This keeps Gitbox usable when an external forge is unavailable.

Mirror credentials should be independent from inbound user
authentication credentials.

## 16. Security Model

The main security boundary is the SSH-authenticated Gitbox server
process.

Important properties include:

-   No unrestricted shell.
-   No execution of arbitrary client-provided commands.
-   Strict repository path validation.
-   Authorization before invoking Git programs.
-   Repository processes executed with constrained privileges.
-   SSH keys identified by cryptographic fingerprints.
-   Administrative operations separated from repository write
    permission.
-   Database operations parameterized rather than constructed from
    untrusted strings.
-   Safe handling of symbolic links and filesystem traversal.
-   Auditable destructive operations.

The design should assume that repository names, Git refs, commit data,
and SSH-originated arguments are untrusted input.

## 17. Suggested Rust Workspace

A possible workspace layout is:

``` text
gitbox/
├── Cargo.toml
├── crates/
│   ├── gitbox-cli/
│   ├── gitbox-server/
│   ├── gitbox-core/
│   ├── gitbox-storage/
│   ├── gitbox-auth/
│   └── gitbox-protocol/
└── docs/
    └── Concept.md
```

Responsibilities could be divided as follows:

`gitbox-cli` contains the user-facing CLI.

`gitbox-server` contains the server entrypoint and SSH command dispatch.

`gitbox-core` contains domain types, permission evaluation, repository
path handling, and common errors.

`gitbox-storage` contains filesystem repository operations and SQLite
persistence.

`gitbox-auth` contains principal, SSH-key, and authorization logic.

`gitbox-protocol` defines the versioned administrative request/response
protocol shared by client and server.

This separation is a starting point rather than a requirement. The
project should avoid splitting into crates until boundaries provide
practical value.

## 18. Protocol Design

Administrative communication should eventually use a small versioned
protocol carried over SSH.

For example, the client might internally invoke:

``` text
gitbox-shell protocol-v1
```

and exchange structured requests and responses.

A serialization format such as JSON is sufficient initially. A binary
protocol is unlikely to provide meaningful benefit at this scale.

A structured protocol makes it possible to provide:

-   Stable error codes
-   Forward-compatible CLI/server versions
-   Machine-readable repository listings
-   Explicit protocol negotiation
-   Reliable escaping of repository names and other values

Human CLI output should be generated by the client rather than parsed
from server-formatted tables.

## 19. Observability

The server should maintain useful operational logs without logging
sensitive key material.

Useful events include:

-   Successful and failed authentication mapping
-   Repository reads
-   Repository writes
-   Repository creation
-   Repository deletion
-   Permission changes
-   SSH key changes
-   Mirror operations

For authentication events, fingerprints should be logged instead of
complete public keys.

An optional audit table can later preserve security-relevant
administrative events independently from normal application logs.

## 20. Backup and Recovery

Gitbox should be easy to back up using ordinary server tooling.

A complete backup consists primarily of:

``` text
repository root
SQLite metadata database
server configuration
```

Git repositories remain valid bare Git repositories and should not
require Gitbox-specific tooling to inspect or recover.

Backup documentation should explain how to obtain a consistent SQLite
snapshot and how repository changes occurring during backup are handled.

External mirrors provide additional redundancy but are not a substitute
for a Gitbox server backup.

## 21. MVP

The first usable Gitbox release should provide:

-   SSH public-key authentication
-   Principals with multiple SSH keys
-   Bare repositories stored in arbitrary folders
-   Clone and fetch over SSH
-   Push over SSH
-   Repository creation
-   Repository listing
-   Repository inspection
-   Repository deletion
-   Folder creation and listing
-   Repository-level read/write permissions
-   Folder-level inherited read/write permissions
-   Administrative principals
-   SQLite metadata
-   A Rust `gitbox` CLI
-   A Rust `gitboxd` server

External mirroring is a strong candidate for the next milestone rather
than a requirement for the initial MVP.

## 22. Design Principles

Gitbox should prefer boring, inspectable infrastructure.

Git repositories should remain ordinary Git repositories.

SSH should remain SSH.

The filesystem should remain understandable without Gitbox.

Authorization rules should be simple enough to reason about manually.

The CLI should automate administration without becoming a replacement
for Git itself.

External hosting services should enhance redundancy rather than become
dependencies.

The result should be a personal Git forge that can be installed on a
small server, understood by its owner, backed up with standard tools,
and operated for years without requiring a large application stack.
