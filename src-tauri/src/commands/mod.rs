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
    ActivityFilter, ActivityRecord, CheckResult, DailySummary, DbRecoveryNotice, GroupSummary,
    RepoDetail, RepoFilter, RepoGroupMembership, RepoId, RepoSummary, ScanResult, Settings,
    UpdateAvailability, UpdateMode, UpdatePolicy, UpdateResult, WeeklySummary,
};
use reposync_core::notify::{NoteKind, NotifiableEvent};
use reposync_core::scheduler::{RepoLocks, SharedGitEngine};
use sqlx::SqlitePool;

use crate::events::{
    emit_check_completed, emit_check_started, emit_error_raised, emit_update_completed,
    emit_update_started,
};
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
    // Announce the check start (BL-NI-31) before the git work runs, then broadcast
    // the completion after.
    emit_check_started(&app, id);
    let result = reposync_core::repo::check_now(&state.pool, &git, RepoId(id)).await?;
    emit_check_completed(&app, &result);
    Ok(result)
}

/// Run a "check now" over every ENABLED repo (E-13 tray "Check All Now").
///
/// The additive E-13 backend command behind the tray "Check All Now" item (also
/// callable from the frontend). Selects the enabled repos (the pure
/// [`reposync_core::store::select_check_all_targets`]) and runs each through the
/// SAME per-repo lock the scheduler uses, so a tray check-all and a scheduled check
/// never launch two `git` processes in one working tree. Returns the number of repos
/// checked. Per-repo events fire like a manual check (`check-started` /
/// `check-completed`); a per-repo failure is surfaced via `error:raised` (the tray
/// action is fire-and-forget, so there is no synchronous caller to receive it) and
/// does not abort the run.
#[tauri::command]
#[specta::specta]
pub async fn repo_check_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<u32, AppError> {
    check_all_enabled(&app, &state.pool, &state.git, &state.locks).await
}

