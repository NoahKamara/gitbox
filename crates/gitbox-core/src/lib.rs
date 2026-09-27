use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u32 = 3;
pub const MAX_REQUEST_BYTES: u64 = 64 * 1024;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct LogicalPath(String);

impl LogicalPath {
    pub fn segments(&self) -> impl Iterator<Item = &str> {
        self.0.split('/')
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl FromStr for LogicalPath {
    type Err = &'static str;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if value.is_empty() || value.starts_with('/') || value.ends_with('/') {
            return Err("path must contain one or more relative segments");
        }

        for segment in value.split('/') {
            if segment.is_empty() || segment.len() > 251 || segment.ends_with(".git") {
                return Err("path contains an empty, oversized, or reserved segment");
            }
            let mut chars = segment.chars();
            if !chars.next().is_some_and(|c| c.is_ascii_alphanumeric())
                || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
                || segment.contains("..")
            {
                return Err(
                    "path segments must start with a letter or number and contain only ASCII letters, numbers, '.', '-', or '_'",
                );
            }
        }

        Ok(Self(value.to_owned()))
    }
}

impl fmt::Display for LogicalPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl<'de> Deserialize<'de> for LogicalPath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Envelope<T> {
    pub version: u32,
    pub body: T,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "method", deny_unknown_fields)]
pub enum Request {
    #[serde(rename = "folder.create")]
    CreateFolder { path: LogicalPath },
    #[serde(rename = "folder.list")]
    ListFolders {},
    #[serde(rename = "repo.create")]
    CreateRepository { path: LogicalPath },
    #[serde(rename = "repo.list")]
    ListRepositories {},
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum Response {
    Created { path: LogicalPath },
    Paths { paths: Vec<LogicalPath> },
    Error { code: ErrorCode, message: String },
}

#[derive(Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    InvalidRequest,
    UnsupportedVersion,
    OperationFailed,
}

#[cfg(test)]
mod tests {
    use super::LogicalPath;

    #[test]
    fn rejects_unsafe_paths() {
        for path in [
            "", "/a", "a/", "a//b", "a/../b", "a/./b", "a.git", "a/b.git", "a b",
        ] {
            assert!(path.parse::<LogicalPath>().is_err(), "{path}");
        }
    }

    #[test]
    fn accepts_nested_path() {
        assert_eq!(
            "projects/gitbox".parse::<LogicalPath>().unwrap().as_str(),
            "projects/gitbox"
        );
    }
    #[test]
    fn requests_reject_missing_extra_and_invalid_paths() {
        use super::Request;
        for json in [
            r#"{"method":"repo.create"}"#,
            r#"{"method":"folder.create","path":null}"#,
            r#"{"method":"repo.create","path":"../outside"}"#,
            r#"{"method":"folder.create","path":"folder.git"}"#,
            r#"{"method":"repo.list","path":"unexpected"}"#,
            r#"{"method":"folder.list","extra":true}"#,
            r#"{"method":"repo.create","path":"valid","extra":true}"#,
        ] {
            assert!(serde_json::from_str::<Request>(json).is_err(), "{json}");
        }
    }

    #[test]
    fn request_envelopes_round_trip_without_changing_the_wire_shape() {
        use super::{Envelope, PROTOCOL_VERSION, Request};
        for body in [
            serde_json::json!({"method":"repo.create","path":"projects/compiler"}),
            serde_json::json!({"method":"folder.create","path":"projects"}),
            serde_json::json!({"method":"repo.list"}),
            serde_json::json!({"method":"folder.list"}),
        ] {
            let wire = serde_json::json!({"version":PROTOCOL_VERSION,"body":body});
            let request: Envelope<Request> = serde_json::from_value(wire.clone()).unwrap();
            assert_eq!(serde_json::to_value(request).unwrap(), wire);
        }
    }
}
