mod admin;
mod rpc;
mod ssh;
mod store;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

use anyhow::{Context, Result, ensure};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Parser)]
#[command(version, about = "Gitbox SSH server")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a Gitbox installation and its first administrator.
    Init {
        #[arg(long)]
        data_dir: PathBuf,
        #[arg(long)]
        admin: String,
        #[arg(long)]
        ssh_key: PathBuf,
        /// Stable absolute path to gitboxd used in authorized_keys.
        #[arg(long)]
        executable: Option<PathBuf>,
    },
    /// Handle a restricted OpenSSH session.
    #[command(hide = true)]
    Ssh {
        #[arg(long)]
        data_dir: PathBuf,
        /// Configuration file, defaults to config.toml in the data directory.
        #[arg(long)]
        config: Option<PathBuf>,
        #[arg(long)]
        key_id: String,
    },
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Config {
    version: u32,
    admin: Admin,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Admin {
    name: String,
    ssh_key: String,
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Commands::Init {
            data_dir,
            admin,
            ssh_key,
            executable,
        } => init(&data_dir, &admin, &ssh_key, executable.as_deref()),
        Commands::Ssh {
            data_dir,
            config,
            key_id,
        } => ssh::session(&data_dir, config.as_deref(), &key_id),
    }
}

fn init(
    data_dir: &Path,
    admin: &str,
    ssh_key_path: &Path,
    executable: Option<&Path>,
) -> Result<()> {
    ensure!(
        !admin.is_empty()
            && admin
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')),
        "admin name must contain only ASCII letters, numbers, '-' or '_'"
    );

    let key_file = fs::read_to_string(ssh_key_path).context("read SSH public key")?;
    let key_line = key_file.trim();
    ensure!(
        !key_line.contains(['\n', '\r']),
        "SSH key file must contain one key"
    );
    let mut parts = key_line.split_whitespace();
    let key_type = parts.next().context("SSH key is empty")?;
    let key_data = parts.next().context("SSH key is incomplete")?;
    ensure!(
        matches!(
            key_type,
            "ssh-ed25519" | "ssh-rsa" | "sk-ssh-ed25519@openssh.com"
        ) || key_type.starts_with("ecdsa-sha2-")
            || key_type.starts_with("sk-ecdsa-sha2-"),
        "unsupported SSH public key type"
    );
    ensure!(
        key_data
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'=')),
        "SSH public key data is malformed"
    );
    let valid_key = Command::new("ssh-keygen")
        .args(["-lf"])
        .arg(ssh_key_path)
        .output()
        .context("run ssh-keygen to validate the public key")?;
    ensure!(valid_key.status.success(), "invalid SSH public key");
    let public_key = format!("{key_type} {key_data}");

    ensure!(
        !data_dir.exists() || !fs::symlink_metadata(data_dir)?.file_type().is_symlink(),
        "data directory must not be a symlink"
    );
    fs::create_dir_all(data_dir).context("create data directory")?;
    let data_dir = fs::canonicalize(data_dir).context("resolve data directory")?;
    #[cfg(unix)]
    fs::set_permissions(&data_dir, fs::Permissions::from_mode(0o700))?;
    let repos_dir = data_dir.join("repos");
    let ssh_dir = data_dir.join(".ssh");
    let config_path = data_dir.join("config.toml");
    let authorized_keys_path = ssh_dir.join("authorized_keys");
    ensure!(!config_path.exists(), "Gitbox is already initialized");
    ensure!(
        !authorized_keys_path.exists(),
        "authorized_keys already exists"
    );
    for dir in [&repos_dir, &ssh_dir] {
        if let Ok(metadata) = fs::symlink_metadata(dir) {
            ensure!(
                metadata.is_dir() && !metadata.file_type().is_symlink(),
                "{} is not a regular directory",
                dir.display()
            );
        }
        fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
        #[cfg(unix)]
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }

    let executable = executable
        .map(Path::to_path_buf)
        .map(Ok)
        .unwrap_or_else(env::current_exe)?;
    ensure!(executable.is_absolute(), "executable path must be absolute");
    ensure!(
        fs::metadata(&executable)?.is_file(),
        "executable path is not a file"
    );
    let executable = executable
        .to_str()
        .context("executable path must be UTF-8")?;
    let data_dir_text = data_dir
        .to_str()
        .context("data directory path must be UTF-8")?;
    ensure!(
        !executable.contains(['\n', '\r']) && !data_dir_text.contains(['\n', '\r']),
        "paths must not contain newlines"
    );
    let shell_command = format!(
        "{} ssh --data-dir {} --key-id admin",
        shell_quote(executable),
        shell_quote(data_dir_text)
    );
    let key_option = shell_command.replace('\\', "\\\\").replace('"', "\\\"");
    let authorized_key = format!("restrict,command=\"{key_option}\" {public_key}\n");
    let config = Config {
        version: 1,
        admin: Admin {
            name: admin.to_owned(),
            ssh_key: public_key,
        },
    };
    let config_text = toml::to_string_pretty(&config)?;

    let mut auth_options = OpenOptions::new();
    auth_options.write(true).create_new(true);
    #[cfg(unix)]
    auth_options.mode(0o600);
    let mut auth_file = auth_options
        .open(&authorized_keys_path)
        .context("create authorized_keys")?;
    if let Err(error) = auth_file.write_all(authorized_key.as_bytes()) {
        let _ = fs::remove_file(&authorized_keys_path);
        return Err(error).context("write authorized_keys");
    }

    let mut config_options = OpenOptions::new();
    config_options.write(true).create_new(true);
    #[cfg(unix)]
    config_options.mode(0o600);
    let result = config_options
        .open(&config_path)
        .and_then(|mut file| file.write_all(config_text.as_bytes()));
    if let Err(error) = result {
        let _ = fs::remove_file(&config_path);
        let _ = fs::remove_file(&authorized_keys_path);
        return Err(error).context("write config.toml");
    }

    println!("Initialized Gitbox for {admin} in {}", data_dir.display());
    println!(
        "Use {} as the SSH account's AuthorizedKeysFile",
        authorized_keys_path.display()
    );
    Ok(())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
