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
    ActivityFilter, ActivityRecord, CheckResult, DailySummary, DbRecoveryNotice, Diagnostics,
    GroupSummary, RepoDetail, RepoFilter, RepoGroupMembership, RepoId, RepoSummary, ScanResult,
    Settings, UpdateAvailability, UpdateMode, UpdatePolicy, UpdateResult, WeeklySummary,
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
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<RepoId, AppError> {
    // Clone the engine OUT of the read lock and drop the guard immediately, so a
    // long-running git operation never holds the lock against a `settings_set`
    // re-probe (BL-NI-19). The engine is cheap to clone (it wraps shared handles).
    let git = { state.git.read().await.clone() }.ok_or(AppError::GitNotFound)?;
    let id = reposync_core::repo::add(&state.pool, &git, std::path::Path::new(&path)).await?;
    // A newly added repo has a current `created_at`, so it belongs in Open recent
    // immediately rather than after the first unrelated refresh (BL-NI-40).
    // `AppHandle` is INJECTED by Tauri, not a frontend argument, so this does not
    // change the IPC contract.
    crate::tray::refresh_recent_menu(&app).await;
    Ok(id)
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
    // A check is what makes a repo "recently active", so this is the single-repo
    // counterpart of the burst refresh (BL-NI-40). A no-op unless the order moved.
    crate::tray::refresh_recent_menu(&app).await;
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
///
/// Since BL-NI-04 the returned count means "attempted", not "succeeded". A check
/// whose fetch failed now returns `Ok` with `failed: true`, so it emits its
/// completion event and counts here. That widening is deliberate and it is the
/// honest reading of a tray item labelled "Check All Now": the user asked for N
/// repos to be checked, N were checked, and some of them came back with bad news
/// that the per-repo event now carries. The `error:raised` arm is left for the
/// checks that could not RUN at all.
#[tauri::command]
#[specta::specta]
pub async fn repo_check_all(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<u32, AppError> {
    check_all_enabled(
        &app,
        &state.pool,
        &state.git,
        &state.locks,
        &state.check_all_semaphore,
    )
    .await
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
    semaphore: &std::sync::Arc<tokio::sync::Semaphore>,
) -> Result<u32, AppError> {
    // Resolve the live engine once (cloned out of the read lock, guard dropped
    // immediately, per BL-NI-19); a check-all with no git is one clear error.
    let git = { git.read().await.clone() }.ok_or(AppError::GitNotFound)?;

    let flags = reposync_core::store::repo_enabled_flags(pool).await?;
    let targets = reposync_core::store::select_check_all_targets(&flags);

    // Fan out under the scheduler's OWN semaphore, rather than one repo at a time
    // (BL-NI-41).
    //
    // The loop this replaces awaited each `check_now` fully before starting the
    // next, so a forty-repo library ran forty git fetches end to end while the
    // scheduler beside it was allowed four at once. The tray item says "Check All
    // Now" and took minutes.
    //
    // THE LOCK ORDER IS NOT A DETAIL. Per-repo mutex FIRST, global permit SECOND,
    // exactly as `scheduler::run_job` does, and the scheduler's own module doc
    // calls that fixed order one of two load-bearing correctness properties.
    // Reversed, a task holding a permit while waiting on a per-repo mutex can
    // starve the global cap against the task that would release that mutex. The
    // permit is dropped before the guard for the same reason.
    //
    // The semaphore is the scheduler's, not a new one of the same size. Two
    // independent caps do not compose: a check-all landing during a tick would put
    // twice the configured number of git processes on the disk, which is the exact
    // number this cap exists to control.
    // ADMISSION IS BOUNDED, and that is not the same thing as the permit cap.
    //
    // Spawning every target at once looks equivalent, since only `concurrency` of
    // them can hold a permit, but it is not: each task takes its repo's mutex
    // BEFORE queuing for a permit, so a 500-repo burst would hold 500 repo locks
    // while four did work. A scheduled job for any of those repos could not even
    // ask for a permit until the burst released that repo's mutex, and would then
    // queue behind every remaining burst task on tokio's fair FIFO semaphore. Not
    // a deadlock, since both sides take mutex-then-permit in the same order, but a
    // convoy that stalls the scheduler for the length of the whole burst.
    //
    // Keeping at most `concurrency` tasks in flight bounds the number of repo
    // mutexes the burst holds at once to the number it can actually work on.
    let max_in_flight = reposync_core::scheduler::DEFAULT_CONCURRENCY.max(1);
    let mut pending = targets.into_iter();
    let mut tasks = tokio::task::JoinSet::new();

    let mut spawn_next = |tasks: &mut tokio::task::JoinSet<_>| {
        let Some(id) = pending.next() else {
            return false;
        };
        let app = app.clone();
        let pool = pool.clone();
        let git = git.clone();
        let locks = locks.clone();
        let semaphore = std::sync::Arc::clone(semaphore);
        tasks.spawn(async move {
            // Per-repo mutex FIRST, global permit SECOND, permit dropped before
            // the guard: the same fixed order `scheduler::run_job` uses.
            let lock = locks.lock_handle(RepoId(id));
            let _guard = lock.lock_owned().await;
            let permit = semaphore
                .acquire_owned()
                .await
                .expect("the scheduler semaphore is never closed");

            emit_check_started(&app, id);
            let outcome = reposync_core::repo::check_now(&pool, &git, RepoId(id)).await;
            drop(permit);
            outcome
        });
        true
    };

    while tasks.len() < max_in_flight && spawn_next(&mut tasks) {}

    let mut checked = 0u32;
    // Failures that COMPLETED (a non-zero fetch), collected rather than announced
    // one at a time. See the coalescing note below.
    let mut soft_failures: Vec<Option<String>> = Vec::new();

    while let Some(joined) = tasks.join_next().await {
        // Refill as each finishes, so the burst keeps `max_in_flight` moving
        // without ever having claimed more repo locks than that.
        spawn_next(&mut tasks);
        match joined {
            Ok(Ok(result)) => {
                // The completion event fires for EVERY outcome, including a failed
                // one (BL-NI-04). That is the whole point: an open window learns
                // that this repo's check finished and how.
                emit_check_completed(app, &result);
                checked += 1;
                if result.failed {
                    soft_failures.push(result.reason.clone());
                }
            }
            // A check that could not RUN at all (git vanished mid-burst, the path
            // is gone). Still surfaced immediately and individually: it is rare,
            // and it usually means the next repo will fail the same way.
            Ok(Err(e)) => emit_error_raised(app, &e),
            // The task did not return a value: it panicked or was cancelled. Under
            // the release profile's `panic = "abort"` a panicking task takes the
            // process with it, so this arm is unreachable in a shipped build and is
            // deliberately NOT counted or reported as a check failure, which would
            // be a different and misleading claim. It is logged, because a debug or
            // dev run can reach it and that repo's check vanished. Same reasoning,
            // and the same event name, as the scheduler's equivalent arm.
            Err(e) => tracing::error!(
                event = reposync_core::logging::event::SCHEDULER_JOB_ABORTED,
                error = %e,
                "a check-all task did not complete; that repo was not checked"
            ),
        }
    }

    // ONE signal for the whole burst, not one per repo.
    //
    // Before BL-NI-04 a failed check returned `Err`, so it landed in the arm above
    // and produced its own `error:raised`, which the shell turns into a toast. That
    // was the only failure feedback the tray path had, and moving failures onto the
    // Ok branch would have removed it silently: "Check All Now" would have reported
    // nothing at all while every repository failed.
    //
    // Restoring it verbatim would restore a worse problem, though. One toast per
    // failed repo means a forty-repo library on a dropped network produces forty
    // toasts, all saying the same thing, burying anything else on screen. So the
    // burst reports once, choosing the most ACTIONABLE representative rather than
    // the first or the most common: an auth failure outranks a network failure
    // because it will not fix itself and the policy engine pauses the repo for it,
    // while a network failure is usually one condition affecting everything and
    // resolves on its own.
    //
    // The per-repo detail is not lost; it is in the activity receipt for each
    // repository, which is where the raw git output now actually renders.
    if let Some(representative) = check_all_failure_signal(&soft_failures, checked) {
        emit_error_raised(app, &representative);
    }

    // A burst is the single event most likely to reorder "most recently active",
    // so the tray's Open recent submenu is refreshed once after it rather than
    // once per repo (BL-NI-40). A no-op when the order did not actually change.
    crate::tray::refresh_recent_menu(app).await;

    Ok(checked)
}

/// Choose the ONE error to report for a check-all burst, or `None` when nothing
/// failed. Pure, so the priority rule is asserted by a test rather than by reading
/// an async function that needs a Tauri handle to call.
///
/// The ordering is by ACTIONABILITY, not by frequency or by which came first:
///
/// 1. **Auth** wins outright. It will not fix itself, it is the failure the policy
///    engine pauses a repository for, and one credential problem hiding behind
///    nineteen network timeouts is the case where a summary most needs to pick the
///    right thing to say.
/// 2. **Network** next. Usually one condition affecting everything, and usually
///    transient, so it is worth naming but not worth outranking auth.
/// 3. Otherwise a **count**, which is the honest fallback: the burst hit something
///    that is neither, and the receipts have the detail.
fn check_all_failure_signal(reasons: &[Option<String>], checked: u32) -> Option<AppError> {
    if reasons.is_empty() {
        return None;
    }
    let any = |code: &str| reasons.iter().any(|r| r.as_deref() == Some(code));
    Some(if any("git.auth_failed") {
        AppError::AuthFailed
    } else if any("net.offline") {
        AppError::Offline
    } else {
        AppError::FetchFailed {
            exit_code: None,
            stderr: format!(
                "{} of {} repositories could not be checked. \
                 Select each one's newest entry in Activity for the exact command and output.",
                reasons.len(),
                checked
            ),
        }
    })
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
pub async fn repo_remove(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    id: i64,
) -> Result<(), AppError> {
    // Hold the per-repo lock across the delete so a scheduled job on this repo
    // cannot race the removal, then evict the now-dead lock entry.
    let _lock = state.locks.lock_handle(RepoId(id)).lock_owned().await;
    reposync_core::store::repo_remove(&state.pool, RepoId(id)).await?;
    state.locks.remove(RepoId(id));
    // A removed repo must not linger in the tray's Open recent submenu, which is
    // the one surface that used to keep showing it until the next restart
    // (BL-NI-40). `AppHandle` is INJECTED by Tauri, not a frontend argument, so
    // this does not change the IPC contract or the generated bindings.
    crate::tray::refresh_recent_menu(&app).await;
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
    // An update writes last_checked_at and often last_updated_at, so it is a
    // recency writer exactly like a check (BL-NI-40).
    crate::tray::refresh_recent_menu(&app).await;
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
    mut settings: Settings,
) -> Result<(), AppError> {
    // Serialize the whole persist/reschedule/probe/swap sequence (BL-NI-35).
    let _write = state.settings_write_lock.lock().await;

    // Read the prior settings BEFORE persisting so we can tell what actually
    // changed and only reconcile the affected live subsystem: the global cadence
    // (below, re-cadence repos) and the `autostart` setting (E-15, actuate the
    // plugin). `app` is Tauri-injected, so it does NOT appear in the generated
    // TypeScript binding (`settingsSet(settings)` is unchanged).
    //
    // The read failure is PROPAGATED rather than tolerated (Codex review round 2,
    // finding 2). It used to degrade to `None`, which meant the code had to guess
    // at the previous autostart value if the plugin call then failed - and that
    // guess got WRITTEN, so an unrelated save could durably flip a setting nobody
    // read or requested. There is nothing to degrade to here: `settings_get`
    // seeds the singleton on first call, so it only fails when the database
    // genuinely cannot be read, and the write below would fail too.
    let previous = reposync_core::store::settings_get(&state.pool).await?;
    let previous_global = previous.global_check_minutes;
    let previous_autostart = previous.autostart;
    let previous_git_path = previous.git_executable_path.clone();

    // Reject an invalid payload BEFORE actuating anything (Codex review round 2,
    // finding 1). The autostart plugin call below happens before the durable
    // write, so without this a save that toggles autostart AND carries an
    // out-of-range unrelated field - a cleared numeric input arriving as 0, say -
    // would change the OS registration, then be rejected, and leave the setting
    // and the OS disagreeing for the next launch to adopt. That would make the
    // rejected save durable by the back door.
    reposync_core::store::validate_settings(&settings)?;

    // E-15 AC1: actuate launch-on-login when the `autostart` setting changed.
    //
    // APPLY-THEN-PERSIST, and deliberately the opposite of the git-path swap next
    // to it (BL-NI-18, Codex review finding 1). It used to be persist-then-apply,
    // on the reasoning that a failed plugin call would be self-healed by the next
    // launch re-registering. Startup reconciliation no longer does that - the OS
    // is now authoritative there - so a persisted value the OS never received
    // would be silently REVERTED at the next launch instead of retried, losing an
    // explicit user choice.
    //
    // Actuating first, and persisting the previous value when actuation fails,
    // establishes the invariant startup reconciliation depends on: THE PERSISTED
    // SETTING NEVER CLAIMS AN ACTUATION THAT DID NOT HAPPEN. Given that, any
    // disagreement seen at startup really was made outside RepoSync, which is
    // exactly what `reposync_core::autostart::reconcile` assumes. It also removes
    // the crash window - dying between the persist and the plugin call can no
    // longer leave a claim the OS never received.
    //
    // Runs before the git probe too, so a bad git path in the same save cannot
    // skip an autostart toggle (the property `plan_settings_reconcile` used to
    // carry, now structural rather than planned).
    let autostart_actuated = previous_autostart != settings.autostart;
    let autostart_result = if autostart_actuated {
        crate::autostart::apply(&app, settings.autostart)
    } else {
        Ok(())
    };
    settings.autostart = autostart_to_persist(
        settings.autostart,
        previous_autostart,
        autostart_result.is_err(),
    );

    if let Err(e) = reposync_core::store::settings_set(&state.pool, &settings).await {
        // The write failed after the OS may already have taken the change. Put the
        // registration back, so a save the user was told had FAILED does not become
        // durable at the next launch by way of adoption. Best-effort by necessity:
        // if the rollback also fails, the OS and the setting disagree and startup
        // adopts whatever the OS actually holds, which is still the truth about the
        // machine. Validation is pre-checked above, so reaching here means the
        // database itself is in trouble.
        if autostart_actuated && autostart_result.is_ok() {
            if let Err(rollback) = crate::autostart::apply(&app, previous_autostart) {
                tracing::warn!(
                    "autostart: the settings write failed and the launch-on-login                      rollback failed too, so the OS registration no longer matches                      the stored setting: {rollback}"
                );
            }
        }
        return Err(e);
    }

    // Mirror the close-button behavior into the shared flag the window
    // CloseRequested handler reads (it is synchronous and cannot query the DB).
    // Storing unconditionally keeps the flag exactly in sync with the just-persisted
    // value; the handler reads it fresh on the next close, so the change is live.
    state.close_minimizes_to_tray.store(
        settings.close_minimizes_to_tray,
        std::sync::atomic::Ordering::Relaxed,
    );

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
    let plan = plan_settings_reconcile(git, previous_global != settings.global_check_minutes);

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
    /// Reject the save with `InvalidSetting(git_executable_path)` after the
    /// independent subsystems above were actuated (a changed git path probed unusable).
    reject_git_path: bool,
}

fn plan_settings_reconcile(
    git: GitReconcile,
    global_cadence_changed: bool,
) -> SettingsReconcilePlan {
    SettingsReconcilePlan {
        reschedule_inherit: global_cadence_changed,
        reject_git_path: matches!(git, GitReconcile::RejectUnavailable),
    }
}

/// The `autostart` value to WRITE, given what the user asked for and whether the
/// OS actuation actually succeeded (BL-NI-18, Codex review finding 1).
///
/// The persisted row must never claim an actuation that did not happen, because
/// startup reconciliation now adopts the OS state on a disagreement: a stored
/// "on" the OS never received would be quietly turned back off at the next
/// launch rather than retried. So a failed apply persists the value that is
/// still true.
///
/// The previous value is always known: `settings_set` propagates a failed
/// pre-read rather than degrading to a guess. An earlier version inferred it
/// when unknown, which meant WRITING an assumption - the Codex review's round-2
/// finding 2, and a fair one.
fn autostart_to_persist(requested: bool, previous: bool, apply_failed: bool) -> bool {
    if apply_failed {
        previous
    } else {
        requested
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

// =============================================================================
// Diagnostics (additive): where the files are, what is being logged, and whether
// the background scheduler is quietly failing.
//
// The motivating gap: RepoSync builds with `windows_subsystem = "windows"`, so a
// release build has no console, and everything worth diagnosing goes to a file in
// a directory the user has no reason to know about. PR #35 gave the app a real
// log; without this card, reaching that log means knowing to type
// `%LOCALAPPDATA%\RepoSync\logs` into an address bar.
// =============================================================================

/// Read the diagnostics snapshot for the Settings -> Diagnostics card.
///
/// `Ok`-only in practice: every field is a read of managed state, a resolved
/// path, or a filesystem stat that reports zeroes rather than failing. It still
/// returns `Result` so the frontend's one `unwrap` helper covers it like every
/// other command.
#[tauri::command]
#[specta::specta]
pub async fn diagnostics_get(state: tauri::State<'_, AppState>) -> Result<Diagnostics, AppError> {
    let paths = reposync_core::paths::AppPaths::from_env();
    // Clone the engine out of the read lock and drop the guard immediately, per
    // BL-NI-19, so reading diagnostics can never block a settings re-probe.
    // The engine and the CONFIGURED path are read as ONE snapshot, under the
    // settings single-flight guard (BL-NI-39 + BL-NI-35).
    //
    // Reading them independently is a race with a user-visible consequence, and
    // it is the exact false warning this feature exists to avoid rather than
    // create. `settings_set` PERSISTS the new path and only then probes and swaps
    // the engine. A diagnostics refresh landing between those two steps would see
    // the new path against the old engine, conclude the configured path had been
    // ignored, and leave that alarm on screen until something refreshed it, on a
    // save that then succeeded perfectly.
    //
    // Holding the guard can make a refresh wait for an in-flight save, which
    // includes a `git --version` probe. That is the right trade: a diagnostics
    // panel that waits a moment is better than one that lies.
    let (git, settings_read) = {
        let _snapshot = state.settings_write_lock.lock().await;
        let git = { state.git.read().await.clone() };
        let settings = reposync_core::store::settings_get(&state.pool).await;
        (git, settings)
    };
    // A settings read that FAILED is not the same as no path configured, and is
    // not silently turned into one. It is logged, and the comparison reports
    // "cannot say" rather than "honored" (see `build_diagnostics`).
    let explicit_git = match settings_read {
        Ok(s) => s.git_executable_path,
        Err(e) => {
            tracing::warn!(
                event = reposync_core::logging::event::DIAGNOSTICS_SETTINGS_READ_FAILED,
                error = %e,
                "could not read settings for the diagnostics view; the configured                  git path cannot be compared against the resolved one"
            );
            None
        }
    };
    Ok(build_diagnostics(
        env!("CARGO_PKG_VERSION"),
        &paths,
        state.log_config.as_ref(),
        git.as_ref().map(|g| g.availability().clone()),
        git.as_ref()
            .and_then(|g| g.git_exe().map(|p| p.to_path_buf())),
        explicit_git,
        &state.scheduler_health,
        state.db_recovered,
    ))
}

/// Open the log directory in the OS file manager.
///
/// Creates it first. On a launch where logging failed to start there may be no
/// directory at all, and opening a file manager on a path that does not exist is
/// an error dialog rather than an answer; an empty folder at least tells the
/// truth about what has been recorded.
#[tauri::command]
#[specta::specta]
pub async fn diagnostics_open_log_dir(_state: tauri::State<'_, AppState>) -> Result<(), AppError> {
    let paths = reposync_core::paths::AppPaths::from_env();
    let dir = paths.log_dir();
    if let Err(e) = std::fs::create_dir_all(&dir) {
        return Err(AppError::Unexpected {
            context: format!("could not create the log directory {}: {e}", dir.display()),
        });
    }
    crate::opener::open_folder(&dir)
}

/// Assemble the [`Diagnostics`] payload from already-resolved inputs.
///
/// Pure and parameterized (no `AppState`, no environment reads beyond the paths
/// it is handed) for the same reason [`build_recovery_notice`] is: it is the part
/// with decisions in it - what "logging is active" means, how an absent git is
/// rendered - and those decisions are worth testing without a Tauri harness.
#[allow(clippy::too_many_arguments)]
fn build_diagnostics(
    app_version: &str,
    paths: &reposync_core::paths::AppPaths,
    log_config: Option<&crate::logging::LogConfig>,
    git_availability: Option<reposync_core::git::GitAvailability>,
    git_exe: Option<std::path::PathBuf>,
    explicit_git: Option<String>,
    health: &crate::SchedulerHealth,
    db_recovered: bool,
) -> Diagnostics {
    use std::sync::atomic::Ordering;

    // The log directory reported is the one the appender ACTUALLY opened when
    // logging is running, and the resolved default otherwise. They agree today;
    // preferring the live one means they cannot silently diverge later.
    let log_dir = log_config
        .map(|c| c.dir.clone())
        .unwrap_or_else(|| paths.log_dir());
    let stats = reposync_core::logging::log_dir_stats(&log_dir);

    // Three engine states map to two INDEPENDENT booleans, not one. A
    // below-floor git is resolved and still used (E-03 AC7: "usable but
    // flagged - operations are still attempted"), so folding it into a single
    // `available: false` would tell the user RepoSync had stopped running git
    // when it had not. Showing the version alongside is the point: "2.28.0"
    // explains the flag by itself.
    // Kept as a PathBuf for the comparison below: explicit_path_honored
    // normalizes separators and wants the path, not its display string.
    let git_exe_for_compare = git_exe;
    let (git_version, git_resolved, git_meets_floor) = match &git_availability {
        Some(reposync_core::git::GitAvailability::Available { version }) => {
            (Some(version.to_string()), true, true)
        }
        Some(reposync_core::git::GitAvailability::BelowFloor { version }) => {
            (Some(version.to_string()), true, false)
        }
        Some(reposync_core::git::GitAvailability::Unavailable) | None => (None, false, false),
    };

    Diagnostics {
        app_version: app_version.to_string(),
        data_dir: paths.data_dir().display().to_string(),
        db_path: paths.db_path().display().to_string(),
        log_dir: log_dir.display().to_string(),
        logging_active: log_config.is_some(),
        log_level: log_config.map(|c| c.level.to_string()),
        log_max_files: log_config.map(|c| c.retention.max_files as i64),
        log_max_bytes: log_config.map(|c| c.retention.max_bytes as i64),
        log_dir_readable: stats.readable,
        log_file_count: stats.file_count as i64,
        log_bytes: stats.total_bytes as i64,
        // Read from the writer's own counters, not inferred from the directory.
        // With logging off there is no writer, so these report zero / none rather
        // than a fabricated healthy-looking value; `logging_active: false` is
        // already the honest answer in that case.
        log_write_failures: log_config
            .map(|c| c.health.write_failures() as i64)
            .unwrap_or(0),
        log_last_write_failure_at: log_config.and_then(|c| c.health.last_failure_unix()),
        log_bytes_written: log_config
            .map(|c| c.health.bytes_written() as i64)
            .unwrap_or(0),
        log_dropped_lines: log_config
            .map(|c| c.health.dropped_lines() as i64)
            .unwrap_or(0),
        onedrive_rooted: paths.is_onedrive_rooted(),
        git_path: git_exe_for_compare
            .as_ref()
            .map(|p| p.display().to_string()),
        git_version,
        git_resolved,
        // "Honored" is only a claim when something was configured. With nothing
        // set there is nothing to honor, so `true` here is an absence, and the
        // field's doc says so rather than leaving a reader to infer it. A
        // configured path with NO resolved git is also not a mismatch: that is
        // GitAvailability::Unavailable, already reported on its own field, and
        // flagging it twice would send the user after the wrong problem.
        git_explicit_path_honored: match (explicit_git.as_deref(), git_exe_for_compare.as_deref()) {
            (Some(explicit), Some(resolved)) if !explicit.trim().is_empty() => Some(
                reposync_core::git::discover::explicit_path_honored(explicit, resolved),
            ),
            // NO comparison was made, and that is said rather than encoded as a
            // healthy-looking `true`. Three distinct situations land here, and
            // each is already reported by the field that owns it: nothing is
            // configured (`git_explicit_path` is None), the settings read failed
            // (logged as diagnostics.settings_read_failed), or no git resolved at
            // all (`git_resolved` is false, which is the real problem and must
            // not be duplicated as a path mismatch).
            _ => None,
        },
        git_explicit_path: explicit_git,
        git_meets_floor,
        scheduler_cycles: health.cycles.load(Ordering::Relaxed) as i64,
        scheduler_repos_checked: health.repos_checked.load(Ordering::Relaxed) as i64,
        scheduler_outcome_persist_failures: health.outcome_persist_failures.load(Ordering::Relaxed)
            as i64,
        db_recovered,
    }
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

/// Rename a group without touching its color.
///
/// Retained as a compatibility command under the E-06 additive IPC contract. New
/// callers should use [`group_update`], which edits name and color together in one
/// atomic write; this exists so a consumer on the original two-argument contract
/// keeps working and does not lose the group's color as a side effect.
#[tauri::command]
#[specta::specta]
pub async fn group_rename(
    state: tauri::State<'_, AppState>,
    id: i64,
    name: String,
) -> Result<(), AppError> {
    reposync_core::store::group_rename(&state.pool, id, &name).await
}

/// Update a group's name and color atomically. A duplicate name is rejected; a
/// missing id is NotFound. A single UPDATE, so a name clash leaves both fields
/// unchanged - an edit never partially persists.
#[tauri::command]
#[specta::specta]
pub async fn group_update(
    state: tauri::State<'_, AppState>,
    id: i64,
    name: String,
    color: Option<String>,
) -> Result<(), AppError> {
    reposync_core::store::group_update(&state.pool, id, &name, color.as_deref()).await
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
    use reposync_core::git::GitAvailability;
    // --- explicit git path honored (BL-NI-39) --------------------------------

    /// A configured path that IS the one running reports honored.
    #[test]
    fn a_configured_git_path_that_resolved_reports_honored() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Available {
                version: reposync_core::git::discover::GitVersion {
                    major: 2,
                    minor: 47,
                    patch: 1,
                },
            }),
            Some(std::path::PathBuf::from("C:/Program Files/Git/cmd/git.exe")),
            Some("C:/Program Files/Git/cmd/git.exe".to_string()),
            &health_with(0, 0, 0),
            false,
        );
        assert_eq!(d.git_explicit_path_honored, Some(true));
        assert_eq!(
            d.git_explicit_path.as_deref(),
            Some("C:/Program Files/Git/cmd/git.exe")
        );
    }

    /// The case this exists for: configured one git, running another, silently.
    #[test]
    fn a_configured_git_path_that_was_ignored_reports_unhonored() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Available {
                version: reposync_core::git::discover::GitVersion {
                    major: 2,
                    minor: 47,
                    patch: 1,
                },
            }),
            Some(std::path::PathBuf::from("C:/Program Files/Git/cmd/git.exe")),
            Some("C:/tools/git/bin/git.exe".to_string()),
            &health_with(0, 0, 0),
            false,
        );
        assert_eq!(
            d.git_explicit_path_honored,
            Some(false),
            "a configured path RepoSync could not use, with a different git running,              is the whole condition BL-NI-39 exists to surface"
        );
    }

    /// Nothing configured is not a mismatch. Reporting one would put a warning on
    /// the default setup, which is the fastest way to teach people to ignore it.
    #[test]
    fn no_configured_git_path_is_not_a_mismatch() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Available {
                version: reposync_core::git::discover::GitVersion {
                    major: 2,
                    minor: 47,
                    patch: 1,
                },
            }),
            Some(std::path::PathBuf::from("C:/Program Files/Git/cmd/git.exe")),
            None,
            &health_with(0, 0, 0),
            false,
        );
        assert_eq!(
            d.git_explicit_path_honored, None,
            "nothing configured means NO comparison was made, which is different              from a comparison that came back honored"
        );
        assert!(d.git_explicit_path.is_none());
    }

    /// A configured path with NO git at all is not a mismatch either: that is
    /// GitAvailability::Unavailable, already reported on its own field. Flagging
    /// it twice would send the user after the wrong problem.
    #[test]
    fn a_configured_path_with_no_git_resolved_is_not_reported_as_a_mismatch() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Unavailable),
            None,
            Some("C:/tools/git/bin/git.exe".to_string()),
            &health_with(0, 0, 0),
            false,
        );
        assert!(!d.git_resolved, "the real condition is reported here");
        assert_eq!(
            d.git_explicit_path_honored, None,
            "and NOT duplicated as a path mismatch, nor claimed as honored"
        );
    }

    // --- check-all failure signalling (BL-NI-04 fallout) ---------------------

    fn r(code: &str) -> Option<String> {
        Some(code.to_string())
    }

    /// A clean burst says nothing. The signal only means something if silence
    /// means success.
    #[test]
    fn a_clean_check_all_raises_no_error() {
        assert!(check_all_failure_signal(&[], 12).is_none());
    }

    /// Auth outranks network, regardless of how outnumbered it is.
    ///
    /// This is the case the priority rule exists for: nineteen repositories timing
    /// out on a dropped connection and ONE with expired credentials. Reporting
    /// "offline" there would send the user to look at their network and leave the
    /// credential problem to be rediscovered later, after the policy engine has
    /// already paused that repository for it.
    #[test]
    fn one_auth_failure_outranks_many_network_failures() {
        let reasons = vec![
            r("net.offline"),
            r("net.offline"),
            r("git.auth_failed"),
            r("net.offline"),
        ];
        assert!(matches!(
            check_all_failure_signal(&reasons, 20),
            Some(AppError::AuthFailed)
        ));
    }

    /// A homogeneous network burst reports the network, not a bare count.
    #[test]
    fn an_all_network_burst_reports_offline() {
        let reasons = vec![r("net.offline"), r("net.offline")];
        assert!(matches!(
            check_all_failure_signal(&reasons, 2),
            Some(AppError::Offline)
        ));
    }

    /// Anything else falls back to a count that names where the detail lives.
    ///
    /// The message has to survive being read in a toast with no other context, so
    /// it carries both numbers and points at Activity. "git fetch failed" alone is
    /// not a next step.
    #[test]
    fn an_unclassified_burst_reports_a_count_and_points_at_activity() {
        let reasons = vec![r("git.fetch_failed"), None];
        match check_all_failure_signal(&reasons, 7) {
            Some(AppError::FetchFailed { stderr, .. }) => {
                assert!(stderr.contains("2 of 7"), "got {stderr:?}");
                assert!(stderr.contains("Activity"), "got {stderr:?}");
            }
            other => panic!("expected a counted FetchFailed, got {other:?}"),
        }
    }

    /// A reason the edge has never seen must not be silently dropped.
    ///
    /// If a future reason code fell through every arm and produced `None`, a real
    /// burst of failures would report nothing at all, which is exactly the
    /// regression this whole signal exists to prevent.
    #[test]
    fn an_unknown_reason_code_still_produces_a_signal() {
        let reasons = vec![r("git.something_invented_later")];
        assert!(check_all_failure_signal(&reasons, 1).is_some());
    }

    use reposync_core::github::{RateLimit, RefreshOutcome, RefreshReport};
    use reposync_core::paths::AppPaths;

    // =========================================================================
    // Diagnostics
    // =========================================================================

    fn health_with(cycles: u64, checked: u64, failures: u64) -> crate::SchedulerHealth {
        use std::sync::atomic::Ordering;
        let h = crate::SchedulerHealth::default();
        h.cycles.store(cycles, Ordering::Relaxed);
        h.repos_checked.store(checked, Ordering::Relaxed);
        h.outcome_persist_failures
            .store(failures, Ordering::Relaxed);
        h
    }

    /// The state that matters most and is easiest to get wrong: logging failed
    /// to start. Reporting the retention the environment ASKED for would tell
    /// the user their logs are being kept for 14 days when nothing is being
    /// written at all - a confident wrong answer in exactly the situation where
    /// the card exists to give a right one.
    #[test]
    fn a_failed_logger_reports_inactive_with_no_configuration() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Unavailable),
            None,
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert!(!d.logging_active);
        assert_eq!(d.log_level, None);
        assert_eq!(d.log_max_files, None);
        assert_eq!(d.log_max_bytes, None);
        // The DIRECTORY is still reported: knowing where logs would have gone is
        // what lets someone check whether it is a permissions problem.
        assert_eq!(d.log_dir, paths.log_dir().display().to_string());
        assert_eq!(d.log_file_count, 0);
        assert!(
            !d.log_dir_readable,
            "the directory does not exist here, and 'could not read' must not be \
             reported as 'read it, found nothing'"
        );
    }

    /// The gap Codex's second finding named, encoded as a test. `logging_active`
    /// proves only that the subscriber installed at STARTUP; `tracing_appender`
    /// writes on a worker thread with no error channel, so a later failure
    /// leaves this true. The card must therefore never treat it as proof that
    /// events are reaching disk - the live directory read is the corroborating
    /// evidence, and the UI flags "started, but nothing on disk" as a warning.
    #[test]
    fn logging_active_and_files_on_disk_are_reported_independently() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let config = crate::logging::LogConfig {
            dir: paths.log_dir(),
            level: tracing::level_filters::LevelFilter::INFO,
            retention: crate::logging::Retention::default(),
            health: std::sync::Arc::new(crate::logging::LogHealth::default()),
        };
        std::fs::create_dir_all(paths.log_dir()).expect("create log dir");

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            Some(&config),
            None,
            None,
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert!(d.logging_active, "the subscriber did install");
        assert!(d.log_dir_readable, "and the directory is readable");
        assert_eq!(
            d.log_file_count, 0,
            "yet nothing is on disk - the contradiction the card surfaces"
        );
    }

    #[test]
    fn an_active_logger_reports_the_configuration_it_was_built_with() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let config = crate::logging::LogConfig {
            dir: paths.log_dir(),
            level: tracing::level_filters::LevelFilter::DEBUG,
            retention: crate::logging::Retention {
                max_files: 7,
                max_bytes: 8 * 1024 * 1024,
            },
            health: std::sync::Arc::new(crate::logging::LogHealth::default()),
        };

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            Some(&config),
            None,
            None,
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert!(d.logging_active);
        assert_eq!(d.log_level.as_deref(), Some("debug"));
        assert_eq!(d.log_max_files, Some(7));
        assert_eq!(d.log_max_bytes, Some(8 * 1024 * 1024));
    }

    /// The three-state engine mapped onto two booleans. A below-floor git is
    /// RESOLVED (RepoSync still runs it, per E-03 AC7) but does NOT meet the
    /// floor. One combined "available" boolean could not say both, and whichever
    /// value it took would be a lie about the other half.
    #[test]
    fn a_below_floor_git_is_resolved_but_does_not_meet_the_floor() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let old = reposync_core::git::discover::GitVersion {
            major: 2,
            minor: 28,
            patch: 0,
        };

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::BelowFloor { version: old }),
            Some(std::path::PathBuf::from("C:\\git\\git.exe")),
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert!(
            d.git_resolved,
            "a below-floor git is still the git RepoSync runs"
        );
        assert!(!d.git_meets_floor);
        assert_eq!(d.git_version.as_deref(), Some("2.28.0"));
        assert_eq!(d.git_path.as_deref(), Some("C:\\git\\git.exe"));
    }

    /// The state that IS a stop: no git at all. Both booleans go false together,
    /// which is what distinguishes it from the below-floor case above.
    #[test]
    fn an_absent_git_is_neither_resolved_nor_at_the_floor() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Unavailable),
            None,
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert!(!d.git_resolved);
        assert!(!d.git_meets_floor);
        assert_eq!(d.git_version, None);
        assert_eq!(d.git_path, None);
    }

    #[test]
    fn an_available_git_is_resolved_and_at_the_floor() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        let v = reposync_core::git::discover::GitVersion {
            major: 2,
            minor: 40,
            patch: 1,
        };

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            Some(GitAvailability::Available { version: v }),
            Some(std::path::PathBuf::from("git")),
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert!(d.git_resolved);
        assert!(d.git_meets_floor);
        assert_eq!(d.git_version.as_deref(), Some("2.40.1"));
    }

    /// BL-NI-14 reaching the surface: the counters the tick loop folds each
    /// `TickReport` into are what the card actually displays.
    #[test]
    fn scheduler_counters_are_carried_through_to_the_payload() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            None,
            None,
            None,
            &health_with(42, 137, 3),
            true,
        );

        assert_eq!(d.scheduler_cycles, 42);
        assert_eq!(d.scheduler_repos_checked, 137);
        assert_eq!(
            d.scheduler_outcome_persist_failures, 3,
            "a silent retry storm has to be countable somewhere the user can see"
        );
        assert!(d.db_recovered);
    }

    /// The counts come from the real directory, not from a stored number, so an
    /// externally deleted file cannot leave the card claiming logs that are gone.
    #[test]
    fn log_file_counts_are_measured_from_disk() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().to_path_buf());
        std::fs::create_dir_all(paths.log_dir()).expect("create log dir");
        std::fs::write(paths.log_dir().join("reposync.2026-08-04.log"), b"hello")
            .expect("write log");

        let d = build_diagnostics(
            "0.9.0",
            &paths,
            None,
            None,
            None,
            None,
            &health_with(0, 0, 0),
            false,
        );

        assert_eq!(d.log_file_count, 1);
        assert_eq!(d.log_bytes, 5);
    }

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
        // Finding 1 regression: the git re-probe and the inherit-cadence reschedule
        // are INDEPENDENT. On a git-less machine (or any save that does not touch
        // the git path) the git portion is Unchanged, so the save NEVER rejects on
        // git and STILL applies a global-cadence change.
        //
        // Autostart used to be planned here too, carrying the property "a git-path
        // typo never skips an autostart toggle". Under BL-NI-18 the actuation moved
        // ahead of the persist and the git probe entirely, so that property is now
        // structural - there is no longer an ordering in which git could skip it.
        let p = plan_settings_reconcile(GitReconcile::Unchanged, true);
        assert!(
            !p.reject_git_path,
            "an unchanged git path must not reject the save (git-less machine)"
        );
        assert!(
            p.reschedule_inherit,
            "a global-cadence change reschedules when the git path is untouched"
        );

        // A changed-but-unusable git path rejects (BL-NI-26).
        let p = plan_settings_reconcile(GitReconcile::RejectUnavailable, false);
        assert!(p.reject_git_path, "a changed unusable git path rejects");
        assert!(
            !p.reschedule_inherit,
            "an unchanged cadence does not reschedule"
        );

        // A changed usable git path swaps in and does not reject.
        let p = plan_settings_reconcile(GitReconcile::Swapped, false);
        assert!(!p.reject_git_path, "a usable git swap does not reject");
        assert!(!p.reschedule_inherit);
    }

    #[test]
    fn autostart_to_persist_never_claims_an_actuation_that_failed() {
        // BL-NI-18 / Codex round-1 finding 1, the invariant startup reconciliation
        // rests on. A successful apply persists what the user asked for.
        assert!(autostart_to_persist(true, false, false));
        assert!(!autostart_to_persist(false, true, false));

        // A FAILED apply persists the value that is still true, so the row does not
        // claim a registration the OS never received. Without this, startup would
        // later see the disagreement, believe it was external, and adopt the OS
        // state - silently undoing an explicit user choice.
        assert!(
            !autostart_to_persist(true, false, true),
            "a failed enable must stay off, not record a registration that failed"
        );
        assert!(
            autostart_to_persist(false, true, true),
            "a failed disable must stay on, not record a removal that failed"
        );

        // No unknown-previous case exists by construction (Codex round-2 finding
        // 2): `settings_set` propagates a failed pre-read instead of guessing, so
        // this function can never be asked to invent a value.
        assert!(!autostart_to_persist(true, false, true));
        assert!(autostart_to_persist(true, true, true));
    }

    #[tokio::test]
    async fn settings_validation_rejects_before_anything_is_actuated() {
        // Codex round-2 finding 1. `settings_set` actuates launch-on-login BEFORE
        // the durable write, so an invalid payload must be rejected before that
        // point or a rejected save changes OS state anyway - and startup adoption
        // then makes it durable.
        //
        // Asserted against the same `validate_settings` the command pre-checks
        // with, over the realistic trigger: a cleared numeric input arriving as 0
        // alongside an autostart toggle.
        let bad = Settings {
            global_check_minutes: 0,
            quiet_hours_start: None,
            quiet_hours_end: None,
            notify_on_release: true,
            notify_on_failure: true,
            git_executable_path: None,
            editor_command: None,
            terminal_command: None,
            autostart: true,
            activity_retention_d: 90,
            github_token_present: false,
            auto_update_check: true,
            close_minimizes_to_tray: true,
        };
        assert!(
            reposync_core::store::validate_settings(&bad).is_err(),
            "a zero cadence must be rejected by the pre-check, before the plugin              call that would change the OS registration"
        );

        // The same payload with a valid cadence passes, so the pre-check is not
        // rejecting the autostart toggle itself.
        let good = Settings {
            global_check_minutes: 360,
            ..bad
        };
        assert!(reposync_core::store::validate_settings(&good).is_ok());
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
