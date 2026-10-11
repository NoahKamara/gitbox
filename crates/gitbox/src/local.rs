use std::path::PathBuf;
use std::process::Command;

use anyhow::{Context, Result, ensure};

/// A local working tree and the committed branch selected for publishing.
pub struct LocalRepository {
    root: PathBuf,
    pub branch: String,
}

impl LocalRepository {
    /// Check local state before creating anything on the server.
    pub fn prepare(remote: &str) -> Result<Self> {
        let root = Command::new("git")
            .args(["rev-parse", "--show-toplevel"])
            .output()
            .context("find local Git repository")?;
        ensure!(
            root.status.success(),
            "--push requires an existing local Git working tree"
        );
        let root = PathBuf::from(String::from_utf8(root.stdout)?.trim_end_matches('\n'));
        let branch = Command::new("git")
            .args(["symbolic-ref", "--quiet", "HEAD"])
            .current_dir(&root)
            .output()
            .context("read current branch")?;
        ensure!(
            branch.status.success(),
            "--push requires a branch; check out a branch before creating the repository"
        );
        let branch = String::from_utf8(branch.stdout)?;
        let branch = branch
            .trim()
            .strip_prefix("refs/heads/")
            .context("HEAD does not point to a local branch")?
            .to_owned();
        let commit = Command::new("git")
            .args(["rev-parse", "--verify", "HEAD^{commit}"])
            .current_dir(&root)
            .output()
            .context("read current commit")?;
        ensure!(
            commit.status.success(),
            "--push requires at least one commit; commit your files first"
        );
        let valid_remote = Command::new("git")
            .args([
                "check-ref-format",
                &format!("refs/remotes/{remote}/gitbox-check"),
            ])
            .current_dir(&root)
            .output()
            .context("validate remote name")?;
        ensure!(
            !remote.starts_with('-') && valid_remote.status.success(),
            "invalid remote name: {remote}"
        );
        let remotes = Command::new("git")
            .arg("remote")
            .current_dir(&root)
            .output()
            .context("list local remotes")?;
        ensure!(
            remotes.status.success(),
            "list local remotes: {}",
            String::from_utf8_lossy(&remotes.stderr).trim()
        );
        ensure!(
            !String::from_utf8(remotes.stdout)?
                .lines()
                .any(|name| name == remote),
            "remote {remote} already exists; choose another name with --remote <name>"
        );
        Ok(Self { root, branch })
    }

    /// Configure the new remote and push the selected branch with an upstream.
    pub fn publish(&self, remote: &str, url: &str) -> Result<()> {
        let added = Command::new("git")
            .args(["remote", "add", "--", remote, url])
            .current_dir(&self.root)
            .output()
            .context("add local Git remote")?;
        ensure!(
            added.status.success(),
            "repository created, but adding remote {remote} failed: {}",
            String::from_utf8_lossy(&added.stderr).trim()
        );
        println!("Remote: {remote}");
        let refspec = format!("refs/heads/{0}:refs/heads/{0}", self.branch);
        let pushed = Command::new("git")
            .args(["push", "--set-upstream", "--", remote, &refspec])
            .current_dir(&self.root)
            .status()
            .context("repository created and remote configured, but starting git push failed")?;
        ensure!(
            pushed.success(),
            "repository created and remote {remote} configured, but push failed; retry with git push --set-upstream {remote} {refspec}"
        );
        Ok(())
    }
}
