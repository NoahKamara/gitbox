use std::io::{Read, Write};

use anyhow::Result;
use gitbox_core::{Envelope, ErrorCode, MAX_REQUEST_BYTES, PROTOCOL_VERSION, Request, Response};

use crate::{admin, store::RepositoryStore};

/// One bounded JSON request and one response per SSH session, delimited by EOF.
pub fn serve(store: &RepositoryStore, input: impl Read, mut output: impl Write) -> Result<()> {
    let mut payload = Vec::new();
    input
        .take(MAX_REQUEST_BYTES + 1)
        .read_to_end(&mut payload)?;
    let response = if payload.len() as u64 > MAX_REQUEST_BYTES {
        Response::Error {
            code: ErrorCode::InvalidRequest,
            message: "request is too large".into(),
        }
    } else {
        match serde_json::from_slice::<Envelope<Request>>(&payload) {
            Err(error) => Response::Error {
                code: ErrorCode::InvalidRequest,
                message: format!("decode request: {error}"),
            },
            Ok(request) if request.version != PROTOCOL_VERSION => Response::Error {
                code: ErrorCode::UnsupportedVersion,
                message: "unsupported protocol version".into(),
            },
            Ok(request) => match admin::execute(store, request.body) {
                Ok(response) => response,
                Err(error) => Response::Error {
                    code: ErrorCode::OperationFailed,
                    message: format!("{error:#}"),
                },
            },
        }
    };
    serde_json::to_writer(
        &mut output,
        &Envelope {
            version: PROTOCOL_VERSION,
            body: response,
        },
    )?;
    output.write_all(b"\n")?;
    output.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::serve;
    use crate::store::RepositoryStore;
    use gitbox_core::{Envelope, ErrorCode, MAX_REQUEST_BYTES, PROTOCOL_VERSION, Response};
    use tempfile::tempdir;

    #[test]
    fn invalid_requests_return_codes_without_changing_the_tree() {
        let dir = tempdir().unwrap();
        let store = RepositoryStore::open(dir.path()).unwrap();
        let cases = [
            (b"not json".to_vec(), ErrorCode::InvalidRequest),
            (vec![b' '; MAX_REQUEST_BYTES as usize + 1], ErrorCode::InvalidRequest),
            (serde_json::to_vec(&serde_json::json!({"version":PROTOCOL_VERSION,"body":{"method":"repo.create","path":"../outside"}})).unwrap(), ErrorCode::InvalidRequest),
            (serde_json::to_vec(&serde_json::json!({"version":0,"body":{"method":"folder.create","path":"unexpected"}})).unwrap(), ErrorCode::UnsupportedVersion),
        ];
        for (payload, expected_code) in cases {
            let mut output = Vec::new();
            serve(&store, payload.as_slice(), &mut output).unwrap();
            let response: Envelope<Response> = serde_json::from_slice(&output).unwrap();
            assert_eq!(response.version, PROTOCOL_VERSION);
            match response.body {
                Response::Error { code, .. } => assert_eq!(code, expected_code),
                _ => panic!("expected an error response"),
            }
        }
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn rpc_reports_created_paths_listings_and_operation_errors() {
        let dir = tempdir().unwrap();
        let store = RepositoryStore::open(dir.path()).unwrap();
        for (body, expected) in [
            (
                serde_json::json!({"method":"repo.create","path":"projects/compiler"}),
                "created",
            ),
            (serde_json::json!({"method":"repo.list"}), "paths"),
            (
                serde_json::json!({"method":"folder.create","path":"projects/compiler"}),
                "error",
            ),
        ] {
            let payload =
                serde_json::to_vec(&serde_json::json!({"version":PROTOCOL_VERSION,"body":body}))
                    .unwrap();
            let mut output = Vec::new();
            serve(&store, payload.as_slice(), &mut output).unwrap();
            let response: serde_json::Value = serde_json::from_slice(&output).unwrap();
            assert_eq!(response["body"]["status"], expected);
            match expected {
                "created" => assert_eq!(response["body"]["path"], "projects/compiler"),
                "paths" => assert_eq!(
                    response["body"]["paths"],
                    serde_json::json!(["projects/compiler"])
                ),
                _ => assert_eq!(response["body"]["code"], "operation_failed"),
            }
        }
    }
}