/// Shared "check all enabled repos" implementation, called by [`repo_check_all`] and
/// directly by the tray menu handler. Resolves git ONCE up front (a check-all with no
/// usable git is a single clear `GitNotFound`, not N repeats) and reuses that engine
/// for every repo in the burst, mirroring how a scheduler tick pins the engine it
/// resolved at tick start. Each repo is checked under its shared per-repo lock so the
/// burst serializes against the scheduler per repo.
pub(crate) async fn check_all_enabled(
    app: &tauri::AppHandle,
    pool: &SqlitePool,
    git: &SharedGitEngine,
    locks: &RepoLocks,
) -> Result<u32, AppError> {
    // Resolve the live engine once (cloned out of the read lock, guard dropped
    // immediately, per BL-NI-19); a check-all with no git is one clear error.
    let git = { git.read().await.clone() }.ok_or(AppError::GitNotFound)?;

    let flags = reposync_core::store::repo_enabled_flags(pool).await?;
    let targets = reposync_core::store::select_check_all_targets(&flags);

    let mut checked = 0u32;
    for id in targets {
        // Serialize per repo against the scheduler via the SAME per-repo lock.
        let _lock = locks.lock_handle(RepoId(id)).lock_owned().await;
        emit_check_started(app, id);
        match reposync_core::repo::check_now(pool, &git, RepoId(id)).await {
            Ok(result) => {
                emit_check_completed(app, &result);
                checked += 1;
            }
            // A single repo's failure must not abort the whole check-all; surface it
            // on the global error event (no synchronous caller to receive it) and move
            // on to the next repo.
            Err(e) => emit_error_raised(app, &e),
        }
    }
    Ok(checked)
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

/// Set a repo's per-repo check cadence (BL-NI-30 / finding 15).
///
/// Additive E-06 amendment (a new command, not a change to `repo_set_policy`, which
/// carries only mode + dirty-handling). `checkFrequencyMin` follows the inherit
/// model: `0` inherits the global cadence (`settings.global_check_minutes`), a
/// POSITIVE value is an explicit per-repo override in minutes. Persists the new
/// cadence via the store, then recomputes this repo's `next_check_at` with the SAME
/// anchored rule the global-cadence change uses
/// ([`reposync_core::scheduler::reschedule_one_repo`]), so a shorter override - or a
/// switch back to inherit - takes effect immediately instead of waiting out the
/// stale schedule. A negative value is rejected (`InvalidSetting`); a missing repo
/// is `NotFound`.
#[tauri::command]
#[specta::specta]
pub async fn repo_set_cadence(
    state: tauri::State<'_, AppState>,
    id: i64,
    check_frequency_min: i64,
) -> Result<(), AppError> {
    reposync_core::store::repo_set_cadence(&state.pool, RepoId(id), check_frequency_min).await?;
    // The cadence is persisted; re-anchor next_check_at so the change is live. A
    // reschedule failure after a successful write is surfaced (the cadence still
    // stands; the next scheduler completion would re-anchor it anyway).
    reposync_core::scheduler::reschedule_one_repo(&state.pool, id, crate::localtime::now_unix())
        .await?;
    Ok(())
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
//
// E-14: when the refresh brings in a genuinely NEW upstream release (the release tag is
// now present and differs from what was cached), raise ONE release toast (gated by
// notify_on_release + quiet hours inside the core's `decide`). This is the manual
// release-notification trigger; background scheduled cycles notify only on failures
// because the scheduled path is a git fetch/pull, not a GitHub release refresh. The
// detailed rationale is a `//` (non-doc) comment on purpose - like `settings_set`'s - so
// it does not bloat the tauri-specta-generated `repoRefreshMetadata` JSDoc; the injected
// `app` is not part of the TypeScript surface, so the IPC binding shape is unchanged.
#[tauri::command]
#[specta::specta]
pub async fn repo_refresh_metadata(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<RepoDetail, AppError> {
    // Snapshot the cached release tag BEFORE the refresh so a genuinely new release
    // can be told from an unchanged one (best-effort: a failed pre-read just means
    // "unknown", and any newly-present tag is then treated as first-seen).
    let before = reposync_core::store::repo_get(&state.pool, RepoId(id))
        .await
        .ok()
        .and_then(|d| d.latest_release_tag);

    let transport = reposync_core::github::ReqwestTransport::new()?;
    // Finding 2: route the manual refresh through the SAME budgeted entry point the
    // background pass uses, spending against the ONE shared budget in AppState, so a
    // manual refresh can never race the background pass into overspending the ceiling.
    // Finding 1: `force = true` re-checks EVERY resource (repo + release + PR) regardless
    // of any window, so a user Refresh always re-fetches.
    let refreshed = reposync_core::github::refresh_one_budgeted(
        &state.pool,
        &transport,
        &reposync_core::github::NoToken,
        &state.github_budget,
        id,
        crate::localtime::now_unix(),
        true,
    )
    .await?;
    let report = match refreshed {
        // Budget exhausted: do NOT spend over the ceiling. Return the last-known detail,
        // which the drawer renders with its "as of <time>" staleness marker - budget
        // exhaustion is graceful degradation, never an error state (E-17 In scope (b)).
        reposync_core::github::BudgetedRefresh::BudgetExhausted => {
            return reposync_core::store::repo_get(&state.pool, RepoId(id)).await;
        }
        reposync_core::github::BudgetedRefresh::Refreshed(report) => report,
    };
    if let Some(err) = refresh_report_error(&report, id) {
        return Err(err);
    }
    let detail = reposync_core::store::repo_get(&state.pool, RepoId(id)).await?;

    // Fire a release toast only when the tag actually changed to a new value.
    if let Some(new_tag) =
        crate::notify::release_change(before.as_deref(), detail.latest_release_tag.as_deref())
    {
        if let Ok(settings) = reposync_core::store::settings_get(&state.pool).await {
            crate::notify::fire_one(
                &app,
                &settings,
                &NotifiableEvent {
                    kind: NoteKind::Release,
                    repo_id: id,
                    repo_name: detail.local_name.clone(),
                    detail: Some(new_tag.to_string()),
                },
            );
        }
    }

    Ok(detail)
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
    // The raw `.git/config` URL is attacker-controlled, so `open_remote`
    // validates/translates it (ssh -> https, reject file://, local/UNC paths)
    // before it can reach the OS launcher (BL-NI-24 finding 2).
    crate::opener::open_remote(&url)
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
//
// After persisting, reconcile the live scheduler cadence and git engine to the
// new settings. The whole sequence (persist -> reschedule -> re-probe -> swap)
// runs under the `settings_write_lock` single-flight guard (BL-NI-35) so two
// overlapping saves cannot interleave and leave the live engine reflecting older
// settings than the database. (The one-line `///` doc above is intentional: it is
// what tauri-specta emits as the `settingsSet` JSDoc, and the IPC contract - name,
// args, return - is unchanged by this behavior, so `bindings.ts` does not drift.)
#[tauri::command]
#[specta::specta]
pub async fn settings_set(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    settings: Settings,
) -> Result<(), AppError> {
    // Serialize the whole persist/reschedule/probe/swap sequence (BL-NI-35).
    let _write = state.settings_write_lock.lock().await;

    // Read the prior settings BEFORE persisting so we can tell what actually
    // changed and only reconcile the affected live subsystem: the global cadence
    // (below, re-cadence repos) and the `autostart` setting (E-15, actuate the
    // plugin). `app` is Tauri-injected, so it does NOT appear in the generated
    // TypeScript binding (`settingsSet(settings)` is unchanged).
    let previous = reposync_core::store::settings_get(&state.pool).await.ok();
    let previous_global = previous.as_ref().map(|s| s.global_check_minutes);
    let previous_autostart = previous.as_ref().map(|s| s.autostart);
    let previous_git_path = previous
        .as_ref()
        .and_then(|s| s.git_executable_path.clone());

    reposync_core::store::settings_set(&state.pool, &settings).await?;

    // Finding 1: only reconcile the LIVE git engine when `git_executable_path`
    // ACTUALLY changed from the previously persisted value. The git re-probe, the
    // autostart actuation, and the inherit-cadence reschedule are INDEPENDENT
    // subsystems; a machine with no git - or any save that does not touch the git
    // path - must not re-probe and reject, which previously (the BL-NI-26 early
    // return plus the Phase 3 autostart wiring landing AFTER it) made EVERY save on a
    // git-less machine skip the autostart actuation and falsely report
    // InvalidSetting for an unrelated notify/quiet-hours/autostart change. When the
    // path is unchanged the last-known-working engine stands untouched.
    let git = if settings.git_executable_path != previous_git_path {
        // The path changed: rebuild the engine from the newly-saved path. Re-read the
        // persisted settings so this mirrors the startup construction EXACTLY (same
        // source, same infallible `new`).
        let configured_git_path = reposync_core::store::settings_get(&state.pool)
            .await
            .ok()
            .and_then(|s| s.git_executable_path);
        let engine = reposync_core::git::SystemGitEngine::new(configured_git_path);
        // BL-NI-26 / finding 5: a changed path that resolves to no usable git keeps
        // the last-known-working engine (DO NOT swap to None) and is reported as a
        // structured InvalidSetting so the UI toasts honestly instead of a false
        // "Settings saved". A usable git is swapped in live so both the command path
        // and the resident scheduler (same shared handle each cycle) pick it up.
        if git_swap_rejection(engine.availability()).is_some() {
            GitReconcile::RejectUnavailable
        } else {
            *state.git.write().await = Some(engine);
            GitReconcile::Swapped
        }
    } else {
        GitReconcile::Unchanged
    };

    // Derive the rest of the reconciliation PURELY (so finding 1's ordering rule is
    // unit-testable without a Tauri harness, see `plan_settings_reconcile`): which
    // independent subsystems to actuate and whether the save ends in a git rejection.
    let plan = plan_settings_reconcile(
        git,
        previous_autostart != Some(settings.autostart),
        previous_global != Some(settings.global_check_minutes),
    );

    // BL-NI-25 / finding 4: a changed global cadence takes effect on already-
    // scheduled INHERIT repos immediately, without waiting out their stale
    // `next_check_at`. Only recompute when the value actually changed, so saving an
    // unrelated setting never disturbs every repo's schedule.
    if plan.reschedule_inherit {
        reposync_core::scheduler::reschedule_inherit_repos(
            &state.pool,
            crate::localtime::now_unix(),
            settings.global_check_minutes,
        )
        .await?;
    }

    // E-15 AC1: actuate launch-on-login when the `autostart` setting changed. ORDERING
    // (finding 1): this runs REGARDLESS of the git outcome and BEFORE any git-path
    // rejection is returned, so a bad git path in the same save never SKIPS an
    // autostart toggle (the pre-fix early return did exactly that). Persist-then-apply
    // (commit 71a0f7b): the value is already stored, so a plugin failure returns a
    // structured `InvalidSetting { field: "autostart" }` without rollback, and
    // `reconcile_on_launch` self-heals it on the next launch.
    let autostart_result = if plan.apply_autostart {
        crate::autostart::apply(&app, settings.autostart)
    } else {
        Ok(())
    };

    // A changed git path that probed unusable is reported LAST, after the independent
    // subsystems above have been actuated - preserving the git error's precedence
    // (BL-NI-26) while no longer skipping autostart/cadence. If git was fine but the
    // autostart actuation failed, surface that instead.
    if plan.reject_git_path {
        return Err(AppError::InvalidSetting {
            field: "git_executable_path".into(),
        });
    }
    autostart_result?;
    Ok(())
}

/// The BL-NI-26 / finding-5 git-swap contract, as a pure decision over the probed
/// [`GitAvailability`] (so it is unit-testable without a Tauri harness): a probe
/// that resolved to a usable git (`Available` or `BelowFloor` - still usable, just
/// flagged) is accepted for the live swap (`None`); an `Unavailable` probe is
/// REJECTED with a structured `InvalidSetting` on the git-path field, so the
/// caller keeps the last-known-working engine instead of silently swapping to
/// None and falsely toasting success.
fn git_swap_rejection(availability: &reposync_core::git::GitAvailability) -> Option<AppError> {
    if availability.is_unavailable() {
        Some(AppError::InvalidSetting {
            field: "git_executable_path".into(),
        })
    } else {
        None
    }
}

/// The git-path portion of a settings save, classified for [`plan_settings_reconcile`]
/// (finding 1). The LIVE engine is only touched when the git path actually changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GitReconcile {
    /// The git path did not change: the live engine stands untouched (no re-probe),
    /// so a git-less machine never fails an unrelated save.
    Unchanged,
    /// The git path changed to a usable git: the engine was swapped in live.
    Swapped,
    /// The git path changed to an unusable git: the last-known-working engine is kept
    /// and the save is rejected AFTER the independent subsystems have run.
    RejectUnavailable,
}

/// Which live subsystems a settings save actuates, and whether it ends in a git-path
/// rejection - derived PURELY (so finding 1's ordering rule is unit-testable without a
/// Tauri harness). The rule: the git re-probe, the autostart actuation, and the
/// inherit-cadence reschedule are INDEPENDENT. autostart is actuated whenever it
/// changed - EVEN when a changed git path probed unusable, so a git-path typo never
/// skips an autostart toggle in the same save - and a save that does not change the git
/// path never rejects on git, so a git-less machine never fails an unrelated
/// notify/quiet-hours/autostart/cadence save.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SettingsReconcilePlan {
    /// Reschedule already-scheduled inherit repos (the global cadence changed).
    reschedule_inherit: bool,
    /// Actuate launch-on-login (the autostart setting changed).
    apply_autostart: bool,
    /// Reject the save with `InvalidSetting(git_executable_path)` after the
    /// independent subsystems above were actuated (a changed git path probed unusable).
    reject_git_path: bool,
}

fn plan_settings_reconcile(
    git: GitReconcile,
    autostart_changed: bool,
    global_cadence_changed: bool,
) -> SettingsReconcilePlan {
    SettingsReconcilePlan {
        reschedule_inherit: global_cadence_changed,
        apply_autostart: autostart_changed,
        reject_git_path: matches!(git, GitReconcile::RejectUnavailable),
    }
}

/// Read the one-time database-recovery notice (E-02 AC7 / BL-NI-33).
///
/// Additive E-06 amendment. Surfaces the `db_recovered` / `db_backup_path` fields
/// parked in [`AppState`] after a startup migration-failure recovery, so the
/// frontend can show the AC7 notice (a dismissible banner). Before this command,
/// nothing read those fields, so the notice could never reach the UI. `Ok`-only:
/// reading managed state never fails.
#[tauri::command]
#[specta::specta]
pub async fn db_recovery_notice(
    state: tauri::State<'_, AppState>,
) -> Result<DbRecoveryNotice, AppError> {
    Ok(build_recovery_notice(
        state.db_recovered,
        state.db_backup_path.as_deref(),
    ))
}

/// Build the [`DbRecoveryNotice`] payload from the parked recovery fields (pure, so
/// it is unit-tested without a Tauri harness, like [`git_swap_rejection`]). The
/// backup path is rendered to a display string for the wire.
fn build_recovery_notice(
    recovered: bool,
    backup_path: Option<&std::path::Path>,
) -> DbRecoveryNotice {
    DbRecoveryNotice {
        recovered,
        backup_path: backup_path.map(|p| p.display().to_string()),
    }
}

// =============================================================================
// App self-update (E-18 auto-update and distribution)
//
// Two thin wrappers over the `tauri-plugin-updater` edge (`crate::updates`), so the
// on-launch check and the Settings "Check for updates" button share one typed path
// and the ship-dark + toggle gates live in one place. reposync-core stays Tauri-free;
// the plugin call lives only in `crate::updates`.
// =============================================================================

/// Check for an app update (E-18). Runs the plugin check and returns a typed
/// [`UpdateAvailability`] distinguishing "update available" / "up to date" /
/// "couldn't reach the update server" WITHOUT throwing (the manual button and the
/// on-launch check share this path). Never installs; every install is user-confirmed
/// via [`app_install_update`]. Infallible by design: an unreachable server (offline,
/// the inert private-repo endpoint, or ship-dark) is a payload state, not an error.
#[tauri::command]
#[specta::specta]
pub async fn app_check_for_update(app: tauri::AppHandle) -> UpdateAvailability {
    crate::updates::check(&app).await
}

/// Download, verify, and install the pending app update, then relaunch (E-18).
/// Called ONLY after the user confirms. The plugin verifies the minisign signature
/// before replacing the running binary; a verification/download failure returns a
/// typed [`AppError`] and leaves the current version intact. On success the process
/// relaunches into the new version (so this normally does not return `Ok`).
#[tauri::command]
#[specta::specta]
pub async fn app_install_update(app: tauri::AppHandle) -> Result<(), AppError> {
    crate::updates::install(&app).await
}

// =============================================================================
// Groups / tags (E-01 groups feature)
//
// Thin adapters over the `reposync_core::store` group functions. Grouping is a
// pure metadata operation on the SQLite tables (no git, no per-repo lock), so
// each handler just forwards the pool.
// =============================================================================

/// List every group with its member repo count (group-management view).
#[tauri::command]
#[specta::specta]
pub async fn group_list(state: tauri::State<'_, AppState>) -> Result<Vec<GroupSummary>, AppError> {
    reposync_core::store::groups_list(&state.pool).await
}

/// Create a group. A duplicate name is rejected as an invalid setting.
#[tauri::command]
#[specta::specta]
pub async fn group_create(
    state: tauri::State<'_, AppState>,
    name: String,
    color: Option<String>,
) -> Result<GroupSummary, AppError> {
    reposync_core::store::group_create(&state.pool, &name, color.as_deref()).await
}

/// Rename a group. A duplicate name is rejected; a missing id is NotFound.
#[tauri::command]
#[specta::specta]
pub async fn group_rename(
    state: tauri::State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), AppError> {
    reposync_core::store::group_rename(&state.pool, id, &name).await
}

/// Delete a group (idempotent; memberships cascade away).
#[tauri::command]
#[specta::specta]
pub async fn group_delete(state: tauri::State<'_, AppState>, id: i64) -> Result<(), AppError> {
    reposync_core::store::group_delete(&state.pool, id).await
}

/// Assign a repo to a group (idempotent; a missing repo/group is NotFound).
#[tauri::command]
#[specta::specta]
pub async fn group_assign(
    state: tauri::State<'_, AppState>,
    repo_id: i64,
    group_id: i64,
) -> Result<(), AppError> {
    reposync_core::store::group_assign(&state.pool, repo_id, group_id).await
}

/// Remove a repo from a group (idempotent).
#[tauri::command]
#[specta::specta]
pub async fn group_unassign(
    state: tauri::State<'_, AppState>,
    repo_id: i64,
    group_id: i64,
) -> Result<(), AppError> {
    reposync_core::store::group_unassign(&state.pool, repo_id, group_id).await
}

/// List the ids of the groups a repo belongs to (ascending).
#[tauri::command]
#[specta::specta]
pub async fn groups_for_repo(
    state: tauri::State<'_, AppState>,
    repo_id: i64,
) -> Result<Vec<i64>, AppError> {
    reposync_core::store::groups_for_repo(&state.pool, repo_id).await
}

/// All repo-group memberships in ONE read (BL-NI-22): one entry per repo that
/// belongs to at least one group, so the Repos screen builds its membership map in
/// a single round-trip instead of fanning `groups_for_repo` out per visible repo.
#[tauri::command]
#[specta::specta]
pub async fn repo_group_memberships(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RepoGroupMembership>, AppError> {
    reposync_core::store::repo_group_memberships(&state.pool).await
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
            pr_stale: false,
            requests_made: 0,
            changed: false,
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

    #[test]
    fn git_swap_rejects_unavailable_and_accepts_usable() {
        // BL-NI-26 / finding 5: a probe that resolved to no usable git is rejected
        // with InvalidSetting on the git-path field, so `settings_set` keeps the
        // last-known-working engine (the early return leaves `state.git` untouched)
        // instead of silently swapping to None and toasting a false success.
        use reposync_core::git::discover::GitVersion;
        use reposync_core::git::GitAvailability;

        let rejected = git_swap_rejection(&GitAvailability::Unavailable);
        assert!(
            matches!(&rejected, Some(AppError::InvalidSetting { field }) if field == "git_executable_path"),
            "an unavailable probe must be rejected as InvalidSetting on git_executable_path, got {rejected:?}"
        );

        // A usable probe (Available, or the still-usable BelowFloor state) is
        // accepted: no rejection, so the live swap proceeds.
        assert!(
            git_swap_rejection(&GitAvailability::Available {
                version: GitVersion {
                    major: 2,
                    minor: 40,
                    patch: 0,
                },
            })
            .is_none(),
            "an available probe must be accepted for the live swap"
        );
        assert!(
            git_swap_rejection(&GitAvailability::BelowFloor {
                version: GitVersion {
                    major: 2,
                    minor: 20,
                    patch: 0,
                },
            })
            .is_none(),
            "a below-floor git is still usable, so the swap proceeds"
        );
    }

    #[test]
    fn plan_settings_reconcile_keeps_subsystems_independent() {
        // Finding 1 regression: the git re-probe, the autostart actuation, and the
        // inherit-cadence reschedule are INDEPENDENT. On a git-less machine (or any
        // save that does not touch the git path) the git portion is Unchanged, so the
        // save NEVER rejects on git and STILL actuates an autostart toggle and a
        // global-cadence change - the exact regression this fix closes.
        let p = plan_settings_reconcile(GitReconcile::Unchanged, true, true);
        assert!(
            !p.reject_git_path,
            "an unchanged git path must not reject the save (git-less machine)"
        );
        assert!(
            p.apply_autostart,
            "an autostart toggle applies when the git path is untouched"
        );
        assert!(
            p.reschedule_inherit,
            "a global-cadence change reschedules when the git path is untouched"
        );

        // A changed-but-unusable git path rejects (BL-NI-26) but STILL actuates the
        // independent autostart toggle first: a git-path typo never skips autostart.
        let p = plan_settings_reconcile(GitReconcile::RejectUnavailable, true, false);
        assert!(p.reject_git_path, "a changed unusable git path rejects");
        assert!(
            p.apply_autostart,
            "autostart still applies even though the git path is rejected"
        );
        assert!(
            !p.reschedule_inherit,
            "an unchanged cadence does not reschedule"
        );

        // A rejection with no autostart / cadence change still rejects and actuates
        // nothing extra.
        let p = plan_settings_reconcile(GitReconcile::RejectUnavailable, false, false);
        assert!(p.reject_git_path);
        assert!(!p.apply_autostart);
        assert!(!p.reschedule_inherit);

        // A changed usable git path swaps in and does not reject.
        let p = plan_settings_reconcile(GitReconcile::Swapped, false, false);
        assert!(!p.reject_git_path, "a usable git swap does not reject");
        assert!(!p.apply_autostart);
        assert!(!p.reschedule_inherit);
    }

    #[test]
    fn build_recovery_notice_maps_parked_recovery_fields() {
        // BL-NI-33 / E-02 AC7: a normal launch reports no recovery and no path.
        let normal = build_recovery_notice(false, None);
        assert!(!normal.recovered);
        assert!(normal.backup_path.is_none());

        // A recovered launch reports the flag and the backup path as a display string,
        // so the frontend can name where the previous database was preserved.
        let path = std::path::Path::new("C:/data/reposync.db.corrupt-1700000000");
        let notice = build_recovery_notice(true, Some(path));
        assert!(notice.recovered);
        assert_eq!(notice.backup_path, Some(path.display().to_string()));
    }
}
