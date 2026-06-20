//! ipc - owned by E-06 (IPC payload structs shared with the Tauri shell).
//!
//! Tracer slice: the thin payload types the check flow returns/emits. These
//! derive serde + specta::Type ONLY - this module (and the whole crate) must
//! never import tauri or tauri-*. The full payload surface (RepoSummary,
//! RepoDetail, ...) lands in E-06.

use serde::{Deserialize, Serialize};

/// Stable identifier for a tracked repo (its `repos.id`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub struct RepoId(pub i64);

/// Result of a "check now" run, returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub repo_id: i64,
    pub decision: String,
    pub reason: Option<String>,
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
    pub is_dirty: bool,
    pub is_detached: bool,
    pub checked_at: i64,
}

/// Event payload emitted when a check completes (the slimmer broadcast form).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckCompletedPayload {
    pub repo_id: i64,
    pub decision: String,
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
    pub checked_at: i64,
}
