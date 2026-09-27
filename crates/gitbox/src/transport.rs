use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{Context, Result, bail, ensure};
use gitbox_core::{Envelope, PROTOCOL_VERSION, Request, Response};

/// Exchange one administrative request using the user's normal SSH configuration.
pub fn request(server: &str, request: &Request) -> Result<Response> {
    validate_server(server)?;
    let mut payload = serde_json::to_vec(&Envelope {
        version: PROTOCOL_VERSION,
        body: request,
    })?;
    payload.push(b'\n');
    let mut child = Command::new("ssh")
        .arg(server)
        .arg("gitboxd rpc")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("start SSH")?;
    let write_result = (|| -> Result<()> {
        let mut stdin = child.stdin.take().context("SSH stdin is unavailable")?;
        stdin.write_all(&payload).context("write SSH request")
    })();
    if let Err(error) = write_result {
        let _ = child.kill();
        let _ = child.wait();
        return Err(error);
    }
    let output = child.wait_with_output().context("wait for SSH")?;
    ensure!(
        output.status.success(),
        "SSH failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    let response: Envelope<Response> =
        serde_json::from_slice(&output.stdout).context("decode server response")?;
    ensure!(
        response.version == PROTOCOL_VERSION,
        "unsupported server protocol version"
    );
    match (&response.body, request) {
        (
            Response::Created { path },
            Request::CreateFolder { path: requested }
            | Request::CreateRepository { path: requested },
        ) if path == requested => {}
        (Response::Paths { .. }, Request::ListFolders {} | Request::ListRepositories {}) => {}
        (Response::Error { .. }, _) => {}
        _ => bail!("unexpected server response"),
    }
    Ok(response.body)
}

pub fn validate_server(server: &str) -> Result<()> {
    ensure!(
        !server.is_empty() && !server.starts_with('-') && !server.chars().any(char::is_whitespace),
        "server must be a non-empty SSH host or alias without whitespace"
    );
    Ok(())
}
