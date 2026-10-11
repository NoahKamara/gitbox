use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::tempdir;

fn generate_key(path: &Path) {
    let output = Command::new("ssh-keygen")
        .args(["-q", "-t", "ed25519", "-N", "", "-f"])
        .arg(path)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_accepts_multiple_keys_and_keeps_them_out_of_runtime_config() {
    let dir = tempdir().unwrap();
    for name in ["laptop", "desktop"] {
        generate_key(&dir.path().join(name));
    }
    let data = dir.path().join("data");
    let output = Command::new(env!("CARGO_BIN_EXE_gitboxd"))
        .arg("init")
        .arg("--data-dir")
        .arg(&data)
        .args(["--admin", "alice", "--ssh-key"])
        .arg(dir.path().join("laptop.pub"))
        .arg("--ssh-key")
        .arg(dir.path().join("desktop.pub"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let config: toml::Value =
        toml::from_str(&fs::read_to_string(data.join("config.toml")).unwrap()).unwrap();
    assert_eq!(config["version"].as_integer(), Some(2));
    let admin = config["admin"].as_table().unwrap();
    assert_eq!(admin.len(), 1);
    assert_eq!(admin["name"].as_str(), Some("alice"));

    let authorized_keys = fs::read_to_string(data.join(".ssh/authorized_keys")).unwrap();
    let entries: Vec<_> = authorized_keys.lines().collect();
    assert_eq!(entries.len(), 2);
    for (entry, name) in entries.iter().zip(["laptop", "desktop"]) {
        let public_key = fs::read_to_string(dir.path().join(format!("{name}.pub"))).unwrap();
        let mut parts = public_key.split_whitespace();
        let key = format!("{} {}", parts.next().unwrap(), parts.next().unwrap());
        assert!(entry.starts_with("restrict,command=\""));
        assert!(entry.ends_with(&format!("\" {key}")));
        assert!(!entry.contains("--key-id"));
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(data.join(".ssh/authorized_keys"))
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}

#[test]
fn init_rejects_every_invalid_key_before_creating_state() {
    let dir = tempdir().unwrap();
    generate_key(&dir.path().join("valid"));
    let key = fs::read_to_string(dir.path().join("valid.pub")).unwrap();
    let malformed = [
        String::new(),
        "ssh-ed25519".to_owned(),
        "ssh-ed25519 AAAA".to_owned(),
        "ssh-ed25519 !invalid!".to_owned(),
        format!("restrict {key}"),
        format!("command=\"id\" {key}"),
        format!("{key}{key}"),
        format!("\n{key}"),
        format!("{key}\n"),
        format!("{}\rcomment", key.trim()),
    ];
    for (index, invalid_key) in malformed.iter().enumerate() {
        let invalid_path = dir.path().join("invalid.pub");
        fs::write(&invalid_path, invalid_key).unwrap();
        let data = dir.path().join(format!("data-{index}"));
        let output = Command::new(env!("CARGO_BIN_EXE_gitboxd"))
            .arg("init")
            .arg("--data-dir")
            .arg(&data)
            .args(["--admin", "alice", "--ssh-key"])
            .arg(dir.path().join("valid.pub"))
            .arg("--ssh-key")
            .arg(&invalid_path)
            .output()
            .unwrap();
        assert!(!output.status.success(), "accepted {invalid_key:?}");
        assert!(!data.exists(), "created state for {invalid_key:?}");
    }
}

#[test]
fn init_requires_at_least_one_key() {
    let dir = tempdir().unwrap();
    let data = dir.path().join("data");
    let output = Command::new(env!("CARGO_BIN_EXE_gitboxd"))
        .arg("init")
        .arg("--data-dir")
        .arg(&data)
        .args(["--admin", "alice"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("--ssh-key"));
    assert!(!data.exists());
}
