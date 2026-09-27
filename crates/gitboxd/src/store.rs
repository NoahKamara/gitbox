use std::fs::{self, File, FileType, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail, ensure};
use gitbox_core::LogicalPath;

/// Owns the repository namespace and its filesystem operations.
/// Locks coordinate Gitbox processes; the tree must not be mutated by other writers.
pub struct RepositoryStore {
    root: PathBuf,
}

pub struct RepositoryTree {
    pub folders: Vec<LogicalPath>,
    pub repositories: Vec<LogicalPath>,
}

/// Keeps the tree stable for the lifetime of a Git transport process.
pub struct Repository {
    path: PathBuf,
    _lock: TreeLock,
}

struct TreeLock(File);

impl Drop for TreeLock {
    fn drop(&mut self) {
        // Release explicitly: another thread's fork can briefly retain this descriptor
        // until exec, even with close-on-exec enabled.
        let _ = self.0.unlock();
    }
}

impl Repository {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl RepositoryStore {
    pub fn open(root: &Path) -> Result<Self> {
        let kind = entry_type(root)?.context("repository root does not exist")?;
        ensure!(
            kind.is_dir() && !kind.is_symlink(),
            "repository root is not a regular directory"
        );
        Ok(Self {
            root: root.to_owned(),
        })
    }

    fn lock(&self, exclusive: bool) -> Result<TreeLock> {
        let kind = entry_type(&self.root)?.context("repository root does not exist")?;
        ensure!(
            kind.is_dir() && !kind.is_symlink(),
            "repository root is not a regular directory"
        );
        let path = self.root.join(".gitbox.lock");
        ensure!(
            entry_type(&path)?.is_none_or(|kind| kind.is_file()),
            "repository lock is not a regular file"
        );
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .context("open repository lock")?;
        if exclusive {
            file.lock()
        } else {
            file.lock_shared()
        }
        .context("lock repository tree")?;
        Ok(TreeLock(file))
    }

    pub fn open_repository(&self, path: &LogicalPath) -> Result<Repository> {
        let lock = self.lock(false)?;
        let (parent, name) = self.checked_parent(path, false)?;
        let repo = parent.join(format!("{name}.git"));
        let kind = entry_type(&repo)?.context("repository does not exist")?;
        ensure!(
            kind.is_dir() && !kind.is_symlink(),
            "repository is not a regular directory"
        );
        ensure!(
            is_bare_repository(&repo)?,
            "repository is not a bare Git repository"
        );
        Ok(Repository {
            path: repo,
            _lock: lock,
        })
    }

    pub fn list(&self) -> Result<RepositoryTree> {
        let _lock = self.lock(false)?;
        let root = &self.root;
        let mut folders = Vec::new();
        let mut repos = Vec::new();
        let mut pending = vec![(root.to_path_buf(), String::new())];
        while let Some((directory, prefix)) = pending.pop() {
            for entry in fs::read_dir(&directory)
                .with_context(|| format!("read folder {}", directory.display()))?
            {
                let entry = entry?;
                let kind = entry.file_type()?;
                if !kind.is_dir() || kind.is_symlink() {
                    continue;
                }
                let name = match entry.file_name().into_string() {
                    Ok(name) => name,
                    Err(_) => continue,
                };
                let (logical, is_repo) = match name.strip_suffix(".git") {
                    Some(base) => (format!("{prefix}{base}"), true),
                    None => (format!("{prefix}{name}"), false),
                };
                let Ok(path) = logical.parse::<LogicalPath>() else {
                    continue;
                };
                if is_repo {
                    let repo = entry.path();
                    if is_bare_repository(&repo)? {
                        repos.push(path);
                    }
                } else {
                    pending.push((entry.path(), format!("{logical}/")));
                    folders.push(path);
                }
            }
        }
        folders.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        repos.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(RepositoryTree {
            folders,
            repositories: repos,
        })
    }

