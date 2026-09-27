use std::fs;
use std::process::Command;

use tempfile::tempdir;

#[test]
fn ssh_reads_an_external_config_without_initializing_mutable_state() {
    let dir = tempdir().unwrap();
    let data = dir.path().join("data");
    fs::create_dir_all(data.join("repos")).unwrap();
    let config = dir.path().join("declarative.toml");
    fs::write(
        &config,
        "version = 1\n[admin]\nname = 'alice'\nssh_key = 'test'\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_gitboxd"))
        .arg("ssh")
        .arg("--data-dir")
        .arg(&data)
        .arg("--config")
        .arg(&config)
        .args(["--key-id", "admin"])
        .env("SSH_ORIGINAL_COMMAND", "gitboxd rpc")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!data.join("config.toml").exists());

    fs::write(
        &config,
        "version = 2\n[admin]\nname = 'alice'\nssh_key = 'test'\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_gitboxd"))
        .arg("ssh")
        .arg("--data-dir")
        .arg(&data)
        .arg("--config")
        .arg(&config)
        .args(["--key-id", "admin"])
        .env("SSH_ORIGINAL_COMMAND", "gitboxd rpc")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unsupported configuration version"));
}
