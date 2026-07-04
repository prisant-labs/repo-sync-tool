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
    // Clone the engine OUT of the read lock and drop the guard immediately, so a
    // long-running git operation never holds the lock against a `settings_set`
    // re-probe (BL-NI-19). The engine is cheap to clone (it wraps shared handles).
    let git = { state.git.read().await.clone() }.ok_or(AppError::GitNotFound)?;
    reposync_core::repo::add(&state.pool, &git, std::path::Path::new(&path)).await
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
    // Clone the engine OUT of the read lock and drop the guard immediately, so a
    // long-running git operation never holds the lock against a `settings_set`
    // re-probe (BL-NI-19). The engine is cheap to clone (it wraps shared handles).
    let git = { state.git.read().await.clone() }.ok_or(AppError::GitNotFound)?;
    // Serialize with any scheduled job on the same repo via the shared per-repo
    // lock: hold it across the whole check so a manual and a scheduled git op
    // never run two `git` processes in one working tree at once.
    let _lock = state.locks.lock_handle(RepoId(id)).lock_owned().await;
    let result = reposync_core::repo::check_now(&state.pool, &git, RepoId(id)).await?;
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
    // Clone the engine OUT of the read lock and drop the guard immediately, so a
    // long-running git operation never holds the lock against a `settings_set`
    // re-probe (BL-NI-19). The engine is cheap to clone (it wraps shared handles).
    let git = { state.git.read().await.clone() }.ok_or(AppError::GitNotFound)?;
    reposync_core::store::repo_scan_parent(&state.pool, &git, std::path::Path::new(&path)).await
}

/// Remove a tracked repo (does not touch the working tree).
#[tauri::command]
#[specta::specta]
pub async fn repo_remove(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    // Hold the per-repo lock across the delete so a scheduled job on this repo
    // cannot race the removal, then evict the now-dead lock entry.
    let _lock = state.locks.lock_handle(RepoId(id)).lock_owned().await;
    reposync_core::store::repo_remove(&state.pool, RepoId(id)).await?;
    state.locks.remove(RepoId(id));
    Ok(())
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
    // Clone the engine OUT of the read lock and drop the guard immediately, so a
    // long-running git operation never holds the lock against a `settings_set`
    // re-probe (BL-NI-19). The engine is cheap to clone (it wraps shared handles).
    let git = { state.git.read().await.clone() }.ok_or(AppError::GitNotFound)?;
    // Serialize with any scheduled job on the same repo via the shared per-repo
    // lock, held across the entire update (started -> execute -> completed).
    let _lock = state.locks.lock_handle(RepoId(id)).lock_owned().await;
    // The started event carries the requested mode label (snake_case wire form).
    emit_update_started(&app, id, update_mode_label(&mode));
    let result = reposync_core::repo::update_now(&state.pool, &git, RepoId(id), mode).await?;
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

/// Map a [`RefreshReport`](reposync_core::github::RefreshReport)'s engine-level outcome
/// to an [`AppError`], or `None` when the refresh succeeded (the command then re-reads
/// and returns the updated detail).
///
/// The engine returns network failures as outcome VALUES, not errors; the E-05 wrapping
/// happens here at the edge. `Skipped` (a non-GitHub repo) is treated as success - the
/// command returns the unchanged detail. `RateLimited` carries the parsed reset time, so
/// the error is honest. Pure, so it is unit-tested below.
fn refresh_report_error(
    report: &reposync_core::github::RefreshReport,
    repo_id: i64,
) -> Option<AppError> {
    use reposync_core::github::RefreshOutcome;
    match report.outcome {
        // Refreshed, served from cache, still-current, or a non-GitHub skip: success.
        RefreshOutcome::Cached
        | RefreshOutcome::Updated
        | RefreshOutcome::NotModified
        | RefreshOutcome::Skipped => None,
        RefreshOutcome::NetworkLost => Some(AppError::Offline),
        RefreshOutcome::NotFound => Some(AppError::NotFound {
            entity: format!("GitHub repository for repo {repo_id}"),
        }),
        // The budget (with the parsed reset) rides along on the rate-limited outcome;
        // fall back to 0 ("unknown") only if it is somehow absent.
        RefreshOutcome::RateLimited => Some(AppError::RateLimited {
            reset_at: report.rate_limit.map(|r| r.reset_at).unwrap_or(0),
        }),
    }
}

/// Refresh GitHub / remote metadata for a repo, then return the updated detail.
///
/// Thin edge over [`reposync_core::github::refresh_one`] on the unauthenticated V1 path
/// (`NoToken`): fetch + persist, map any engine failure to an [`AppError`]
/// ([`refresh_report_error`]), then re-read the [`RepoDetail`]. A MANUAL refresh fetches
/// unconditionally, so the deferred release-cadence caveat (BL-NI-15b) does not apply.
#[tauri::command]
#[specta::specta]
pub async fn repo_refresh_metadata(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<RepoDetail, AppError> {
    let transport = reposync_core::github::ReqwestTransport::new()?;
    let report = reposync_core::github::refresh_one(
        &state.pool,
        &transport,
        &reposync_core::github::NoToken,
        id,
        crate::localtime::now_unix(),
    )
    .await?;
    if let Some(err) = refresh_report_error(&report, id) {
        return Err(err);
    }
    reposync_core::store::repo_get(&state.pool, RepoId(id)).await
}

/// Open the repo's folder in the OS file manager.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_folder(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    let detail = reposync_core::store::repo_get(&state.pool, RepoId(id)).await?;
    crate::opener::open_folder(std::path::Path::new(&detail.local_path))
}

/// Open the repo in a terminal.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_terminal(
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), AppError> {
    let detail = reposync_core::store::repo_get(&state.pool, RepoId(id)).await?;
    let settings = reposync_core::store::settings_get(&state.pool).await?;
    let terminal = settings.terminal_command.ok_or(AppError::InvalidSetting {
        field: "terminal_command".into(),
    })?;
    crate::opener::open_terminal(&terminal, std::path::Path::new(&detail.local_path))
}

