//! Tauri IPC command handlers for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) for the stub; E-06 (IPC contract) for the
//! typed payloads; E-12 (tracer bullet) wires the first two real commands.
//!
//! Each `#[tauri::command]` here is a thin adapter: it pulls the shared pool and
//! git engine out of managed [`AppState`](crate::AppState), calls into
//! `reposync-core`, and returns the core's typed result/error verbatim. No
//! product logic lives here - the shell only crosses the IPC boundary.

use reposync_core::error::AppError;
use reposync_core::ipc::{CheckResult, RepoId};

use crate::events::emit_check_completed;
use crate::AppState;

/// Add a repository to the registry by absolute local path.
///
/// Thin wrapper over [`reposync_core::repo::add`]: validates + inspects + writes
/// the `repos` / `repo_local_state` rows and returns the new [`RepoId`].
#[tauri::command]
#[specta::specta]
pub async fn repo_add_path(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<RepoId, AppError> {
    let git = state.git.as_ref().ok_or(AppError::GitNotFound)?;
    reposync_core::repo::add(&state.pool, git, std::path::Path::new(&path)).await
}

/// Run a "check now" for a tracked repo, then broadcast the result.
///
/// Calls [`reposync_core::repo::check_now`], emits the `repo:check-completed`
/// event so every window's listener sees the outcome, and returns the full
/// [`CheckResult`] to the caller.
#[tauri::command]
#[specta::specta]
pub async fn repo_check_now(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<CheckResult, AppError> {
    let git = state.git.as_ref().ok_or(AppError::GitNotFound)?;
    let result = reposync_core::repo::check_now(&state.pool, git, RepoId(id)).await?;
    emit_check_completed(&app, &result);
    Ok(result)
}
