use std::path::Path;
use std::process::Command;
use std::str::FromStr;
use std::{env, fs, io};

use anyhow::{Context, Result, bail, ensure};
use gitbox_core::LogicalPath;

use crate::{Config, rpc, store::RepositoryStore};

#[derive(Debug, PartialEq, Eq)]
enum SshCommand {
    AdminRpc,
    UploadPack(LogicalPath),
    ReceivePack(LogicalPath),
}

impl FromStr for SshCommand {
    type Err = anyhow::Error;

    fn from_str(command: &str) -> Result<Self> {
        if command == "gitboxd rpc" {
            return Ok(Self::AdminRpc);
        }
        let (program, argument) = command.split_once(' ').context("unsupported SSH command")?;
        let repo_name = argument
            .strip_prefix('\'')
            .and_then(|value| value.strip_suffix(".git'"))
            .context("Git repository path must be quoted and end in .git")?;
        let path = repo_name.parse().map_err(anyhow::Error::msg)?;
        match program {
            "git-upload-pack" => Ok(Self::UploadPack(path)),
            "git-receive-pack" => Ok(Self::ReceivePack(path)),
            _ => bail!("unsupported SSH command"),
        }
    }
}

pub fn session(data_dir: &Path, key_id: &str) -> Result<()> {
    let config_text =
        fs::read_to_string(data_dir.join("config.toml")).context("read Gitbox configuration")?;
    let config: Config = toml::from_str(&config_text).context("parse Gitbox configuration")?;
    ensure!(config.version == 1, "unsupported configuration version");
    ensure!(key_id == "admin", "unknown SSH key");
    let command: SshCommand = env::var("SSH_ORIGINAL_COMMAND")
        .context("SSH_ORIGINAL_COMMAND is missing")?
        .parse()?;
    let store = RepositoryStore::open(&data_dir.join("repos"))?;
    let (program, path) = match command {
        SshCommand::AdminRpc => return rpc::serve(&store, io::stdin().lock(), io::stdout().lock()),
        SshCommand::UploadPack(path) => ("git-upload-pack", path),
        SshCommand::ReceivePack(path) => ("git-receive-pack", path),
    };
    let repository = store.open_repository(&path)?;
    let status = Command::new(program)
        .arg(repository.path())
        .status()
        .with_context(|| format!("run {program}"))?;
    ensure!(status.success(), "{program} exited with {status}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::SshCommand;

    #[test]
    fn accepts_only_supported_commands() {
        assert_eq!(
            "gitboxd rpc".parse::<SshCommand>().unwrap(),
            SshCommand::AdminRpc
        );
        assert_eq!(
            "git-upload-pack 'projects/compiler.git'"
                .parse::<SshCommand>()
                .unwrap(),
            SshCommand::UploadPack("projects/compiler".parse().unwrap())
        );
        assert_eq!(
            "git-receive-pack 'projects/compiler.git'"
                .parse::<SshCommand>()
                .unwrap(),
            SshCommand::ReceivePack("projects/compiler".parse().unwrap())
        );
        for command in [
            "",
            "sh",
            "gitboxd rpc extra",
            "git-upload-pack projects/compiler.git",
            "git-upload-pack '/outside.git'",
            "git-upload-pack '../outside.git'",
            "git-upload-pack 'projects/compiler.git'; id",
            "git-upload-pack 'a.git' 'b.git'",
            "git-receive-pack 'a.git\n'",
            "echo 'a.git'",
        ] {
            assert!(command.parse::<SshCommand>().is_err(), "{command}");
        }
    }
}
