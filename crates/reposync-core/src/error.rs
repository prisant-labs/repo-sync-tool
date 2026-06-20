//! error - owned by E-05 (AppError taxonomy: thiserror + serde + specta::Type).
//!
//! E-05 extends the week-1 tracer subset to the full 30-variant taxonomy; these
//! codes are stable.
//!
//! The wire shape ([`AppErrorPayload`]) is FROZEN: every variant serializes to a
//! flat object `{ code, message, remediation, context }`. The frontend keys off
//! the stable `code` string; `context` carries variant-specific detail as JSON.
//! Expanding the enum does not change the wire shape because every variant
//! funnels through [`AppError::to_payload`].

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

/// The RepoSync error taxonomy.
///
/// Thirty variants grouped by domain (git, fs, net/github, db, config,
/// internal). Each carries a stable [`code`] string and a [`remediation`] hint;
/// the `Display` impl supplies the `message`. The first seven variants are the
/// week-1 tracer subset and are frozen verbatim (identifiers, fields,
/// `#[error(...)]` strings, and codes) because `repo.rs` / `db.rs` construct
/// them and `bindings.ts` froze the wire shape.
///
/// [`code`]: AppError::code
/// [`remediation`]: AppError::remediation
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    // --- git.* (tracer subset) ---
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

    // --- git.* (E-05 additions) ---
    #[error("git is too old: found {found}, requires {required}")]
    GitTooOld { found: String, required: String },

    #[error("branch has no upstream: {branch}")]
    NoUpstream { branch: String },

    #[error("upstream branch is gone: {upstream}")]
    DeletedUpstream { upstream: String },

    #[error("HEAD is detached")]
    DetachedHead,

    #[error("working tree has uncommitted changes: {path}")]
    DirtyTree { path: String },

    #[error("branch cannot fast-forward: {branch}")]
    FfNotPossible { branch: String },

    #[error("git authentication failed")]
    AuthFailed,

    #[error("git command failed (exit {exit_code}): {stderr}")]
    CommandFailed { exit_code: i32, stderr: String },

    // --- fs.* (E-05 additions) ---
    #[error("path is not accessible: {path}")]
    PathNotAccessible { path: String },

    #[error("app data is in a synced folder: {path}")]
    OneDriveSynced { path: String },

    // --- net.* / github.* (E-05 additions) ---
    #[error("no network connection")]
    Offline,

    #[error("the request timed out")]
    Timeout,

    #[error("github rate limit reached (resets at {reset_at})")]
    RateLimited { reset_at: i64 },

    #[error("github repository not found")]
    GithubNotFound,

    #[error("github returned an error (status {status})")]
    GithubApiError { status: u16 },

    #[error("remote is not on github")]
    NotAGithubRemote,

    // --- db.* (E-05 additions) ---
    #[error("database migration failed: {cause}")]
    MigrationFailed { cause: String },

    #[error("the database is locked")]
    DbLocked,

    #[error("entity not found: {entity}")]
    NotFound { entity: String },

    // --- config.* (E-05 additions) ---
    #[error("invalid setting: {field}")]
    InvalidSetting { field: String },

    #[error("invalid update policy: {detail}")]
    InvalidPolicy { detail: String },

    #[error("quiet hours are malformed")]
    QuietHoursMalformed,

    // --- internal.* (E-05 catch-all) ---
    #[error("an unexpected internal error occurred: {context}")]
    Unexpected { context: String },
}

