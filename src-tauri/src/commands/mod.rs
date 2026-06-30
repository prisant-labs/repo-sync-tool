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

use crate::events::{emit_check_completed, emit_update_completed, emit_update_started};
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
    state: tauri::State<'_, AppState>,
    filter: RepoFilter,
) -> Result<Vec<RepoSummary>, AppError> {
    reposync_core::store::repo_list(&state.pool, &filter).await
}

/// Get the full detail of a single tracked repo.
#[tauri::command]
#[specta::specta]
pub async fn repo_get(state: tauri::State<'_, AppState>, id: i64) -> Result<RepoDetail, AppError> {
    reposync_core::store::repo_get(&state.pool, RepoId(id)).await
}

/// Scan a parent folder for candidate git repositories.
#[tauri::command]
#[specta::specta]
pub async fn repo_scan_parent(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<ScanResult, AppError> {
    let git = state.git.as_ref().ok_or(AppError::GitNotFound)?;
    reposync_core::store::repo_scan_parent(&state.pool, git, std::path::Path::new(&path)).await
}

/// Remove a tracked repo (does not touch the working tree).
#[tauri::command]
#[specta::specta]
pub async fn repo_remove(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    reposync_core::store::repo_remove(&state.pool, RepoId(id)).await
}

/// Enable or disable scheduled checks for a repo.
#[tauri::command]
#[specta::specta]
pub async fn repo_set_enabled(
    state: tauri::State<'_, AppState>,
    id: i64,
    enabled: bool,
) -> Result<(), AppError> {
    reposync_core::store::repo_set_enabled(&state.pool, RepoId(id), enabled).await
}

/// Set the per-repo update policy (E-07).
///
/// Thin wrapper over [`reposync_core::store::repo_set_policy`]: persists the
/// repo's `update_mode`, rejecting a non-V1 mode at the boundary.
#[tauri::command]
#[specta::specta]
pub async fn repo_set_policy(
    state: tauri::State<'_, AppState>,
    id: i64,
    policy: UpdatePolicy,
) -> Result<(), AppError> {
    reposync_core::store::repo_set_policy(&state.pool, RepoId(id), &policy).await
}

/// Run an "update now" for a repo in the given mode (E-07).
///
/// Emits `repo:update-started` before the run, calls the shared
/// [`reposync_core::repo::update_now`] decide -> execute -> record path (the same
/// path the E-08 scheduler reuses), then emits `repo:update-completed` with the
/// outcome and returns the full [`UpdateResult`].
#[tauri::command]
#[specta::specta]
pub async fn repo_update_now(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
    mode: UpdateMode,
) -> Result<UpdateResult, AppError> {
    let git = state.git.as_ref().ok_or(AppError::GitNotFound)?;
    // The started event carries the requested mode label (snake_case wire form).
    emit_update_started(&app, id, update_mode_label(&mode));
    let result = reposync_core::repo::update_now(&state.pool, git, RepoId(id), mode).await?;
    emit_update_completed(&app, id, &result.outcome);
    Ok(result)
}

/// The snake_case label for an [`UpdateMode`], for the `update-started` event's
/// `mode` field (the shell does not import the core's private helper).
fn update_mode_label(mode: &UpdateMode) -> &'static str {
    match mode {
        UpdateMode::CheckOnly => "check_only",
        UpdateMode::FetchOnly => "fetch_only",
        UpdateMode::PullFfOnly => "pull_ff_only",
        UpdateMode::PullStandard => "pull_standard",
        UpdateMode::PullRebase => "pull_rebase",
    }
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

/// List activity-log records, filtered (newest first).
///
/// Thin wrapper over [`reposync_core::activity::list`]: the read-side counterpart
/// to the E-09 writer, returning the filtered audit trail for the activity-timeline
/// UI. The core clamps the row limit so a UI read can never pull the whole log.
#[tauri::command]
#[specta::specta]
pub async fn activity_list(
    state: tauri::State<'_, AppState>,
    filter: ActivityFilter,
) -> Result<Vec<ActivityRecord>, AppError> {
    reposync_core::activity::list(&state.pool, &filter).await
}

/// Get today's daily summary (for the user's local day).
///
/// Thin wrapper over [`reposync_core::summary::summary_today`]: the edge supplies the
/// local-day window ([`crate::localtime::local_day_window`]) because reposync-core is
/// timezone-free, then the core aggregates the day's activity + state read-only.
#[tauri::command]
#[specta::specta]
pub async fn summary_today(state: tauri::State<'_, AppState>) -> Result<DailySummary, AppError> {
    let window = crate::localtime::local_day_window();
    reposync_core::summary::summary_today(&state.pool, &window).await
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
pub async fn settings_get(state: tauri::State<'_, AppState>) -> Result<Settings, AppError> {
    reposync_core::store::settings_get(&state.pool).await
}

/// Write the settings singleton.
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), AppError> {
    reposync_core::store::settings_set(&state.pool, &settings).await
}
