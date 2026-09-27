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
        Request::CreateRepository { path } => {
            store.create_repository(&path)?;
            Ok(Response::Created { path })
        }
        Request::ListFolders {} => Ok(Response::Paths {
            paths: store.list()?.folders,
        }),
        Request::ListRepositories {} => Ok(Response::Paths {
            paths: store.list()?.repositories,
        }),
    }
}
