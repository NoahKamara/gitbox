mod local;
mod transport;

use std::env;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use gitbox_core::{LogicalPath, Request, Response};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

/// The Gitbox CLI
#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Cli {
    /// SSH host or alias from your SSH configuration.
    #[arg(long, global = true)]
    server: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(subcommand)]
    Repo(RepoCommands),
    #[command(subcommand)]
    Folder(FolderCommands),
    #[command(subcommand)]
    Server(ServerCommands),
}

/// Manage Repositories
#[derive(Subcommand)]
enum RepoCommands {
    /// Create a repository
    Create {
        path: LogicalPath,
        /// Add a local remote and push the current branch, setting its upstream
        #[arg(long)]
        push: bool,
        /// Name of the local Git remote to add
        #[arg(long, default_value = "origin", requires = "push")]
        remote: String,
    },
    /// Show repository details and its Git remote URL
    Inspect { path: LogicalPath },
    /// List repositories recursively
    List,
}

/// Manage Folders
#[derive(Subcommand)]
enum FolderCommands {
    /// Create a folder
    Create { path: LogicalPath },
    /// List folders recursively
    List,
}

/// Manage the local server setting
#[derive(Subcommand)]
enum ServerCommands {
    /// Save an SSH host or alias as the default server
    SetDefault { server: String },
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error:#}");
        process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    if let Commands::Server(ServerCommands::SetDefault { server }) = &cli.command {
        transport::validate_server(server)?;
        let config_path = config_path()?;
        if fs::symlink_metadata(&config_path)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            bail!(
                "Gitbox config is a symlink; update its declarative configuration instead, or use --server for this command"
            );
        }
        let mut config = read_config(&config_path)?;
        config.insert(
            "default_server".to_owned(),
            toml::Value::String(server.clone()),
        );
        let parent = config_path.parent().context("invalid config path")?;
        fs::create_dir_all(parent).context("create Gitbox config directory")?;
        let temporary = config_path.with_extension(format!("toml.tmp-{}", process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let result = (|| -> Result<()> {
            let mut file = options
                .open(&temporary)
                .context("create temporary config")?;
            file.write_all(toml::to_string_pretty(&config)?.as_bytes())?;
            file.sync_all()?;
            fs::rename(&temporary, &config_path).context("save Gitbox config")?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result?;
        println!("Default server: {server}");
        return Ok(());
    }
    let server = match cli.server {
        Some(server) => server,
        None => {
            let config = read_config(&config_path()?)?;
            match config.get("default_server") {
                Some(toml::Value::String(server)) => server.clone(),
                Some(_) => bail!("default_server in Gitbox config must be a string"),
                None => {
                    bail!("no server selected; use --server or gitbox server set-default <alias>")
                }
            }
        }
    };
    transport::validate_server(&server)?;
    let mut publish = None;
    let request = match cli.command {
        Commands::Folder(FolderCommands::Create { path }) => Request::CreateFolder { path },
        Commands::Folder(FolderCommands::List) => Request::ListFolders {},
        Commands::Repo(RepoCommands::Create { path, push, remote }) => {
            let default_branch = if push {
                let repository = local::LocalRepository::prepare(&remote)?;
                let branch = repository.branch.clone();
                publish = Some((repository, remote));
                Some(branch)
            } else {
                None
            };
            Request::CreateRepository {
                path,
                default_branch,
            }
        }
        Commands::Repo(RepoCommands::Inspect { path }) => Request::InspectRepository { path },
        Commands::Repo(RepoCommands::List) => Request::ListRepositories {},
        Commands::Server(_) => unreachable!("server settings were handled above"),
    };
    match transport::request(&server, &request)? {
        Response::Created { path } => println!("{path}"),
        Response::Repository { repository } => {
            let url = format!("{server}:{}.git", repository.path);
            println!("Repository: {}", repository.path);
            println!("Server: {server}");
            println!("Remote URL: {url}");
            println!(
                "Default branch: {}",
                repository.default_branch.as_deref().unwrap_or("(detached)")
            );
            if let Some((local, remote)) = publish {
                local.publish(&remote, &url)?;
            } else {
                println!("Empty: {}", if repository.empty { "yes" } else { "no" });
            }
        }
        Response::Paths { paths } => {
            for path in paths {
                println!("{path}");
            }
        }
        Response::Error { message, .. } => bail!("{message}"),
    }
    Ok(())
}

fn config_path() -> Result<PathBuf> {
    let base = match env::var_os("XDG_CONFIG_HOME") {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from(env::var_os("HOME").context("HOME is not set")?).join(".config"),
    };
    Ok(base.join("gitbox/config.toml"))
}

fn read_config(path: &Path) -> Result<toml::Table> {
    match fs::read_to_string(path) {
        Ok(text) => toml::from_str(&text).context("parse Gitbox config"),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(toml::Table::new()),
        Err(error) => Err(error).context("read Gitbox config"),
    }
}
