#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;

use gitbox_core::PROTOCOL_VERSION;
use serde_json::json;
use tempfile::tempdir;

#[test]
fn set_default_preserves_declarative_config_symlinks() {
    let dir = tempdir().unwrap();
    let managed = dir.path().join("managed.toml");
    let contents = "default_server = 'declarative-host'\n";
    fs::write(&managed, contents).unwrap();
    let config_dir = dir.path().join("gitbox");
    fs::create_dir(&config_dir).unwrap();
    let config = config_dir.join("config.toml");
    std::os::unix::fs::symlink(&managed, &config).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_gitbox"))
        .args(["server", "set-default", "another-host"])
        .env("XDG_CONFIG_HOME", dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("declarative configuration"));
    assert!(
        fs::symlink_metadata(&config)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_to_string(managed).unwrap(), contents);
}

#[test]
fn cli_exchanges_typed_requests_and_rejects_invalid_responses() {
    let dir = tempdir().unwrap();
    let ssh = dir.path().join("ssh");
    fs::write(
        &ssh,
        "#!/bin/sh\ncat >\"$GITBOX_TEST_REQUEST\"\nprintf '%s\\n' \"$GITBOX_TEST_RESPONSE\"\n",
    )
    .unwrap();
    fs::set_permissions(&ssh, fs::Permissions::from_mode(0o700)).unwrap();
    let request_path = dir.path().join("request.json");
    let path = std::env::join_paths(
        std::iter::once(dir.path().to_path_buf())
            .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap())),
    )
    .unwrap();
    for (response, expected_output, expected_error) in [
        (
            json!({"version":PROTOCOL_VERSION,"body":{"status":"created","path":"projects/compiler"}}),
            Some("projects/compiler\n"),
            None,
        ),
        (
            json!({"version":0,"body":{"status":"created","path":"projects/compiler"}}),
            None,
            Some("unsupported server protocol version"),
        ),
        (
            json!({"version":PROTOCOL_VERSION,"body":{"status":"created","path":"different"}}),
            None,
            Some("unexpected server response"),
        ),
        (
            json!({"version":PROTOCOL_VERSION,"body":{"status":"paths","paths":[]}}),
            None,
            Some("unexpected server response"),
        ),
        (
            json!({"version":PROTOCOL_VERSION,"body":{"status":"created","path":"../outside"}}),
            None,
            Some("decode server response"),
        ),
        (
            json!({"version":PROTOCOL_VERSION,"body":{"status":"error","code":"operation_failed","message":"already exists"}}),
            None,
            Some("already exists"),
        ),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_gitbox"))
            .args([
                "--server",
                "test-host",
                "repo",
                "create",
                "projects/compiler",
            ])
            .env("PATH", &path)
            .env("GITBOX_TEST_REQUEST", &request_path)
            .env("GITBOX_TEST_RESPONSE", response.to_string())
            .output()
            .unwrap();
        let request: serde_json::Value =
            serde_json::from_slice(&fs::read(&request_path).unwrap()).unwrap();
        assert_eq!(
            request,
            json!({"version":PROTOCOL_VERSION,"body":{"method":"repo.create","path":"projects/compiler"}})
        );
        if let Some(expected) = expected_output {
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            assert_eq!(String::from_utf8(output.stdout).unwrap(), expected);
        }
        if let Some(expected) = expected_error {
            assert!(!output.status.success());
            assert!(
                String::from_utf8_lossy(&output.stderr).contains(expected),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}