impl AppError {
    /// Stable machine-readable error code. These strings are part of the IPC
    /// contract and must not change.
    pub fn code(&self) -> &'static str {
        match self {
            // git.* (tracer subset)
            AppError::PathMissing { .. } => "fs.path_missing",
            AppError::NotADirectory { .. } => "fs.not_a_directory",
            AppError::NotARepo { .. } => "git.not_a_repo",
            AppError::DuplicateRepo { .. } => "fs.duplicate_repo",
            AppError::GitNotFound => "git.not_found",
            AppError::FetchFailed { .. } => "git.fetch_failed",
            AppError::Db { .. } => "db.query_failed",
            // git.* (E-05 additions)
            AppError::GitTooOld { .. } => "git.too_old",
            AppError::NoUpstream { .. } => "git.no_upstream",
            AppError::DeletedUpstream { .. } => "git.deleted_upstream",
            AppError::DetachedHead => "git.detached_head",
            AppError::DirtyTree { .. } => "git.dirty_tree",
            AppError::FfNotPossible { .. } => "git.ff_not_possible",
            AppError::AuthFailed => "git.auth_failed",
            AppError::CommandFailed { .. } => "git.command_failed",
            // fs.* (E-05 additions)
            AppError::PathNotAccessible { .. } => "fs.path_not_accessible",
            AppError::OneDriveSynced { .. } => "fs.onedrive_synced",
            // net.* / github.* (E-05 additions)
            AppError::Offline => "net.offline",
            AppError::Timeout => "net.timeout",
            AppError::RateLimited { .. } => "github.rate_limited",
            AppError::GithubNotFound => "github.not_found",
            AppError::GithubApiError { .. } => "github.api_error",
            AppError::NotAGithubRemote => "github.not_a_github_remote",
            // db.* (E-05 additions)
            AppError::MigrationFailed { .. } => "db.migration_failed",
            AppError::DbLocked => "db.locked",
            AppError::NotFound { .. } => "db.not_found",
            // config.* (E-05 additions)
            AppError::InvalidSetting { .. } => "config.invalid_setting",
            AppError::InvalidPolicy { .. } => "config.invalid_policy",
            AppError::QuietHoursMalformed => "config.quiet_hours_malformed",
            // internal.* (E-05 catch-all)
            AppError::Unexpected { .. } => "internal.unexpected",
        }
    }

    /// Actionable guidance shown to the user alongside the message.
    pub fn remediation(&self) -> String {
        match self {
            // git.* (tracer subset)
            AppError::PathMissing { .. } => {
                "Check that the folder still exists and the path is spelled correctly.".to_string()
            }
            AppError::NotADirectory { .. } => "Select a folder, not a file.".to_string(),
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
            // git.* (E-05 additions)
            AppError::GitTooOld { .. } => {
                "Your Git is older than the supported minimum (2.30). Update Git.".to_string()
            }
            AppError::NoUpstream { .. } => {
                "This branch has no upstream. Set a tracking branch to check it.".to_string()
            }
            AppError::DeletedUpstream { .. } => {
                "The upstream branch is gone. Update the remote or branch config.".to_string()
            }
            AppError::DetachedHead => {
                "HEAD is detached. Check out a branch to enable updates.".to_string()
            }
            AppError::DirtyTree { .. } => {
                "Uncommitted changes were found. Commit or stash, then retry.".to_string()
            }
            AppError::FfNotPossible { .. } => {
                "The branch has diverged and cannot fast-forward. Resolve manually.".to_string()
            }
            AppError::AuthFailed => {
                "Authentication failed. Refresh your Git credentials and retry.".to_string()
            }
            AppError::CommandFailed { .. } => {
                "A Git command failed. See the activity log for details.".to_string()
            }
            // fs.* (E-05 additions)
            AppError::PathNotAccessible { .. } => {
                "RepoSync cannot access this folder. Check permissions.".to_string()
            }
            AppError::OneDriveSynced { .. } => {
                "App data is in a synced folder, which can corrupt the database. Move it out of OneDrive."
                    .to_string()
            }
            // net.* / github.* (E-05 additions)
            AppError::Offline => {
                "No network connection. RepoSync will retry automatically.".to_string()
            }
            AppError::Timeout => "The request timed out. RepoSync will retry.".to_string(),
            AppError::RateLimited { .. } => {
                "GitHub's rate limit was reached. Enrichment resumes after the reset time."
                    .to_string()
            }
            AppError::GithubNotFound => {
                "The GitHub repository was not found. It may be private or renamed.".to_string()
            }
            AppError::GithubApiError { .. } => {
                "GitHub returned an error. Metadata enrichment was skipped this cycle.".to_string()
            }
            AppError::NotAGithubRemote => {
                "This remote is not on GitHub, so release metadata is unavailable.".to_string()
            }
            // db.* (E-05 additions)
            AppError::MigrationFailed { .. } => {
                "The database could not be upgraded. Your old data was backed up; see the log."
                    .to_string()
            }
            AppError::DbLocked => {
                "The database is busy. Close other RepoSync instances and retry.".to_string()
            }
            AppError::NotFound { .. } => {
                "That item no longer exists. Refresh and try again.".to_string()
            }
            // config.* (E-05 additions)
            AppError::InvalidSetting { .. } => {
                "A setting has an invalid value. Correct it in Settings.".to_string()
            }
            AppError::InvalidPolicy { .. } => {
                "That update mode is not available. Choose a supported mode.".to_string()
            }
            AppError::QuietHoursMalformed => {
                "Quiet hours are misconfigured. Re-enter the time window in Settings.".to_string()
            }
            // internal.* (E-05 catch-all)
            AppError::Unexpected { .. } => {
                "Something went wrong inside RepoSync. Please file an issue with the log."
                    .to_string()
            }
        }
    }

    /// Whether retrying this error may succeed without user action. Derived, not
    /// serialized: true only for transient transport/availability failures.
    pub fn retryable(&self) -> bool {
        matches!(
            self,
            AppError::Offline | AppError::Timeout | AppError::RateLimited { .. } | AppError::DbLocked
        )
    }

    /// Variant-specific detail rendered into the wire `context` field.
    fn context(&self) -> Option<serde_json::Value> {
        match self {
            // path-carrying variants
            AppError::PathMissing { path }
            | AppError::NotADirectory { path }
            | AppError::NotARepo { path }
            | AppError::DuplicateRepo { path }
            | AppError::DirtyTree { path }
            | AppError::PathNotAccessible { path }
            | AppError::OneDriveSynced { path } => Some(json!({ "path": path })),
            // tracer detail
            AppError::FetchFailed { exit_code, stderr } => {
                Some(json!({ "exitCode": exit_code, "stderr": stderr }))
            }
            AppError::Db { cause } => Some(json!({ "cause": cause })),
            // git.* additions
            AppError::GitTooOld { found, required } => {
                Some(json!({ "found": found, "required": required }))
            }
            AppError::NoUpstream { branch } | AppError::FfNotPossible { branch } => {
                Some(json!({ "branch": branch }))
            }
            AppError::DeletedUpstream { upstream } => Some(json!({ "upstream": upstream })),
            AppError::CommandFailed { exit_code, stderr } => {
                Some(json!({ "exitCode": exit_code, "stderr": stderr }))
            }
            // net.* / github.* additions
            AppError::RateLimited { reset_at } => Some(json!({ "resetAt": reset_at })),
            AppError::GithubApiError { status } => Some(json!({ "status": status })),
            // db.* additions
            AppError::MigrationFailed { cause } => Some(json!({ "cause": cause })),
            AppError::NotFound { entity } => Some(json!({ "entity": entity })),
            // config.* additions
            AppError::InvalidSetting { field } => Some(json!({ "field": field })),
            AppError::InvalidPolicy { detail } => Some(json!({ "detail": detail })),
            // internal.* catch-all
            AppError::Unexpected { context } => Some(json!({ "context": context })),
            // no-field variants
            AppError::GitNotFound
            | AppError::DetachedHead
            | AppError::AuthFailed
            | AppError::Offline
            | AppError::Timeout
            | AppError::GithubNotFound
            | AppError::NotAGithubRemote
            | AppError::DbLocked
            | AppError::QuietHoursMalformed => None,
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

    /// One instance of every variant, used by the golden-snapshot and
    /// remediation coverage tests. Keep in sync with the enum.
    fn one_of_each() -> Vec<AppError> {
        vec![
            // git.* (tracer subset)
            AppError::PathMissing { path: "p".into() },
            AppError::NotADirectory { path: "p".into() },
            AppError::NotARepo { path: "p".into() },
            AppError::DuplicateRepo { path: "p".into() },
            AppError::GitNotFound,
            AppError::FetchFailed {
                exit_code: Some(1),
                stderr: "x".into(),
            },
            AppError::Db { cause: "x".into() },
            // git.* (E-05 additions)
            AppError::GitTooOld {
                found: "2.20".into(),
                required: "2.30".into(),
            },
            AppError::NoUpstream {
                branch: "main".into(),
            },
            AppError::DeletedUpstream {
                upstream: "origin/main".into(),
            },
            AppError::DetachedHead,
            AppError::DirtyTree { path: "p".into() },
            AppError::FfNotPossible {
                branch: "main".into(),
            },
            AppError::AuthFailed,
            AppError::CommandFailed {
                exit_code: 1,
                stderr: "x".into(),
            },
            // fs.* (E-05 additions)
            AppError::PathNotAccessible { path: "p".into() },
            AppError::OneDriveSynced { path: "p".into() },
            // net.* / github.* (E-05 additions)
            AppError::Offline,
            AppError::Timeout,
            AppError::RateLimited { reset_at: 1000 },
            AppError::GithubNotFound,
            AppError::GithubApiError { status: 503 },
            AppError::NotAGithubRemote,
            // db.* (E-05 additions)
            AppError::MigrationFailed { cause: "x".into() },
            AppError::DbLocked,
            AppError::NotFound {
                entity: "repo".into(),
            },
            // config.* (E-05 additions)
            AppError::InvalidSetting {
                field: "interval".into(),
            },
            AppError::InvalidPolicy {
                detail: "bad mode".into(),
            },
            AppError::QuietHoursMalformed,
            // internal.* catch-all
            AppError::Unexpected {
                context: "boom".into(),
            },
        ]
    }

    #[test]
    fn golden_snapshot_of_all_codes() {
        // The full, sorted set of every code in the taxonomy. This is the
        // frozen contract: adding or removing a variant must update this array
        // (and the bindings consumers downstream).
        const EXPECTED: &[&str] = &[
            "config.invalid_policy",
            "config.invalid_setting",
            "config.quiet_hours_malformed",
            "db.locked",
            "db.migration_failed",
            "db.not_found",
            "db.query_failed",
            "fs.duplicate_repo",
            "fs.not_a_directory",
            "fs.onedrive_synced",
            "fs.path_missing",
            "fs.path_not_accessible",
            "git.auth_failed",
            "git.command_failed",
            "git.deleted_upstream",
            "git.detached_head",
            "git.dirty_tree",
            "git.fetch_failed",
            "git.ff_not_possible",
            "git.no_upstream",
            "git.not_a_repo",
            "git.not_found",
            "git.too_old",
            "github.api_error",
            "github.not_a_github_remote",
            "github.not_found",
            "github.rate_limited",
            "internal.unexpected",
            "net.offline",
            "net.timeout",
        ];

        let variants = one_of_each();
        assert_eq!(variants.len(), 30, "there must be exactly 30 variants");

        let mut codes: Vec<&str> = variants.iter().map(|e| e.code()).collect();

        // Every code is non-empty and namespaced (contains a '.').
        for c in &codes {
            assert!(!c.is_empty(), "code must be non-empty");
            assert!(c.contains('.'), "code must be namespaced: {c}");
        }

        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 30, "all 30 codes must be unique");

        assert_eq!(
            codes.as_slice(),
            EXPECTED,
            "the sorted code set must match the frozen golden snapshot"
        );
    }

    #[test]
    fn every_variant_has_non_empty_remediation() {
        for err in one_of_each() {
            assert!(
                !err.remediation().is_empty(),
                "remediation must be non-empty for {}",
                err.code()
            );
        }
    }

    #[test]
    fn retryable_truth_table() {
        // The four transient variants are retryable.
        assert!(AppError::Offline.retryable());
        assert!(AppError::Timeout.retryable());
        assert!(AppError::RateLimited { reset_at: 0 }.retryable());
        assert!(AppError::DbLocked.retryable());

        // A sample of terminal variants are not.
        assert!(!AppError::PathMissing { path: "p".into() }.retryable());
        assert!(!AppError::GitNotFound.retryable());
        assert!(!AppError::AuthFailed.retryable());
        assert!(!AppError::GithubNotFound.retryable());
        assert!(
            !AppError::Db {
                cause: "x".into()
            }
            .retryable()
        );
        assert!(
            !AppError::Unexpected {
                context: "x".into()
            }
            .retryable()
        );
    }

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
    fn git_too_old_round_trip() {
        let err = AppError::GitTooOld {
            found: "2.20".to_string(),
            required: "2.30".to_string(),
        };
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "git.too_old");
        assert!(value["message"].as_str().unwrap().contains("2.20"));
        assert!(!value["remediation"].as_str().unwrap().is_empty());
        assert_eq!(value["context"]["found"], "2.20");
        assert_eq!(value["context"]["required"], "2.30");
    }

    #[test]
    fn rate_limited_round_trip() {
        let err = AppError::RateLimited { reset_at: 1_700_000_000 };
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "github.rate_limited");
        assert!(value["message"].is_string());
        assert!(!value["remediation"].as_str().unwrap().is_empty());
        assert_eq!(value["context"]["resetAt"], 1_700_000_000_i64);
    }

    #[test]
    fn github_api_error_round_trip() {
        let err = AppError::GithubApiError { status: 503 };
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "github.api_error");
        assert!(value["message"].is_string());
        assert!(!value["remediation"].as_str().unwrap().is_empty());
        assert_eq!(value["context"]["status"], 503);
    }

    #[test]
    fn unexpected_round_trip() {
        let err = AppError::Unexpected {
            context: "panic in worker".to_string(),
        };
        let value = serde_json::to_value(&err).expect("serialize");
        assert_eq!(value["code"], "internal.unexpected");
        assert!(value["message"].is_string());
        assert!(!value["remediation"].as_str().unwrap().is_empty());
        assert_eq!(value["context"]["context"], "panic in worker");
    }
}
