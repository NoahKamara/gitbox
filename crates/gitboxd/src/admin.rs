use anyhow::Result;
use gitbox_core::{Request, Response};

use crate::store::RepositoryStore;

/// Execute an administrative request after the SSH session has admitted the administrator.
pub fn execute(store: &RepositoryStore, request: Request) -> Result<Response> {
    match request {
        Request::CreateFolder { path } => {
            store.create_folder(&path)?;
            Ok(Response::Created { path })
        }
        Request::CreateRepository {
            path,
            default_branch,
        } => {
            store.create_repository(&path, default_branch.as_deref().unwrap_or("main"))?;
            Ok(Response::Repository {
                repository: store.inspect_repository(&path)?,
            })
        }
        Request::InspectRepository { path } => Ok(Response::Repository {
            repository: store.inspect_repository(&path)?,
        }),
        Request::ListFolders {} => Ok(Response::Paths {
            paths: store.list()?.folders,
        }),
        Request::ListRepositories {} => Ok(Response::Paths {
            paths: store.list()?.repositories,
        }),
    }
}
