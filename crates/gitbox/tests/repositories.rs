#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output};

use gitbox_core::PROTOCOL_VERSION;
use serde_json::json;
use tempfile::{TempDir, tempdir};

struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("bin")).unwrap();
        fs::write(dir.path().join("bin/ssh"), "#!/bin/sh\nif [ \"$2\" = 'gitboxd rpc' ]; then\n  cat >\"$GITBOX_TEST_REQUEST\"\n  printf '%s\\n' \"$GITBOX_TEST_RESPONSE\"\nelse\n  exec git-receive-pack \"$GITBOX_TEST_REPOSITORY\"\nfi\n").unwrap();
        fs::set_permissions(
            dir.path().join("bin/ssh"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        Self { dir }
    }

    fn cli(&self, args: &[&str]) -> Output {
        let path = std::env::join_paths(
            std::iter::once(self.dir.path().join("bin"))
                .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
        )
        .unwrap();
        Command::new(env!("CARGO_BIN_EXE_gitbox"))
            .args(["--server", "test-host", "repo"])
            .args(args)
            .current_dir(self.dir.path())
            .env("PATH", path)
            .env("XDG_CONFIG_HOME", self.dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GITBOX_TEST_REQUEST", self.dir.path().join("request.json"))
            .env("GITBOX_TEST_REPOSITORY", self.dir.path().join("remote.git"))
            .env("GITBOX_TEST_RESPONSE", json!({"version":PROTOCOL_VERSION,"body":{"status":"repository","repository":{"path":"htk","default_branch":"develop","empty":true}}}).to_string())
            .output().unwrap()
    }

    fn git(&self, args: &[&str]) -> Output {
        let output = Command::new("git")
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        output
    }
}

#[test]
fn create_and_inspect_show_remote_url_without_requiring_a_local_repository() {
    let fixture = Fixture::new();
    for (args, method) in [
        (["create", "htk"], "repo.create"),
        (["inspect", "htk"], "repo.inspect"),
    ] {
        let output = fixture.cli(&args);
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("Remote URL: test-host:htk.git"));
        let request: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.dir.path().join("request.json")).unwrap())
                .unwrap();
        assert_eq!(
            request,
            json!({"version":PROTOCOL_VERSION,"body":{"method":method,"path":"htk"}})
        );
        assert!(!fixture.dir.path().join(".git").exists());
    }
}

#[test]
fn push_checks_working_tree_commits_and_remote_conflicts_before_creation() {
    let fixture = Fixture::new();
    let output = fixture.cli(&["create", "htk", "--push"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("existing local Git working tree"));
    assert!(!fixture.dir.path().join("request.json").exists());

    fixture.git(&["init", "--quiet", "--initial-branch=develop"]);
    let output = fixture.cli(&["create", "htk", "--push"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("at least one commit"));
    assert!(!fixture.dir.path().join("request.json").exists());

    fixture.git(&["commit", "--quiet", "--allow-empty", "-m", "initial"]);
    fixture.git(&[
        "remote",
        "add",
        "origin",
        "https://example.invalid/original.git",
    ]);
    let output = fixture.cli(&["create", "htk", "--push"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("remote origin already exists"));
    assert!(!fixture.dir.path().join("request.json").exists());
    assert_eq!(
        fixture.git(&["remote", "get-url", "origin"]).stdout,
        b"https://example.invalid/original.git\n"
    );

    fixture.git(&["checkout", "--detach"]);
    let output = fixture.cli(&["create", "htk", "--push", "--remote", "gitbox"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("check out a branch"));
    assert!(!fixture.dir.path().join("request.json").exists());
}

#[test]
fn push_adds_named_remote_publishes_current_branch_and_sets_upstream() {
    let fixture = Fixture::new();
    fixture.git(&["init", "--quiet", "--initial-branch=develop"]);
    fixture.git(&["commit", "--quiet", "--allow-empty", "-m", "initial"]);
    fixture.git(&[
        "remote",
        "add",
        "origin",
        "https://example.invalid/original.git",
    ]);
    fixture.git(&[
        "init",
        "--bare",
        "--quiet",
        "--initial-branch=develop",
        "remote.git",
    ]);
    let output = fixture.cli(&["create", "htk", "--push", "--remote", "gitbox"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let request: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.dir.path().join("request.json")).unwrap())
            .unwrap();
    assert_eq!(
        request,
        json!({"version":PROTOCOL_VERSION,"body":{"method":"repo.create","path":"htk","default_branch":"develop"}})
    );
    assert_eq!(
        fixture.git(&["remote", "get-url", "gitbox"]).stdout,
        b"test-host:htk.git\n"
    );
    assert_eq!(
        fixture.git(&["remote", "get-url", "origin"]).stdout,
        b"https://example.invalid/original.git\n"
    );
    assert_eq!(
        fixture.git(&["config", "branch.develop.remote"]).stdout,
        b"gitbox\n"
    );
    assert_eq!(
        fixture.git(&["config", "branch.develop.merge"]).stdout,
        b"refs/heads/develop\n"
    );
    assert_eq!(
        fixture
            .git(&["--git-dir=remote.git", "rev-parse", "HEAD"])
            .stdout,
        fixture.git(&["rev-parse", "HEAD"]).stdout
    );
}

#[test]
fn failed_push_keeps_remote_and_explains_how_to_retry() {
    let fixture = Fixture::new();
    fixture.git(&["init", "--quiet", "--initial-branch=develop"]);
    fixture.git(&["commit", "--quiet", "--allow-empty", "-m", "initial"]);
    let output = fixture.cli(&["create", "htk", "--push"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains(
        "retry with git push --set-upstream origin refs/heads/develop:refs/heads/develop"
    ));
    assert_eq!(
        fixture.git(&["remote", "get-url", "origin"]).stdout,
        b"test-host:htk.git\n"
    );
}

#[test]
fn remote_name_requires_push_and_invalid_names_fail_before_creation() {
    let fixture = Fixture::new();
    let output = fixture.cli(&["create", "htk", "--remote", "gitbox"]);
    assert!(!output.status.success());
    assert!(!fixture.dir.path().join("request.json").exists());
    fixture.git(&["init", "--quiet", "--initial-branch=develop"]);
    fixture.git(&["commit", "--quiet", "--allow-empty", "-m", "initial"]);
    let output = fixture.cli(&["create", "htk", "--push", "--remote", "bad..remote"]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid remote name"));
    assert!(!fixture.dir.path().join("request.json").exists());
}
