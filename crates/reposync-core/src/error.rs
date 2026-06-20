//! error - owned by E-05 (AppError taxonomy: thiserror + serde + specta::Type).
//!
//! E-05 extends this to the full ~30-variant taxonomy; these codes are stable.
//!
//! The wire shape ([`AppErrorPayload`]) is FROZEN: every variant serializes to a
//! flat object `{ code, message, remediation, context }`. The frontend keys off
//! the stable `code` string; `context` carries variant-specific detail as JSON.

use serde::{Deserialize, Serialize};
use serde_json::json;

/// The on-the-wire representation of an [`AppError`]. Every error the core sends
/// across IPC reduces to this flat shape.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
pub struct AppErrorPayload {
    /// Stable machine code, e.g. `fs.path_missing`.
    pub code: String,
    /// Human-readable summary of what went wrong.
    pub message: String,
    /// Actionable guidance for resolving the error.
    pub remediation: String,
    /// Variant-specific detail (path, exit_code, stderr, cause, ...).
    pub context: Option<serde_json::Value>,
}

/// The RepoSync error taxonomy (week-1 tracer subset).
///
/// EXACTLY seven variants for the tracer. Each carries a stable [`code`] string
/// and a [`remediation`] hint; the `Display` impl supplies the `message`.
///
/// [`code`]: AppError::code
/// [`remediation`]: AppError::remediation
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("path does not exist: {path}")]
    PathMissing { path: String },

    #[error("path is not a directory: {path}")]
    NotADirectory { path: String },

    #[error("not a git repository: {path}")]
    NotARepo { path: String },

    #[error("a repo with this path is already tracked: {path}")]
    DuplicateRepo { path: String },

    #[error("the git executable could not be found")]
    GitNotFound,

    #[error("git fetch failed (exit {exit_code:?}): {stderr}")]
    FetchFailed {
        exit_code: Option<i32>,
        stderr: String,
    },

    #[error("database query failed: {cause}")]
    Db { cause: String },
}

impl AppError {
    /// Stable machine-readable error code. These strings are part of the IPC
    /// contract and must not change.
    pub fn code(&self) -> &'static str {
        match self {
            AppError::PathMissing { .. } => "fs.path_missing",
            AppError::NotADirectory { .. } => "fs.not_a_directory",
            AppError::NotARepo { .. } => "git.not_a_repo",
            AppError::DuplicateRepo { .. } => "fs.duplicate_repo",
            AppError::GitNotFound => "git.not_found",
            AppError::FetchFailed { .. } => "git.fetch_failed",
            AppError::Db { .. } => "db.query_failed",
        }
    }

    /// Actionable guidance shown to the user alongside the message.
    pub fn remediation(&self) -> String {
        match self {
            AppError::PathMissing { .. } => {
                "Check that the folder still exists and the path is spelled correctly.".to_string()
            }
            AppError::NotADirectory { .. } => {
                "Select a folder, not a file.".to_string()
            }
            AppError::NotARepo { .. } => {
                "Choose a folder that contains a git repository (one with a .git directory)."
                    .to_string()
            }
            AppError::DuplicateRepo { .. } => {
                "This repository is already being tracked. Open it from the existing list."
                    .to_string()
            }
            AppError::GitNotFound => {
                "Install git and ensure it is on your PATH, or set the git executable path in settings."
                    .to_string()
            }
            AppError::FetchFailed { .. } => {
                "Check your network connection and credentials, then retry. See the activity log for the raw git output."
                    .to_string()
            }
            AppError::Db { .. } => {
                "Restart RepoSync. If the problem persists, the database file may be corrupt or on a syncing folder."
                    .to_string()
            }
        }
    }

    /// Variant-specific detail rendered into the wire `context` field.
    fn context(&self) -> Option<serde_json::Value> {
        match self {
            AppError::PathMissing { path }
            | AppError::NotADirectory { path }
            | AppError::NotARepo { path }
            | AppError::DuplicateRepo { path } => Some(json!({ "path": path })),
            AppError::GitNotFound => None,
            AppError::FetchFailed { exit_code, stderr } => {
                Some(json!({ "exitCode": exit_code, "stderr": stderr }))
            }
            AppError::Db { cause } => Some(json!({ "cause": cause })),
        }
    }

    /// Reduce this error to its frozen wire payload.
    pub fn to_payload(&self) -> AppErrorPayload {
        AppErrorPayload {
            code: self.code().to_string(),
            message: self.to_string(),
            remediation: self.remediation(),
            context: self.context(),
        }
    }

    /// Build a [`AppError::NotARepo`] from a `git2` open failure for `path`.
    pub fn not_a_repo_from_git2(path: &std::path::Path, _err: &git2::Error) -> AppError {
        AppError::NotARepo {
            path: path.display().to_string(),
        }
    }
}

/// Serialize an [`AppError`] as its flat [`AppErrorPayload`] wire shape.
impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.to_payload().serialize(serializer)
    }
}

/// Describe [`AppError`] to `specta` (and therefore `tauri-specta`) as its flat
/// [`AppErrorPayload`] wire shape.
///
/// `AppError` serializes through [`AppErrorPayload`] (the frozen
/// `{ code, message, remediation, context }` object), so its generated
/// TypeScript type must be that payload, not the natural enum shape. Delegating
/// `definition` to [`AppErrorPayload`] keeps the binding and the runtime
/// serialization in lockstep. Without this impl, `Result<T, AppError>` is not a
/// valid `tauri-specta` command return type (it requires the error half to
/// implement `specta::Type`), which is the error half of every fallible IPC
/// command per the E-06 contract.
impl specta::Type for AppError {
    fn definition(types: &mut specta::Types) -> specta::datatype::DataType {
        <AppErrorPayload as specta::Type>::definition(types)
    }
}

impl From<sqlx::Error> for AppError {
    fn from(err: sqlx::Error) -> Self {
        AppError::Db {
            cause: err.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_missing_wire_shape() {
        let err = AppError::PathMissing {
            path: "C:/missing".to_string(),
        };
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "fs.path_missing");
        assert!(value["message"].is_string());
        assert!(value["message"].as_str().unwrap().contains("C:/missing"));
        assert!(value["remediation"].is_string());
        assert!(!value["remediation"].as_str().unwrap().is_empty());
        assert_eq!(value["context"]["path"], "C:/missing");
    }

    #[test]
    fn fetch_failed_context_carries_exit_and_stderr() {
        let err = AppError::FetchFailed {
            exit_code: Some(128),
            stderr: "fatal: not found".to_string(),
        };
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "git.fetch_failed");
        assert_eq!(value["context"]["exitCode"], 128);
        assert_eq!(value["context"]["stderr"], "fatal: not found");
    }

    #[test]
    fn all_seven_codes_unique_and_non_empty() {
        let codes = [
            AppError::PathMissing { path: "p".into() }.code(),
            AppError::NotADirectory { path: "p".into() }.code(),
            AppError::NotARepo { path: "p".into() }.code(),
            AppError::DuplicateRepo { path: "p".into() }.code(),
            AppError::GitNotFound.code(),
            AppError::FetchFailed {
                exit_code: None,
                stderr: String::new(),
            }
            .code(),
            AppError::Db {
                cause: String::new(),
            }
            .code(),
        ];
        assert_eq!(codes.len(), 7);
        for c in codes {
            assert!(!c.is_empty(), "code must be non-empty");
        }
        let mut sorted = codes.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 7, "all seven codes must be unique");
    }
}