    fn checked_parent<'a>(
        &self,
        path: &'a LogicalPath,
        create_missing: bool,
    ) -> Result<(PathBuf, &'a str)> {
        let root = &self.root;
        let mut segments = path.segments().peekable();
        let mut parent = root.to_path_buf();
        while let Some(segment) = segments.next() {
            if segments.peek().is_none() {
                return Ok((parent, segment));
            }
            ensure!(
                entry_type(&parent.join(format!("{segment}.git")))?.is_none(),
                "{segment} is already a repository"
            );
            parent.push(segment);
            match entry_type(&parent)? {
                Some(kind) => ensure!(
                    kind.is_dir() && !kind.is_symlink(),
                    "{} is not a regular folder",
                    parent.display()
                ),
                None if create_missing => fs::create_dir(&parent)
                    .with_context(|| format!("create folder {}", parent.display()))?,
                None => bail!("folder {} does not exist", parent.display()),
            }
        }
        unreachable!("validated paths have at least one segment")
    }

    pub fn create_folder(&self, path: &LogicalPath) -> Result<()> {
        let _lock = self.lock(true)?;
        let (parent, name) = self.checked_parent(path, true)?;
        ensure!(
            entry_type(&parent.join(format!("{name}.git")))?.is_none(),
            "a repository already exists at {path}"
        );
        let folder = parent.join(name);
        match entry_type(&folder)? {
            Some(kind) => ensure!(
                kind.is_dir() && !kind.is_symlink(),
                "{} is not a regular folder",
                folder.display()
            ),
            None => fs::create_dir(&folder)
                .with_context(|| format!("create folder {}", folder.display()))?,
        }
        Ok(())
    }

    pub fn create_repository(&self, path: &LogicalPath) -> Result<()> {
        let _lock = self.lock(true)?;
        let (parent, name) = self.checked_parent(path, true)?;
        ensure!(
            entry_type(&parent.join(name))?.is_none(),
            "a folder already exists at {path}"
        );
        let repo = parent.join(format!("{name}.git"));
        ensure!(
            entry_type(&repo)?.is_none(),
            "repository {path} already exists"
        );
        fs::create_dir(&repo).with_context(|| format!("create repository {}", repo.display()))?;
        let result = Command::new("git")
            .args(["init", "--bare", "--quiet", "--initial-branch=main", "."])
            .current_dir(&repo)
            .output()
            .context("run git init --bare");
        let init_error = match result {
            Ok(output) if output.status.success() => return Ok(()),
            Ok(output) => format!(
                "git init --bare failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
            Err(error) => format!("{error:#}"),
        };
        fs::remove_dir_all(&repo)
            .with_context(|| format!("{init_error}; could not remove {}", repo.display()))?;
        bail!("{init_error}")
    }
}

fn entry_type(path: &Path) -> Result<Option<FileType>> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => Ok(Some(metadata.file_type())),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
    }
}

