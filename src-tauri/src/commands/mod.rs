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
use reposync_core::ipc::{
    ActivityFilter, ActivityRecord, CheckResult, DailySummary, RepoDetail, RepoFilter, RepoId,
    RepoSummary, ScanResult, Settings, UpdateMode, UpdatePolicy, UpdateResult, WeeklySummary,
};

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

// =============================================================================
// E-06 contract stubs.
//
// These freeze the full IPC command surface NOW so the generated bindings and
// the typecheck gate cover every command the V1 efforts will fill in. Each body
// returns `AppError::Unexpected { context: "not yet implemented" }` rather than
// `unimplemented!()` / `todo!()`: a panic inside a Tauri command poisons the IPC
// channel for that invoke and clippy flags the macro under `-D warnings`. The
// `// TODO(E-0x):` on each body names the effort that replaces the stub.
//
// `_state` / `_app` are injected by Tauri and are NOT part of the TypeScript
// surface; only the camelCase value params (filter, id, policy, ...) appear in
// the generated bindings. The injected args are underscore-prefixed because the
// stub bodies do not touch them yet.
// =============================================================================

/// Stub: typed error returned by every not-yet-implemented command body.
fn not_implemented() -> AppError {
    AppError::Unexpected {
        context: "not yet implemented".into(),
    }
}

/// List tracked repos (summary view), filtered.
#[tauri::command]
#[specta::specta]
pub async fn repo_list(
    _state: tauri::State<'_, AppState>,
    filter: RepoFilter,
) -> Result<Vec<RepoSummary>, AppError> {
    // TODO(E-02): query repos + repo_local_state and apply `filter`.
    let _ = filter;
    Err(not_implemented())
}

/// Get the full detail of a single tracked repo.
#[tauri::command]
#[specta::specta]
pub async fn repo_get(_state: tauri::State<'_, AppState>, id: i64) -> Result<RepoDetail, AppError> {
    // TODO(E-02): join repos + repo_local_state + repo_remote_meta for `id`.
    let _ = id;
    Err(not_implemented())
}

/// Scan a parent folder for candidate git repositories.
#[tauri::command]
#[specta::specta]
pub async fn repo_scan_parent(
    _state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ScanResult, AppError> {
    // TODO(E-02/E-03): walk `path` and report discovered repos.
    let _ = path;
    Err(not_implemented())
}

/// Remove a tracked repo (does not touch the working tree).
#[tauri::command]
#[specta::specta]
pub async fn repo_remove(_state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    // TODO(E-02): delete the repos row (cascades local_state / remote_meta).
    let _ = id;
    Err(not_implemented())
}

/// Enable or disable scheduled checks for a repo.
#[tauri::command]
#[specta::specta]
pub async fn repo_set_enabled(
    _state: tauri::State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), AppError> {
    // TODO(E-02): flip the `enabled` flag for `id`.
    let _ = (id, enabled);
    Err(not_implemented())
}

/// Set the per-repo update policy.
#[tauri::command]
#[specta::specta]
pub async fn repo_set_policy(
    _state: tauri::State<'_, AppState>,
    id: i64,
    policy: UpdatePolicy,
) -> Result<(), AppError> {
    // TODO(E-07): persist the update/dirty/branch policy for `id`.
    let _ = (id, policy);
    Err(not_implemented())
}

/// Run an "update now" for a repo in the given mode.
#[tauri::command]
#[specta::specta]
pub async fn repo_update_now(
    _app: tauri::AppHandle,
    _state: tauri::State<'_, AppState>,
    id: i64,
    mode: UpdateMode,
) -> Result<UpdateResult, AppError> {
    // TODO(E-07): perform the update and emit update-started/completed.
    let _ = (id, mode);
    Err(not_implemented())
}

/// Refresh GitHub / remote metadata for a repo.
#[tauri::command]
#[specta::specta]
pub async fn repo_refresh_metadata(
    _state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<RepoDetail, AppError> {
    // TODO(E-10): fetch remote meta (release, topics, archived) for `id`.
    let _ = id;
    Err(not_implemented())
}

/// Open the repo's folder in the OS file manager.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_folder(_state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    // TODO(E-03): resolve `id`'s local_path and reveal it via the shell.
    let _ = id;
    Err(not_implemented())
}

/// Open the repo in a terminal.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_terminal(
    _state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), AppError> {
    // TODO(E-03): launch the configured terminal at `id`'s local_path.
    let _ = id;
    Err(not_implemented())
}

/// Open the repo in the configured editor.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_editor(_state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    // TODO(E-03): launch the configured editor at `id`'s local_path.
    let _ = id;
    Err(not_implemented())
}

/// Open the repo's remote (origin URL) in the browser.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_remote(_state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    // TODO(E-03): open `id`'s remote_origin_url in the default browser.
    let _ = id;
    Err(not_implemented())
}

/// List activity-log records, filtered.
#[tauri::command]
#[specta::specta]
pub async fn activity_list(
    _state: tauri::State<'_, AppState>,
    filter: ActivityFilter,
) -> Result<Vec<ActivityRecord>, AppError> {
    // TODO(E-09): query activity_records and apply `filter`.
    let _ = filter;
    Err(not_implemented())
}

/// Get today's daily summary.
#[tauri::command]
#[specta::specta]
pub async fn summary_today(_state: tauri::State<'_, AppState>) -> Result<DailySummary, AppError> {
    // TODO(E-11): compute (or read the cached) daily summary.
    Err(not_implemented())
}

/// Get the current week's summary (V1.1 stub).
#[tauri::command]
#[specta::specta]
pub async fn summary_week(_state: tauri::State<'_, AppState>) -> Result<WeeklySummary, AppError> {
    // TODO(E-11/V1.1): compute the weekly roll-up.
    Err(not_implemented())
}

/// Read the settings singleton.
#[tauri::command]
#[specta::specta]
pub async fn settings_get(_state: tauri::State<'_, AppState>) -> Result<Settings, AppError> {
    // TODO(E-02): read the settings row.
    Err(not_implemented())
}

/// Write the settings singleton.
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    _state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), AppError> {
    // TODO(E-02): validate and persist `settings`.
    let _ = settings;
    Err(not_implemented())
}
