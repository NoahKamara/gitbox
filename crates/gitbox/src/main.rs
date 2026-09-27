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
    Create { path: LogicalPath },
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
    let request = match cli.command {
        Commands::Folder(FolderCommands::Create { path }) => Request::CreateFolder { path },
        Commands::Folder(FolderCommands::List) => Request::ListFolders {},
        Commands::Repo(RepoCommands::Create { path }) => Request::CreateRepository { path },
        Commands::Repo(RepoCommands::List) => Request::ListRepositories {},
        Commands::Server(ServerCommands::SetDefault { server }) => {
            transport::validate_server(&server)?;
            let config_path = config_path()?;
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
    };
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
    match transport::request(&server, &request)? {
        Response::Created { path } => println!("{path}"),
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