fn is_bare_repository(path: &Path) -> Result<bool> {
    Ok(
        entry_type(&path.join("HEAD"))?.is_some_and(|kind| kind.is_file())
            && entry_type(&path.join("objects"))?.is_some_and(|kind| kind.is_dir()),
    )
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use tempfile::tempdir;

    use super::RepositoryStore;

    #[test]
    fn repo_create_adds_parents_and_initializes_bare_git() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("repos");
        fs::create_dir(&root).unwrap();
        let store = RepositoryStore::open(&root).unwrap();
        store
            .create_repository(&"team/compiler".parse().unwrap())
            .unwrap();
        let repo = root.join("team/compiler.git");
        let output = Command::new("git")
            .arg("--git-dir")
            .arg(repo)
            .args(["rev-parse", "--is-bare-repository"])
            .output()
            .unwrap();
        assert_eq!(output.stdout, b"true\n");
    }

    #[test]
    fn folder_create_is_recursive_and_idempotent() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("repos");
        fs::create_dir(&root).unwrap();
        let store = RepositoryStore::open(&root).unwrap();
        let path = "team/archive".parse().unwrap();
        store.create_folder(&path).unwrap();
        store.create_folder(&path).unwrap();
        assert!(root.join("team/archive").is_dir());
    }

    #[test]
    fn folder_and_repo_share_a_logical_namespace() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("repos");
        fs::create_dir(&root).unwrap();
        let store = RepositoryStore::open(&root).unwrap();
        store.create_repository(&"team".parse().unwrap()).unwrap();
        assert!(store.create_folder(&"team".parse().unwrap()).is_err());
        store.create_folder(&"archive".parse().unwrap()).unwrap();
        assert!(
            store
                .create_repository(&"archive".parse().unwrap())
                .is_err()
        );
    }

    #[test]
    fn list_returns_recursive_folders_and_repositories() {
        let dir = tempdir().unwrap();
        let root = dir.path().join("repos");
        fs::create_dir(&root).unwrap();
        let store = RepositoryStore::open(&root).unwrap();
        store
            .create_folder(&"projects/archive".parse().unwrap())
            .unwrap();
        store
            .create_repository(&"projects/compiler".parse().unwrap())
            .unwrap();
        store
            .create_repository(&"auto/deep/backup".parse().unwrap())
            .unwrap();
        let tree = store.list().unwrap();
        let folders: Vec<_> = tree.folders.iter().map(|p| p.as_str()).collect();
        let repos: Vec<_> = tree.repositories.iter().map(|p| p.as_str()).collect();
        assert_eq!(
            folders,
            ["auto", "auto/deep", "projects", "projects/archive"]
        );
        assert_eq!(repos, ["auto/deep/backup", "projects/compiler"]);
    }

    #[cfg(unix)]
    #[test]
    fn repo_create_rejects_symlink_parent() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let root = dir.path().join("repos");
        let outside = dir.path().join("outside");
        fs::create_dir(&root).unwrap();
        let store = RepositoryStore::open(&root).unwrap();
        fs::create_dir(&outside).unwrap();
        symlink(&outside, root.join("team")).unwrap();
        assert!(
            store
                .create_repository(&"team/compiler".parse().unwrap())
                .is_err()
        );
        assert!(!outside.join("compiler.git").exists());
    }
    #[test]
    fn open_repository_holds_a_shared_tree_lock_until_dropped() {
        let dir = tempdir().unwrap();
        let store = RepositoryStore::open(dir.path()).unwrap();
        let path = "project".parse().unwrap();
        store.create_repository(&path).unwrap();
        let repository = store.open_repository(&path).unwrap();
        assert_eq!(repository.path(), dir.path().join("project.git"));
        let second = store.open_repository(&path).unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(dir.path().join(".gitbox.lock"))
            .unwrap();
        assert!(matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));
        drop(repository);
        assert!(matches!(lock.try_lock(), Err(fs::TryLockError::WouldBlock)));
        drop(second);
        lock.try_lock().unwrap();
    }

    #[test]
    fn concurrent_creates_preserve_the_shared_namespace() {
        let dir = tempdir().unwrap();
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            let folder = scope.spawn(|| {
                let store = RepositoryStore::open(dir.path()).unwrap();
                barrier.wait();
                store.create_folder(&"project".parse().unwrap())
            });
            let repo = scope.spawn(|| {
                let store = RepositoryStore::open(dir.path()).unwrap();
                barrier.wait();
                store.create_repository(&"project".parse().unwrap())
            });
            assert_ne!(folder.join().unwrap().is_ok(), repo.join().unwrap().is_ok());
        });
        assert_ne!(
            dir.path().join("project").exists(),
            dir.path().join("project.git").exists()
        );
    }

    #[cfg(unix)]
    #[test]
    fn listing_and_opening_reject_symlinks_and_incomplete_repositories() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let store = RepositoryStore::open(dir.path()).unwrap();
        symlink(outside.path(), dir.path().join("linked.git")).unwrap();
        fs::create_dir(dir.path().join("incomplete.git")).unwrap();
        for name in ["linked", "incomplete", "missing"] {
            assert!(store.open_repository(&name.parse().unwrap()).is_err());
        }
        assert!(store.list().unwrap().repositories.is_empty());
        symlink(outside.path(), dir.path().join("folder")).unwrap();
        assert!(
            store
                .open_repository(&"folder/project".parse().unwrap())
                .is_err()
        );
        assert!(store.list().unwrap().folders.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_root_and_lock_before_mutation() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let outside = tempdir().unwrap();
        symlink(outside.path(), dir.path().join("linked")).unwrap();
        assert!(RepositoryStore::open(&dir.path().join("linked")).is_err());
        let store = RepositoryStore::open(dir.path()).unwrap();
        symlink(outside.path().join("lock"), dir.path().join(".gitbox.lock")).unwrap();
        assert!(store.create_folder(&"project".parse().unwrap()).is_err());
        assert!(!outside.path().join("lock").exists());
        assert!(!dir.path().join("project").exists());
    }
}