/// Open the repo in the configured editor.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_editor(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    let detail = reposync_core::store::repo_get(&state.pool, RepoId(id)).await?;
    let settings = reposync_core::store::settings_get(&state.pool).await?;
    let editor = settings.editor_command.ok_or(AppError::InvalidSetting {
        field: "editor_command".into(),
    })?;
    crate::opener::open_editor(&editor, std::path::Path::new(&detail.local_path))
}

/// Open the repo's remote (origin URL) in the browser.
#[tauri::command]
#[specta::specta]
pub async fn repo_open_remote(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    let detail = reposync_core::store::repo_get(&state.pool, RepoId(id)).await?;
    let url = detail.remote_origin_url.ok_or_else(|| AppError::NotFound {
        entity: format!("remote origin URL for repo {id}"),
    })?;
    crate::opener::open_url(&url)
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
    reposync_core::store::settings_set(&state.pool, &settings).await?;

    // Live git re-probe (BL-NI-19): once the new settings are persisted, rebuild
    // the git engine from the newly-saved `git_executable_path` and swap the
    // shared engine, so a user who fixes a broken/missing git path recovers
    // WITHOUT restarting - the command path picks up the new engine immediately.
    // Re-read the persisted settings so this mirrors the startup construction
    // EXACTLY (same source, same infallible `new`, same unavailable-check). The
    // resident scheduler keeps its own initial engine and only picks up the
    // re-probe on restart - a known limitation (see `AppState` / setup notes).
    let configured_git_path = reposync_core::store::settings_get(&state.pool)
        .await
        .ok()
        .and_then(|s| s.git_executable_path);
    let engine = reposync_core::git::SystemGitEngine::new(configured_git_path);
    let next = if engine.availability().is_unavailable() {
        None
    } else {
        Some(engine)
    };
    *state.git.write().await = next;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use reposync_core::github::{RateLimit, RefreshOutcome, RefreshReport};

    fn report(outcome: RefreshOutcome, rate_limit: Option<RateLimit>) -> RefreshReport {
        RefreshReport {
            outcome,
            rate_limit,
            release_stale: false,
        }
    }

    #[test]
    fn refresh_report_error_maps_engine_outcomes_to_apperror() {
        // Success-ish outcomes carry no error: the command re-reads + returns the detail.
        for ok in [
            RefreshOutcome::Cached,
            RefreshOutcome::Updated,
            RefreshOutcome::NotModified,
            RefreshOutcome::Skipped,
        ] {
            assert!(
                refresh_report_error(&report(ok, None), 7).is_none(),
                "{ok:?} is not an error"
            );
        }

        // Engine failures map to typed AppErrors (E-05 wrapping at the edge).
        assert!(matches!(
            refresh_report_error(&report(RefreshOutcome::NetworkLost, None), 7),
            Some(AppError::Offline)
        ));
        assert!(matches!(
            refresh_report_error(&report(RefreshOutcome::NotFound, None), 7),
            Some(AppError::NotFound { .. })
        ));

        // RateLimited carries the parsed reset time through to an honest error.
        let rl = RateLimit {
            remaining: 0,
            limit: 60,
            reset_at: 1_700_000_000,
        };
        assert!(matches!(
            refresh_report_error(&report(RefreshOutcome::RateLimited, Some(rl)), 7),
            Some(AppError::RateLimited {
                reset_at: 1_700_000_000
            })
        ));
    }
}
