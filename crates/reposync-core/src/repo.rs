//! repo - owned by E-02 / E-03 (the core flows where layers meet).
//!
//! Tracer slice: `add` (validate + inspect + persist) and `check_now`
//! (re-inspect + fetch + ahead/behind + record an activity row). Uses the sqlx
//! RUNTIME query API and unix-seconds timestamps (no chrono).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{Row, SqlitePool};

use crate::activity::{self, ActivityInput};
use crate::error::AppError;
use crate::git::{AheadBehind, InspectResult, SystemGitEngine};
use crate::ipc::{CheckResult, RepoId, UpdateMode, UpdateResult};
use crate::policy::{
    decide, Action, PolicyDecision, RepoState, RunOutcome, SkipReason, UpstreamState,
};

/// Current unix time in whole seconds.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Clear the consecutive-failure counter and the auto-pause flag for a repo after
/// a SUCCESSFUL manual check/update (the manual-recovery path).
///
/// The E-08 scheduler excludes `auto_paused = 1` repos from its due-query and
/// never runs an excluded repo, so a successful manual run is the only thing that
/// re-admits a repo the user has fixed. Without this, auto-pause would be a
/// permanent one-way trip (E-08 spec: "auto_paused resets to 0 on a successful
/// manual check or an explicit user resume").
async fn clear_failure_state(pool: &SqlitePool, repo_id: i64) -> Result<(), AppError> {
    sqlx::query(
        "UPDATE repo_local_state SET consecutive_failures = 0, auto_paused = 0 WHERE repo_id = ?",
    )
    .bind(repo_id)
    .execute(pool)
    .await?;
    Ok(())
}

/// Add a repository to the registry.
///
/// Validates the path, inspects it via git2, derives registry fields, and writes
/// a `repos` row plus an initial `repo_local_state` row. Re-adding the same path
/// yields [`AppError::DuplicateRepo`].
pub async fn add(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    path: &Path,
) -> Result<RepoId, AppError> {
    // 1. Validate the path.
    if !path.exists() {
        return Err(AppError::PathMissing {
            path: path.display().to_string(),
        });
    }
    if !path.is_dir() {
        return Err(AppError::NotADirectory {
            path: path.display().to_string(),
        });
    }

    // 2. Canonicalize so aliases of the same repo (case differences, trailing
    //    "/.", junctions/symlinks) collapse to one stored path and the
    //    UNIQUE(local_path) constraint catches duplicates. The path already
    //    passed the exists()/is_dir() checks above, so canonicalize() resolves.
    //    Fall back to the validated path on the rare canonicalize failure.
    //    `dunce::canonicalize` resolves exactly like `std::fs::canonicalize` but
    //    returns a clean, non-verbatim path (no `\\?\` extended-length prefix) so
    //    the stored `local_path` opens directly in explorer/editors/terminals.
    let canonical = dunce::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let path = canonical.as_path();

    // 3. Inspect (maps non-repo to AppError::NotARepo).
    let inspect = git.inspect(path)?;

    // 4. Derive registry fields from the canonical path.
    let local_name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    let local_path = path.display().to_string();
    let remote_origin_url = origin_url(path);
    let host_type = match &remote_origin_url {
        Some(url) if url.contains("github.com") => "github",
        _ => "unknown",
    };
    let created_at = now_secs();

    // 5. Insert the repos row; UNIQUE(local_path) violation -> DuplicateRepo.
    //    check_frequency_min is inserted as 0, the INHERIT sentinel: a new repo
    //    follows the global cadence (settings.global_check_minutes) until a user
    //    sets an explicit positive per-repo override. The schema default is 360,
    //    but that would read as a 6h override that ignores the global control, so
    //    `add` sets 0 explicitly rather than relying on the column default.
    let insert = sqlx::query(
        "INSERT INTO repos \
         (local_name, local_path, remote_origin_url, host_type, created_at, check_frequency_min) \
         VALUES (?, ?, ?, ?, ?, 0)",
    )
    .bind(&local_name)
    .bind(&local_path)
    .bind(&remote_origin_url)
    .bind(host_type)
    .bind(created_at)
    .execute(pool)
    .await;

    let repo_id = match insert {
        Ok(res) => res.last_insert_rowid(),
        Err(e) => {
            if is_unique_violation(&e) {
                return Err(AppError::DuplicateRepo { path: local_path });
            }
            return Err(AppError::from(e));
        }
    };

    // 6. Insert the initial repo_local_state row from the inspection.
    sqlx::query(
        "INSERT INTO repo_local_state \
         (repo_id, active_branch, head_sha, upstream_branch, is_dirty, is_detached) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(repo_id)
    .bind(&inspect.active_branch)
    .bind(&inspect.head_sha)
    .bind(&inspect.upstream_branch)
    .bind(inspect.is_dirty as i64)
    .bind(inspect.is_detached as i64)
    .execute(pool)
    .await?;

    Ok(RepoId(repo_id))
}

/// Run a "check now" for a tracked repo.
///
/// Re-inspects, fetches (recording the raw command/output even on failure),
/// computes ahead/behind when an upstream is known, applies the tracer-inline
/// decision policy, updates `repo_local_state`, and appends an
/// `activity_records` row. A non-zero fetch records the activity row, then
/// returns [`AppError::FetchFailed`].
pub async fn check_now(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    id: RepoId,
) -> Result<CheckResult, AppError> {
    let repo_id = id.0;

    // 1. Look up the path, stored upstream, and configured update mode. A missing
    //    id is NotFound (mirrors store::repo_get), not the generic db error
    //    fetch_one would yield via the From<sqlx::Error> impl on the "no rows"
    //    case.
    let row = sqlx::query("SELECT local_path, update_mode FROM repos WHERE id = ?")
        .bind(repo_id)
        .fetch_optional(pool)
        .await?;
    let row = row.ok_or_else(|| AppError::NotFound {
        entity: format!("repo {repo_id}"),
    })?;
    let local_path: String = row.try_get("local_path")?;
    let mode_str: String = row.try_get("update_mode")?;
    let mode = parse_update_mode(&mode_str);
    let path = Path::new(&local_path);

    // 2. Inspect local state.
    let local = git.inspect(path)?;
    let has_origin = has_origin_remote(path);
    let now = now_secs();

    // 3. Fetch ONLY when the mode needs remote data (M-2: check_only reports the
    //    local view and never touches the network; no/deleted-upstream skip up
    //    front). check_now never mutates either way - it reports, it does not pull.
    let fetch: Option<crate::git::FetchOutcome> = if needs_remote_fetch(&mode, &local, has_origin) {
        Some(git.fetch(path).await?)
    } else {
        None
    };
    let fetch_failed = matches!(&fetch, Some(f) if !f.success);

    // 4. State to report: re-inspect after a SUCCESSFUL fetch (M-3, so an upstream
    //    pruned by the fetch reads as deleted-upstream), else the local inspect.
    //    upstream is the fresh inspect's, never the stale stored ref.
    let inspect = match &fetch {
        Some(f) if f.success => git.inspect(path).unwrap_or_else(|_| local.clone()),
        _ => local.clone(),
    };
    let ahead_behind = compute_ahead_behind(git, path, &inspect).await;
    let upstream = resolve_upstream(inspect.upstream_branch.clone(), None);

    // 5. Report the decision via the E-07 engine (the same `decide` that drives
    //    update_now and the scheduler). A failed fetch is an operational failure
    //    (auth/network), surfaced as the reason; otherwise `decide` over the
    //    configured mode produces the reported decision/reason. check_now reports
    //    a would-be fast-forward; it never executes one.
    let (decision, reason): (String, Option<String>) = if fetch_failed {
        let why = match fetch.as_ref().expect("fetch_failed implies Some").class {
            crate::git::FetchClass::AuthFailure => "git.auth_failed",
            crate::git::FetchClass::NetworkFailure => "net.offline",
            _ => "git.fetch_failed",
        };
        ("skip-with-reason".to_string(), Some(why.to_string()))
    } else {
        let state = repo_state_from_reads(&inspect, &ahead_behind, has_origin);
        match decide(&state, &mode) {
            PolicyDecision::Act(Action::PullFastForward) => {
                ("would-fast-forward".to_string(), None)
            }
            PolicyDecision::Act(Action::Fetch) | PolicyDecision::Act(Action::ReportStatus) => {
                ("report-status".to_string(), None)
            }
            PolicyDecision::Skip(reason) => (
                "skip-with-reason".to_string(),
                Some(skip_reason_label(reason).to_string()),
            ),
        }
    };

    // 7. Update the cached local state. active_branch is refreshed from the
    //    fresh inspect so a branch switch since `add` is reflected; omitting it
    //    leaves stale state. upstream_branch is also refreshed here since it was
    //    already inspected.
    sqlx::query(
        "UPDATE repo_local_state SET \
         active_branch = ?, ahead_count = ?, behind_count = ?, is_dirty = ?, is_detached = ?, \
         head_sha = ?, upstream_branch = ?, last_checked_at = ?, last_attempted_at = ? \
         WHERE repo_id = ?",
    )
    .bind(&inspect.active_branch)
    .bind(ahead_behind.ahead)
    .bind(ahead_behind.behind)
    .bind(inspect.is_dirty as i64)
    .bind(inspect.is_detached as i64)
    .bind(&inspect.head_sha)
    .bind(&upstream)
    .bind(now)
    .bind(now)
    .bind(repo_id)
    .execute(pool)
    .await?;

    // 8. Append the activity record. A locally-decided check (no fetch) records no
    //    git command; a fetched check records the fetch capture.
    let status = if fetch_failed { "failed" } else { "success" };
    let summary = format!(
        "check: decision={decision}, ahead={:?}, behind={:?}",
        ahead_behind.ahead, ahead_behind.behind
    );
    activity::record(
        pool,
        &ActivityInput {
            repo_id,
            timestamp: Some(now),
            action_type: "check".into(),
            status: status.into(),
            reason_code: reason.clone(),
            summary: Some(summary),
            commit_range: None,
            raw_command: fetch.as_ref().map(|f| f.raw_command.clone()),
            raw_stdout: fetch.as_ref().map(|f| f.raw_stdout.clone()),
            raw_stderr: fetch.as_ref().map(|f| f.raw_stderr.clone()),
            exit_code: fetch.as_ref().and_then(|f| f.exit_code),
            duration_ms: fetch.as_ref().map(|f| f.duration_ms),
        },
    )
    .await;

    // 9. A failed fetch records the activity row above, then surfaces the error.
    if let Some(f) = fetch.as_ref().filter(|f| !f.success) {
        return Err(AppError::FetchFailed {
            exit_code: f.exit_code,
            stderr: f.raw_stderr.clone(),
        });
    }

    // 10. E-08 review fix (HIGH): reaching here means the check succeeded; clear the
    //     failure streak + auto-pause so a user-recovered repo is re-admitted to the
    //     scheduler's due-query (which excludes auto_paused = 1).
    clear_failure_state(pool, repo_id).await?;

    Ok(CheckResult {
        repo_id,
        decision,
        reason,
        ahead: ahead_behind.ahead,
        behind: ahead_behind.behind,
        is_dirty: inspect.is_dirty,
        is_detached: inspect.is_detached,
        checked_at: now,
    })
}

/// Run an "update now" for a tracked repo in the requested mode (E-07).
///
/// This is the SHARED decide -> execute -> record path: the manual
/// `repo_update_now` command and the E-08 scheduler both call it so a scheduled
/// update and a manual one run identically. The Tauri handler wraps this with the
/// `update-started` / `update-completed` event emission; this core function does
/// no event I/O (the core is Tauri-free).
///
/// Flow: look up the repo path, re-inspect, fetch to refresh the comparison base,
/// build the policy [`RepoState`], and call [`decide`] for `mode`. The returned
/// [`Action`] is executed through the git engine:
///
///   - [`Action::PullFastForward`] -> `pull --ff-only` (the one mutation);
///   - [`Action::Fetch`] -> the refreshing fetch already ran, so this is a no-op
///     beyond recording it;
///   - [`Action::ReportStatus`] -> nothing to do.
///
/// A [`PolicyDecision::Skip`] performs no git mutation and records the typed
/// reason. A minimal activity row is recorded either way (the full activity
/// writer + retention is E-09).
///
/// The returned [`UpdateResult`]'s `outcome` is one of the stable strings
/// `updated`, `up_to_date`, `skipped`, or `failed`; `mode` echoes the requested
/// mode; `commit_range` carries `before..after` when a fast-forward advanced the
/// tree.
async fn run_update_inner(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    id: RepoId,
    mode: UpdateMode,
) -> Result<(UpdateResult, RunOutcome), AppError> {
    let repo_id = id.0;
    let mode_label = update_mode_str(&mode).to_string();

    // 1. Resolve the repo path (NotFound on a missing id).
    let row = sqlx::query(
        "SELECT r.local_path AS local_path, s.upstream_branch AS upstream_branch \
         FROM repos r LEFT JOIN repo_local_state s ON s.repo_id = r.id \
         WHERE r.id = ?",
    )
    .bind(repo_id)
    .fetch_optional(pool)
    .await?;
    let row = row.ok_or_else(|| AppError::NotFound {
        entity: format!("repo {repo_id}"),
    })?;
    let local_path: String = row.try_get("local_path")?;
    let path = Path::new(&local_path);

    // 2. Re-inspect, capture the pre-update HEAD for the commit range.
    let before = git.inspect(path)?;
    let head_before = before.head_sha.clone();
    let now = now_secs();

    // 3. Fetch ONLY when the grid cell needs remote data (M-1). check_only never
    //    fetches; a dirty/detached/no-upstream/deleted-upstream pull_ff_only repo
    //    and a no/deleted-upstream fetch_only repo all decide from local state and
    //    never touch the network.
    let has_origin = has_origin_remote(path);
    let fetch: Option<crate::git::FetchOutcome> = if needs_remote_fetch(&mode, &before, has_origin)
    {
        Some(git.fetch(path).await?)
    } else {
        None
    };

    // 4. Decide over the right state and execute. A failed refreshing fetch
    //    short-circuits to `failed` (the comparison base is gone). Otherwise build
    //    the state from a fresh re-inspect when a fetch ran (M-3, so an upstream
    //    pruned by the fetch reads as deleted-upstream), else from the local
    //    inspect; then decide -> execute. PullFastForward re-checks dirtiness right
    //    before the pull (H-1).
    let exec = match &fetch {
        Some(f) if !f.success => exec_failed_fetch(f),
        maybe_fetch => {
            let state_inspect = if maybe_fetch.is_some() {
                git.inspect(path).unwrap_or_else(|_| before.clone())
            } else {
                before.clone()
            };
            let ahead_behind = compute_ahead_behind(git, path, &state_inspect).await;
            let state = repo_state_from_reads(&state_inspect, &ahead_behind, has_origin);
            let decision = decide(&state, &mode);
            execute_action(
                git,
                path,
                maybe_fetch.as_ref(),
                decision,
                head_before.as_deref(),
            )
            .await?
        }
    };
    let outcome = exec.outcome;
    let status = exec.status;
    let reason_code = exec.reason_code.clone();
    let commit_range = exec.commit_range.clone();

    // 7. Refresh the cached local state from the post-action reads.
    let post = git.inspect(path).unwrap_or_else(|_| before.clone());
    let post_ab = compute_ahead_behind(git, path, &post).await;
    let upstream = resolve_upstream(post.upstream_branch.clone(), None);
    let updated_at_col = if status == "success" && outcome == "updated" {
        Some(now)
    } else {
        None
    };
    sqlx::query(
        "UPDATE repo_local_state SET \
         active_branch = ?, ahead_count = ?, behind_count = ?, is_dirty = ?, is_detached = ?, \
         head_sha = ?, upstream_branch = ?, last_checked_at = ?, last_attempted_at = ?, \
         last_updated_at = COALESCE(?, last_updated_at) \
         WHERE repo_id = ?",
    )
    .bind(&post.active_branch)
    .bind(post_ab.ahead)
    .bind(post_ab.behind)
    .bind(post.is_dirty as i64)
    .bind(post.is_detached as i64)
    .bind(&post.head_sha)
    .bind(&upstream)
    .bind(now)
    .bind(now)
    .bind(updated_at_col)
    .bind(repo_id)
    .execute(pool)
    .await?;

    // 8. Record the activity row through the single E-09 sink (best-effort: a
    //    logging failure must not abort the update that already happened).
    let summary = format!("update: mode={mode_label}, outcome={outcome}");
    activity::record(
        pool,
        &ActivityInput {
            repo_id,
            timestamp: Some(now),
            action_type: "update".into(),
            status: status.into(),
            reason_code: reason_code.clone(),
            summary: Some(summary),
            commit_range: commit_range.clone(),
            raw_command: Some(exec.act_command.clone()),
            raw_stdout: Some(exec.act_stdout.clone()),
            raw_stderr: Some(exec.act_stderr.clone()),
            exit_code: exec.act_exit,
            duration_ms: Some(exec.act_duration),
        },
    )
    .await;

    // E-08 review fix (HIGH): a successful manual update clears the failure streak
    // and auto-pause, re-admitting a user-recovered repo to the scheduler's
    // due-query. A failed update leaves the counters for the scheduler's failure
    // state machine to manage.
    if status == "success" {
        clear_failure_state(pool, repo_id).await?;
    }

    let run_outcome = classify_run_outcome(status, reason_code.as_deref());
    Ok((
        UpdateResult {
            repo_id,
            mode: mode_label,
            outcome: outcome.to_string(),
            commit_range,
            ahead: post_ab.ahead,
            behind: post_ab.behind,
            updated_at: now,
        },
        run_outcome,
    ))
}

/// Run an "update now" for a tracked repo in the requested mode (E-07) - the
/// public manual-command entry point.
///
/// A thin wrapper over the shared [`run_update_inner`] path (the same path the
/// E-08 scheduler drives via [`update_now_scheduled`]), so a manual update and a
/// scheduled one execute identically. The Tauri handler wraps this with the
/// `update-started` / `update-completed` event emission; this core function does
/// no event I/O.
pub async fn update_now(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    id: RepoId,
    mode: UpdateMode,
) -> Result<UpdateResult, AppError> {
    run_update_inner(pool, git, id, mode).await.map(|(r, _)| r)
}

/// The classified result of a scheduled update: the IPC [`UpdateResult`] plus the
/// [`RunOutcome`] the E-08 failure state machine needs (the coarse `outcome`
/// string cannot tell an auth failure from a network failure, but the scheduler
/// must, to choose immediate-pause vs retry).
pub struct ScheduledUpdate {
    pub result: UpdateResult,
    pub run_outcome: RunOutcome,
}

/// The scheduler-facing variant of [`update_now`]: it runs the SAME shared
/// decide -> execute -> record path for the repo's CONFIGURED update mode (read
/// from `repos.update_mode`, not a caller-supplied mode) and additionally returns
/// the classified [`RunOutcome`]. The E-08 scheduler calls this so the safety
/// rules and the git execution live in exactly one place.
///
/// [`AppError::NotFound`] when no such repo id exists.
pub async fn update_now_scheduled(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    id: RepoId,
) -> Result<ScheduledUpdate, AppError> {
    let row = sqlx::query("SELECT update_mode FROM repos WHERE id = ?")
        .bind(id.0)
        .fetch_optional(pool)
        .await?;
    let row = row.ok_or_else(|| AppError::NotFound {
        entity: format!("repo {}", id.0),
    })?;
    let mode_str: String = row.try_get("update_mode")?;
    let mode = parse_update_mode(&mode_str);

    let (result, run_outcome) = run_update_inner(pool, git, id, mode).await?;
    Ok(ScheduledUpdate {
        result,
        run_outcome,
    })
}

/// Classify a [`run_update_inner`] execution into the [`RunOutcome`] the E-07
/// failure state machine consumes.
///
/// A `success` status - which includes a normal SKIP (dirty, no-upstream,
/// deleted-upstream, diverged, detached) - is [`RunOutcome::Success`] and never
/// counts toward auto-pause: a skip is an expected non-action, not an operational
/// failure. Only a `failed` status (an executed fetch/pull that genuinely failed)
/// maps to a failure class by its reason code. An unrecognized failure reason is
/// treated as a transient network failure (the retry path), never an
/// auth-pause - the conservative default.
fn classify_run_outcome(status: &str, reason_code: Option<&str>) -> RunOutcome {
    if status == "success" {
        return RunOutcome::Success;
    }
    match reason_code {
        Some("git.auth_failed") => RunOutcome::AuthFailure,
        Some("net.offline") => RunOutcome::NetworkFailure,
        Some("git.ff_not_possible") => RunOutcome::FfNotPossible,
        _ => RunOutcome::NetworkFailure,
    }
}

/// Whether the requested mode and the repo's LOCAL state require a network fetch
/// to reach the correct decision (M-1/M-2: never fetch for grid cells the engine
/// resolves from local state alone).
///
/// - `check_only` / non-V1: never fetch (report local, or reject).
/// - `fetch_only`: fetch when there is something to refresh - a live tracking
///   upstream, or a detached HEAD (the grid fetches refs even when detached);
///   no-upstream and deleted-upstream skip up front.
/// - `pull_ff_only`: fetch only for a clean, non-detached, live-tracking repo -
///   the clean/ahead/behind/diverged cells whose decision depends on FRESH
///   ahead/behind. dirty, detached, no-upstream, and deleted-upstream are local
///   skips that need no network.
fn needs_remote_fetch(mode: &UpdateMode, inspect: &InspectResult, has_origin: bool) -> bool {
    let upstream = classify_upstream(inspect, has_origin);
    match mode {
        UpdateMode::CheckOnly | UpdateMode::PullStandard | UpdateMode::PullRebase => false,
        UpdateMode::FetchOnly => inspect.is_detached || upstream == UpstreamState::Tracking,
        UpdateMode::PullFfOnly => {
            !inspect.is_dirty && !inspect.is_detached && upstream == UpstreamState::Tracking
        }
    }
}

/// Ahead/behind for `inspect`'s upstream via a LOCAL rev-list (no network).
/// `None`/`None` when there is no resolvable upstream or the read fails
/// (conservative: an unknown comparison base is `None`, never a fabricated 0).
async fn compute_ahead_behind(
    git: &SystemGitEngine,
    path: &Path,
    inspect: &InspectResult,
) -> AheadBehind {
    match resolve_upstream(inspect.upstream_branch.clone(), None) {
        Some(u) => git.ahead_behind(path, &u).await.unwrap_or(AheadBehind {
            ahead: None,
            behind: None,
        }),
        None => AheadBehind {
            ahead: None,
            behind: None,
        },
    }
}

/// Build the [`ExecOutcome`] for a refreshing fetch that FAILED: the update cannot
/// proceed because the comparison base the decision rested on is gone. Carries the
/// fetch's failure class as the reason code and records the fetch capture.
fn exec_failed_fetch(fetch: &crate::git::FetchOutcome) -> ExecOutcome {
    let reason = match fetch.class {
        crate::git::FetchClass::AuthFailure => "git.auth_failed",
        crate::git::FetchClass::NetworkFailure => "net.offline",
        _ => "git.fetch_failed",
    };
    ExecOutcome {
        outcome: "failed",
        status: "failed",
        reason_code: Some(reason.to_string()),
        commit_range: None,
        act_command: fetch.raw_command.clone(),
        act_stdout: fetch.raw_stdout.clone(),
        act_stderr: fetch.raw_stderr.clone(),
        act_exit: fetch.exit_code,
        act_duration: fetch.duration_ms,
    }
}

/// The result of executing a [`PolicyDecision`] in [`update_now`]: the stable
/// outcome/status strings, the optional reason code and commit range, and the
/// raw capture of the git op that should be recorded in the activity row.
struct ExecOutcome {
    /// The `UpdateResult.outcome` string: `updated`, `up_to_date`, `skipped`, or
    /// `failed`.
    outcome: &'static str,
    /// The activity row `status`: `success` or `failed`.
    status: &'static str,
    /// The typed reason code for a skip or failure (an `AppError` code).
    reason_code: Option<String>,
    /// `before..after` when a fast-forward advanced the tree.
    commit_range: Option<String>,
    /// The raw capture of the recorded git op (the fetch, or the pull when one
    /// ran).
    act_command: String,
    act_stdout: String,
    act_stderr: String,
    act_exit: Option<i32>,
    act_duration: i64,
}

/// Execute the decided [`PolicyDecision`] through the git engine, returning the
/// [`ExecOutcome`] the caller records. Split out of [`update_now`] so the
/// outcome accumulation is a single expression per branch and the decide ->
/// execute mapping reads as a table.
///
/// `fetch` is `Some` when the grid cell needed a remote refresh (and is then a
/// SUCCESSFUL fetch, since the caller short-circuits a failed one via
/// [`exec_failed_fetch`]); it is `None` for a locally-decided cell that issued no
/// git command. The fetch capture is the default activity record (a fast-forward
/// pull replaces it with its own). The `PullFastForward` arm re-inspects dirtiness
/// immediately before the pull (H-1).
async fn execute_action(
    git: &SystemGitEngine,
    path: &Path,
    fetch: Option<&crate::git::FetchOutcome>,
    decision: PolicyDecision,
    head_before: Option<&str>,
) -> Result<ExecOutcome, AppError> {
    // Default activity capture = the refreshing fetch when one ran, else empty (a
    // locally-decided cell issues no git command, per M-1).
    let from_fetch = |outcome: &'static str,
                      status: &'static str,
                      reason_code: Option<String>,
                      commit_range: Option<String>| ExecOutcome {
        outcome,
        status,
        reason_code,
        commit_range,
        act_command: fetch.map(|f| f.raw_command.clone()).unwrap_or_default(),
        act_stdout: fetch.map(|f| f.raw_stdout.clone()).unwrap_or_default(),
        act_stderr: fetch.map(|f| f.raw_stderr.clone()).unwrap_or_default(),
        act_exit: fetch.and_then(|f| f.exit_code),
        act_duration: fetch.map(|f| f.duration_ms).unwrap_or(0),
    };

    // A failed fetch is handled by the caller (exec_failed_fetch); here `fetch` is
    // either None (locally decided, no fetch) or a SUCCESSFUL fetch.
    match decision {
        PolicyDecision::Act(Action::PullFastForward) => {
            // H-1: re-inspect dirtiness immediately before the one mutating action.
            // `git pull --ff-only` only refuses a CONFLICTING dirty tree, so the
            // "never fast-forward a dirty tree" guarantee is enforced HERE, in our
            // code, not git's internals. A tree dirtied since the decide skips.
            if git.inspect(path).map(|i| i.is_dirty).unwrap_or(false) {
                return Ok(from_fetch(
                    "skipped",
                    "success",
                    Some(SkipReason::Dirty.code().to_string()),
                    None,
                ));
            }
            let pull = git.pull_ff_only(path).await?;
            let mut exec = ExecOutcome {
                outcome: "failed",
                status: "failed",
                reason_code: None,
                commit_range: None,
                act_command: pull.raw_command.clone(),
                act_stdout: pull.raw_stdout.clone(),
                act_stderr: pull.raw_stderr.clone(),
                act_exit: pull.exit_code,
                act_duration: pull.duration_ms,
            };
            if pull.success {
                exec.status = "success";
                exec.outcome = if pull.class == crate::git::PullClass::NoOp {
                    "up_to_date"
                } else {
                    "updated"
                };
                // Capture the post-update HEAD for the commit range.
                if let Ok(after) = git.inspect(path) {
                    if let (Some(b), Some(a)) = (head_before, after.head_sha.as_deref()) {
                        if b != a {
                            exec.commit_range = Some(format!("{b}..{a}"));
                        }
                    }
                }
            } else {
                exec.reason_code = Some(
                    match pull.class {
                        crate::git::PullClass::FfNotPossible => "git.ff_not_possible",
                        crate::git::PullClass::AuthFailure => "git.auth_failed",
                        crate::git::PullClass::NetworkFailure => "net.offline",
                        _ => "git.command_failed",
                    }
                    .to_string(),
                );
            }
            Ok(exec)
        }
        PolicyDecision::Act(Action::Fetch) => {
            // fetch_only's action: the refreshing fetch ran (fetch is Some here).
            let outcome = match fetch {
                Some(f) if f.class == crate::git::FetchClass::Success => "updated",
                _ => "up_to_date",
            };
            Ok(from_fetch(outcome, "success", None, None))
        }
        PolicyDecision::Act(Action::ReportStatus) => {
            Ok(from_fetch("up_to_date", "success", None, None))
        }
        PolicyDecision::Skip(reason) => Ok(from_fetch(
            "skipped",
            "success",
            Some(skip_reason_label(reason).to_string()),
            None,
        )),
    }
}

/// Decide which upstream ref `check_now` compares against.
///
/// The freshly inspected upstream is AUTHORITATIVE: a fresh `None` means the
/// branch currently has no resolvable upstream (e.g. it was removed after the
/// repo was added, the deleted-upstream state), so ahead/behind must be unknown
/// (`None`) rather than a comparison against a stale stored ref. The DB's
/// `stored_upstream` is therefore intentionally ignored; it is accepted only so
/// the call site and this decision stay self-documenting (and so the rule is
/// unit-testable in isolation). It used to be a fallback - a tracer crutch from
/// when inspect did not report upstream reliably - which masked a removed
/// upstream by re-introducing the old ref.
fn resolve_upstream(fresh: Option<String>, _stored: Option<String>) -> Option<String> {
    fresh
}

/// Classify HEAD's upstream relationship for the E-07 policy engine from the
/// E-03 reads.
///
/// The policy engine needs to tell no-upstream from deleted-upstream, but both
/// report `upstream_branch = None` from inspect. The disambiguator is whether the
/// repo has an `origin` remote configured: a deleted-upstream repo was cloned (so
/// `origin` exists, but the tracking ref was pruned), while a no-upstream repo is
/// standalone (no remote at all). A detached HEAD has no branch upstream and is
/// classified as `None` here; the engine keys its detached handling off the
/// `is_detached` flag, not this value.
fn classify_upstream(inspect: &InspectResult, has_origin: bool) -> UpstreamState {
    if inspect.upstream_branch.is_some() {
        UpstreamState::Tracking
    } else if !inspect.is_detached && has_origin {
        UpstreamState::Deleted
    } else {
        UpstreamState::None
    }
}

/// Build the policy engine's [`RepoState`] from the E-03 reads (the mapping the
/// manual command path and the scheduler both perform).
fn repo_state_from_reads(
    inspect: &InspectResult,
    ahead_behind: &AheadBehind,
    has_origin: bool,
) -> RepoState {
    RepoState::new(
        inspect.is_dirty,
        inspect.is_detached,
        classify_upstream(inspect, has_origin),
        ahead_behind.ahead,
        ahead_behind.behind,
    )
}

/// Whether the repo at `path` has an `origin` remote configured (the no-upstream
/// vs deleted-upstream disambiguator). Best-effort via git2; `false` on any read
/// failure (treated as standalone, the conservative no-upstream classification).
fn has_origin_remote(path: &Path) -> bool {
    git2::Repository::open(path)
        .ok()
        .and_then(|repo| repo.find_remote("origin").ok().map(|_| ()))
        .is_some()
}

/// Map a [`SkipReason`] to the stable check-decision reason string surfaced in
/// the [`CheckResult`] and the activity row's `reason_code`. Uses the
/// skip-reason's [`AppError`] code so the reason is machine-readable.
fn skip_reason_label(reason: SkipReason) -> &'static str {
    reason.code()
}

/// Parse the stored `update_mode` column value (snake_case, per the IPC enum's
/// serde rename) into an [`UpdateMode`].
///
/// An unrecognized value defaults to [`UpdateMode::FetchOnly`], the schema
/// default - a defensive choice so a corrupt/foreign value never panics and
/// never silently escalates to a mutating mode (fetch_only is non-mutating).
fn parse_update_mode(s: &str) -> UpdateMode {
    match s {
        "check_only" => UpdateMode::CheckOnly,
        "fetch_only" => UpdateMode::FetchOnly,
        "pull_ff_only" => UpdateMode::PullFfOnly,
        "pull_standard" => UpdateMode::PullStandard,
        "pull_rebase" => UpdateMode::PullRebase,
        _ => UpdateMode::FetchOnly,
    }
}

/// The snake_case wire/DB string for an [`UpdateMode`] (the inverse of
/// [`parse_update_mode`]), used to persist the policy and label results.
fn update_mode_str(mode: &UpdateMode) -> &'static str {
    match mode {
        UpdateMode::CheckOnly => "check_only",
        UpdateMode::FetchOnly => "fetch_only",
        UpdateMode::PullFfOnly => "pull_ff_only",
        UpdateMode::PullStandard => "pull_standard",
        UpdateMode::PullRebase => "pull_rebase",
    }
}

/// Best-effort origin remote URL via git2. `None` if absent or unreadable.
fn origin_url(path: &Path) -> Option<String> {
    let repo = git2::Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().ok().map(|s| s.to_string())
}

/// Whether a sqlx error is a SQLite UNIQUE constraint violation.
fn is_unique_violation(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        // SQLite reports UNIQUE failures with code "2067" (extended) / "19"
        // (primary). Matching the message is the portable check across sqlx
        // versions; codes are checked first when present.
        if let Some(code) = db_err.code() {
            if code == "2067" || code == "1555" || code == "19" {
                return true;
            }
        }
        let msg = db_err.message().to_ascii_lowercase();
        return msg.contains("unique constraint failed");
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::git::fixtures::{build_fixture, FixtureState};
    use tempfile::TempDir;

    // --- H2: fresh inspect upstream is authoritative over the stored ref -------

    #[test]
    fn resolve_upstream_fresh_none_overrides_stored() {
        // The deleted-upstream case: fresh inspect reports None (the branch's
        // upstream was removed after `add`), but the DB still has the old ref.
        // Resolution MUST yield None - a comparison against the stale stored ref
        // would mislabel a deleted upstream as a real ahead/behind base. Before
        // the fix this was `fresh.or(stored)`, which re-introduced the stale ref.
        let resolved = resolve_upstream(None, Some("refs/remotes/origin/main".to_string()));
        assert_eq!(
            resolved, None,
            "a fresh None must NOT fall back to the stored upstream"
        );
    }

    #[test]
    fn resolve_upstream_prefers_fresh_when_present() {
        // When fresh inspection has an upstream, it wins (and the stored value,
        // even if different, is irrelevant).
        let resolved = resolve_upstream(
            Some("refs/remotes/origin/feature".to_string()),
            Some("refs/remotes/origin/stale".to_string()),
        );
        assert_eq!(
            resolved.as_deref(),
            Some("refs/remotes/origin/feature"),
            "the fresh upstream is authoritative"
        );
    }

    // --- update_mode parse/format round-trip ----------------------------------

    #[test]
    fn update_mode_parse_format_round_trip() {
        for (s, mode) in [
            ("check_only", UpdateMode::CheckOnly),
            ("fetch_only", UpdateMode::FetchOnly),
            ("pull_ff_only", UpdateMode::PullFfOnly),
            ("pull_standard", UpdateMode::PullStandard),
            ("pull_rebase", UpdateMode::PullRebase),
        ] {
            assert_eq!(update_mode_str(&parse_update_mode(s)), s, "round-trip {s}");
            assert_eq!(update_mode_str(&mode), s, "format {s}");
        }
        // An unknown/corrupt value defaults to the non-mutating fetch_only.
        assert_eq!(
            update_mode_str(&parse_update_mode("nonsense")),
            "fetch_only",
            "an unrecognized mode must default to the non-mutating fetch_only"
        );
    }

    // --- upstream classification ----------------------------------------------

    #[test]
    fn classify_upstream_distinguishes_none_deleted_tracking() {
        let with_up = InspectResult {
            head_sha: Some("a".into()),
            active_branch: Some("main".into()),
            is_dirty: false,
            is_detached: false,
            upstream_branch: Some("origin/main".into()),
        };
        assert_eq!(
            classify_upstream(&with_up, true),
            UpstreamState::Tracking,
            "an inspected upstream is Tracking"
        );

        let no_up = InspectResult {
            upstream_branch: None,
            ..with_up.clone()
        };
        // No upstream + an origin remote present -> Deleted (pruned tracking ref).
        assert_eq!(classify_upstream(&no_up, true), UpstreamState::Deleted);
        // No upstream + no origin remote -> None (standalone repo).
        assert_eq!(classify_upstream(&no_up, false), UpstreamState::None);

        // Detached HEAD is always None here regardless of remote presence.
        let detached = InspectResult {
            is_detached: true,
            active_branch: None,
            upstream_branch: None,
            ..with_up
        };
        assert_eq!(classify_upstream(&detached, true), UpstreamState::None);
    }

    /// Build a git2 repo with one commit at `dir`. Returns nothing; panics on
    /// failure (test helper).
    fn init_repo_with_commit(dir: &Path) {
        let repo = git2::Repository::init(dir).expect("init repo");
        std::fs::write(dir.join("README.md"), "hello\n").expect("write file");

        let mut index = repo.index().expect("index");
        index.add_path(Path::new("README.md")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");

        let sig = git2::Signature::now("Tracer Test", "tracer@example.com").expect("signature");
        repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .expect("commit");
    }

    async fn fresh_pool(dir: &Path) -> SqlitePool {
        let db_file = dir.join("repo-test.db");
        let pool = db::open_pool(&db_file).await.expect("open_pool");
        db::run_migrations(&pool).await.expect("migrations");
        pool
    }

    #[tokio::test]
    async fn add_then_duplicate_then_check() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping add_then_duplicate_then_check: git not resolvable");
            return;
        };

        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        let repotmp = TempDir::new().expect("repo tempdir");
        init_repo_with_commit(repotmp.path());

        // add() writes repos + repo_local_state rows.
        let id = add(&pool, &git, repotmp.path()).await.expect("add ok");
        assert!(id.0 >= 1);

        let repos_count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM repos")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("c")
            .unwrap();
        assert_eq!(repos_count, 1);

        let state_count: i64 =
            sqlx::query("SELECT COUNT(*) AS c FROM repo_local_state WHERE repo_id = ?")
                .bind(id.0)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("c")
                .unwrap();
        assert_eq!(state_count, 1);

        // Re-adding the same path -> DuplicateRepo.
        let dup = add(&pool, &git, repotmp.path()).await;
        assert!(
            matches!(dup, Err(AppError::DuplicateRepo { .. })),
            "expected DuplicateRepo, got {dup:?}"
        );

        // check_now on this repo (no remote -> no upstream) reports local status
        // and does NOT fetch (M-2: no-upstream skips up front), yet it still writes
        // a 'check' activity row and updates last_checked_at.
        let _ = check_now(&pool, &git, id).await;

        let act = sqlx::query(
            "SELECT raw_command, reason_code FROM activity_records \
             WHERE repo_id = ? AND action_type = 'check'",
        )
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .expect("activity row present");
        let raw_command: Option<String> = act.try_get("raw_command").unwrap();
        let reason_code: Option<String> = act.try_get("reason_code").unwrap();
        assert!(
            raw_command.is_none(),
            "a no-upstream check reports local status without fetching, so no git \
             command is recorded, got {raw_command:?}"
        );
        assert_eq!(
            reason_code.as_deref(),
            Some("git.no_upstream"),
            "a repo with no remote is a no-upstream skip"
        );

        let last_checked: Option<i64> =
            sqlx::query("SELECT last_checked_at FROM repo_local_state WHERE repo_id = ?")
                .bind(id.0)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("last_checked_at")
                .unwrap();
        assert!(last_checked.is_some(), "last_checked_at should be set");
    }

    /// Delete the working clone's remote-tracking ref so a fresh inspect reports
    /// no upstream, modelling "the upstream was removed after the repo was added".
    fn delete_tracking_ref(working: &Path, branch: &str) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(working)
            .args(["update-ref", "-d", &format!("refs/remotes/origin/{branch}")])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        assert!(ok, "deleting the tracking ref should succeed");
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn check_now_with_removed_upstream_yields_none_not_stale_comparison() {
        // H2 (end-to-end): a repo whose upstream is removed AFTER `add` (so the
        // DB has a stored upstream but a fresh inspect reports None) must yield
        // ahead/behind = None, not a comparison against the stale stored ref.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping check_now_with_removed_upstream...: git not resolvable");
            return;
        };

        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        // A clean fixture starts WITH an upstream (refs/remotes/origin/main), so
        // `add` records a stored upstream in repo_local_state.
        let fx = build_fixture(FixtureState::Clean);
        let working = fx.working_path();

        let id = add(&pool, &git, working).await.expect("add ok");

        // Sanity: the stored upstream is present after add.
        let stored: Option<String> =
            sqlx::query("SELECT upstream_branch FROM repo_local_state WHERE repo_id = ?")
                .bind(id.0)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("upstream_branch")
                .unwrap();
        assert!(
            stored.is_some(),
            "the clean fixture should record a stored upstream at add time"
        );

        // Now remove the upstream tracking ref: a fresh inspect will report None.
        delete_tracking_ref(working, "main");

        // check_now: even though the DB still has the stored upstream, the fresh
        // inspect of None must win, so ahead/behind are None (no stale compare).
        let result = check_now(&pool, &git, id).await.expect("check_now ok");
        assert_eq!(
            result.ahead, None,
            "removed upstream must report ahead = None, not a stale comparison"
        );
        assert_eq!(
            result.behind, None,
            "removed upstream must report behind = None, not a stale comparison"
        );
        // This clone HAS an origin remote (it was cloned from the fixture's bare
        // upstream) but its tracking ref was deleted, so the E-07 engine
        // correctly classifies it as DELETED-upstream, distinct from a standalone
        // no-upstream repo. (The old inline tracer policy conflated the two into
        // "no upstream"; the engine now tells them apart.)
        assert_eq!(
            result.reason.as_deref(),
            Some("git.deleted_upstream"),
            "a clone whose tracking ref was pruned is deleted-upstream, the \
             engine's machine-readable code"
        );
        assert_eq!(
            result.decision, "skip-with-reason",
            "a removed upstream is a typed skip"
        );
    }

    #[tokio::test]
    async fn check_now_missing_repo_is_not_found() {
        // M-3: check_now against a migrated DB with no such repo id must return
        // AppError::NotFound, not a generic db error. The lookup is step 1, so this
        // returns before any git inspection - a fresh pool with no rows suffices and
        // git availability is irrelevant to the assertion.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping check_now_missing_repo_is_not_found: git not resolvable");
            return;
        };

        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        // No repo was ever added, so id 9999 does not exist.
        let result = check_now(&pool, &git, RepoId(9999)).await;
        assert!(
            matches!(result, Err(AppError::NotFound { .. })),
            "check_now on a missing repo id must be NotFound, got {result:?}"
        );
    }

    #[tokio::test]
    async fn add_via_non_canonical_spelling_is_duplicate() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping add_via_non_canonical_spelling_is_duplicate: git not resolvable");
            return;
        };

        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        let repotmp = TempDir::new().expect("repo tempdir");
        init_repo_with_commit(repotmp.path());

        // First add via the plain path.
        let id = add(&pool, &git, repotmp.path()).await.expect("add ok");
        assert!(id.0 >= 1);

        // Re-add via a non-canonical spelling (trailing "/."). canonicalize()
        // collapses it to the same path, so UNIQUE(local_path) must reject it.
        let aliased = repotmp.path().join(".");
        let dup = add(&pool, &git, &aliased).await;
        assert!(
            matches!(dup, Err(AppError::DuplicateRepo { .. })),
            "expected DuplicateRepo for non-canonical spelling, got {dup:?}"
        );

        // Only one repos row exists despite the two spellings.
        let repos_count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM repos")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("c")
            .unwrap();
        assert_eq!(repos_count, 1, "aliased path must not create a second row");
    }

    // --- update_now (E-07): decide -> execute -> record, against real fixtures -

    /// Set a repo's update_mode column (the policy persistence under test).
    async fn set_mode(pool: &SqlitePool, id: RepoId, mode: &str) {
        sqlx::query("UPDATE repos SET update_mode = ? WHERE id = ?")
            .bind(mode)
            .bind(id.0)
            .execute(pool)
            .await
            .expect("set update_mode");
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn update_now_fast_forwards_a_behind_repo() {
        // The one mutating cell end-to-end: a clean, behind repo under
        // pull_ff_only fast-forwards and reports `updated`.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_fast_forwards_a_behind_repo: git not resolvable");
            return;
        };
        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        let fx = build_fixture(FixtureState::Behind);
        let id = add(&pool, &git, fx.working_path()).await.expect("add ok");
        set_mode(&pool, id, "pull_ff_only").await;

        let result = update_now(&pool, &git, id, UpdateMode::PullFfOnly)
            .await
            .expect("update_now ok");
        assert_eq!(result.mode, "pull_ff_only");
        assert_eq!(
            result.outcome, "updated",
            "a clean behind repo must fast-forward to `updated`"
        );
        assert!(
            result.commit_range.is_some(),
            "a fast-forward that advanced the tree records a commit range"
        );
        assert_eq!(
            result.behind,
            Some(0),
            "the repo is level with upstream after the fast-forward"
        );

        // An 'update' activity row was recorded with the pull capture.
        let row = sqlx::query(
            "SELECT status, reason_code, raw_command FROM activity_records \
             WHERE repo_id = ? AND action_type = 'update'",
        )
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .expect("update activity row present");
        let status: String = row.try_get("status").unwrap();
        let raw_command: Option<String> = row.try_get("raw_command").unwrap();
        assert_eq!(status, "success");
        assert!(
            raw_command
                .as_deref()
                .map(|s| s.contains("pull --ff-only"))
                .unwrap_or(false),
            "the update activity row records the pull invocation"
        );
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn update_now_skips_a_dirty_repo_without_mutating() {
        // AC3 end-to-end: a dirty repo under pull_ff_only skips-dirty, never
        // mutating, and records the typed reason code.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_skips_a_dirty_repo_without_mutating: git missing");
            return;
        };
        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        let fx = build_fixture(FixtureState::Dirty);
        let id = add(&pool, &git, fx.working_path()).await.expect("add ok");
        set_mode(&pool, id, "pull_ff_only").await;

        let result = update_now(&pool, &git, id, UpdateMode::PullFfOnly)
            .await
            .expect("update_now ok");
        assert_eq!(
            result.outcome, "skipped",
            "a dirty tree is never mutated; the update is skipped"
        );

        let reason: Option<String> = sqlx::query(
            "SELECT reason_code FROM activity_records \
             WHERE repo_id = ? AND action_type = 'update'",
        )
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .unwrap()
        .try_get("reason_code")
        .unwrap();
        assert_eq!(
            reason.as_deref(),
            Some("git.dirty_tree"),
            "the skip records the dirty reason code"
        );

        // The working tree was NOT mutated: it is still dirty.
        let still = git.inspect(fx.working_path()).expect("inspect");
        assert!(still.is_dirty, "the skip must not have touched the tree");
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn update_now_check_only_never_mutates_a_behind_repo() {
        // A behind repo under check_only reports up_to_date (no fetch decision to
        // act on beyond the refreshing fetch) and never fast-forwards.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_check_only_never_mutates_a_behind_repo: git missing");
            return;
        };
        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        let fx = build_fixture(FixtureState::Behind);
        let id = add(&pool, &git, fx.working_path()).await.expect("add ok");

        let head_before = git.inspect(fx.working_path()).unwrap().head_sha;
        let result = update_now(&pool, &git, id, UpdateMode::CheckOnly)
            .await
            .expect("update_now ok");
        assert_eq!(result.outcome, "up_to_date");
        assert!(
            result.commit_range.is_none(),
            "check_only must not advance the tree"
        );
        let head_after = git.inspect(fx.working_path()).unwrap().head_sha;
        assert_eq!(
            head_before, head_after,
            "check_only must leave HEAD untouched"
        );
    }

    #[tokio::test]
    async fn update_now_missing_repo_is_not_found() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_missing_repo_is_not_found: git missing");
            return;
        };
        let dbtmp = TempDir::new().expect("db tempdir");
        let pool = fresh_pool(dbtmp.path()).await;

        let result = update_now(&pool, &git, RepoId(424242), UpdateMode::PullFfOnly).await;
        assert!(
            matches!(result, Err(AppError::NotFound { .. })),
            "update_now on a missing repo id must be NotFound, got {result:?}"
        );
    }

    // --- E-07 review fixes: execution-path ordering (M-1/M-2/M-3) + H-1 dirty re-check ---

    /// Read the `raw_command` of the single 'update' activity row for a repo.
    async fn update_raw_command(pool: &SqlitePool, id: RepoId) -> Option<String> {
        sqlx::query(
            "SELECT raw_command FROM activity_records \
             WHERE repo_id = ? AND action_type = 'update'",
        )
        .bind(id.0)
        .fetch_one(pool)
        .await
        .unwrap()
        .try_get("raw_command")
        .unwrap()
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn update_now_check_only_issues_no_fetch() {
        // M-1/M-2: check_only must never touch the network, even for a behind repo
        // (the grid's check_only column is report-no-action for every state). The
        // recorded update row must carry no fetch command.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_check_only_issues_no_fetch: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;
        let fx = build_fixture(FixtureState::Behind);
        let id = add(&pool, &git, fx.working_path()).await.expect("add");

        let _ = update_now(&pool, &git, id, UpdateMode::CheckOnly)
            .await
            .expect("update_now ok");
        let raw = update_raw_command(&pool, id).await;
        assert!(
            raw.as_deref().map(|s| !s.contains("fetch")).unwrap_or(true),
            "check_only must not fetch, got raw_command={raw:?}"
        );
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn update_now_dirty_pull_ff_issues_no_fetch() {
        // pull_ff_only on a dirty repo is a LOCAL skip (the grid says Skip(dirty)):
        // it must not fetch.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_dirty_pull_ff_issues_no_fetch: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;
        let fx = build_fixture(FixtureState::Dirty);
        let id = add(&pool, &git, fx.working_path()).await.expect("add");
        set_mode(&pool, id, "pull_ff_only").await;

        let result = update_now(&pool, &git, id, UpdateMode::PullFfOnly)
            .await
            .expect("update_now ok");
        assert_eq!(result.outcome, "skipped");
        let raw = update_raw_command(&pool, id).await;
        assert!(
            raw.as_deref().map(|s| !s.contains("fetch")).unwrap_or(true),
            "dirty pull_ff_only skips locally without fetching, got {raw:?}"
        );
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn update_now_no_upstream_fetch_only_issues_no_fetch() {
        // fetch_only on a no-upstream repo skips up front (nothing to fetch from):
        // it must not fetch.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping update_now_no_upstream_fetch_only_issues_no_fetch: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;
        let fx = build_fixture(FixtureState::NoUpstream);
        let id = add(&pool, &git, fx.working_path()).await.expect("add");
        set_mode(&pool, id, "fetch_only").await;

        let result = update_now(&pool, &git, id, UpdateMode::FetchOnly)
            .await
            .expect("update_now ok");
        assert_eq!(result.outcome, "skipped");
        let raw = update_raw_command(&pool, id).await;
        assert!(
            raw.as_deref().map(|s| !s.contains("fetch")).unwrap_or(true),
            "no-upstream fetch_only skips without fetching, got {raw:?}"
        );
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn execute_action_rechecks_dirty_before_pull() {
        // H-1: even given a PullFastForward decision, execute_action re-inspects
        // the working tree immediately before the pull and SKIPS if dirty - our own
        // guard, since `git pull --ff-only` would fast-forward a NON-CONFLICTING
        // dirty tree. Simulate the race: a behind repo whose tree is dirtied after
        // the decide but before execution.
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping execute_action_rechecks_dirty_before_pull: git missing");
            return;
        };
        let fx = build_fixture(FixtureState::Behind);
        let working = fx.working_path();

        // A real refreshing fetch (the fixture's bare upstream is local).
        let fetch = git.fetch(working).await.expect("fetch ok");
        let head_before = git.inspect(working).unwrap().head_sha;

        // Dirty the tree AFTER the fetch/decide (an untracked file: non-conflicting,
        // so git --ff-only itself would NOT refuse it - only our guard does).
        std::fs::write(working.join("uncommitted-local-edit.txt"), "dirty\n")
            .expect("dirty the tree");
        assert!(
            git.inspect(working).unwrap().is_dirty,
            "the tree must read dirty for this test to exercise the H-1 guard"
        );

        let exec = execute_action(
            &git,
            working,
            Some(&fetch),
            PolicyDecision::Act(Action::PullFastForward),
            head_before.as_deref(),
        )
        .await
        .expect("execute_action ok");

        assert_eq!(
            exec.outcome, "skipped",
            "a dirty tree must NOT be fast-forwarded even with a PullFastForward decision"
        );
        assert_eq!(
            exec.reason_code.as_deref(),
            Some("git.dirty_tree"),
            "the H-1 re-check records the dirty reason code"
        );
        let head_after = git.inspect(working).unwrap().head_sha;
        assert_eq!(
            head_before, head_after,
            "the H-1 guard must leave HEAD untouched (no fast-forward)"
        );
    }

    // --- E-08 review fix (HIGH): a successful manual run clears auto-pause -----
    // The E-08 scheduler's due-query excludes auto_paused = 1, and the scheduler
    // never runs an excluded repo, so the ONLY way to re-admit a user-recovered
    // repo is a successful manual check/update. Without clearing the flag here,
    // auto-pause is a permanent one-way trip. (E-08 spec: "auto_paused resets to 0
    // on a successful manual check".)

    /// Force a repo into the 3-strikes auto-paused state.
    async fn force_auto_paused(pool: &SqlitePool, id: RepoId) {
        sqlx::query(
            "UPDATE repo_local_state SET consecutive_failures = 3, auto_paused = 1 \
             WHERE repo_id = ?",
        )
        .bind(id.0)
        .execute(pool)
        .await
        .expect("force auto-pause");
    }

    /// Read `(consecutive_failures, auto_paused)` for a repo.
    async fn read_failure_state(pool: &SqlitePool, id: RepoId) -> (i64, i64) {
        let row = sqlx::query(
            "SELECT consecutive_failures, auto_paused FROM repo_local_state WHERE repo_id = ?",
        )
        .bind(id.0)
        .fetch_one(pool)
        .await
        .unwrap();
        (
            row.try_get("consecutive_failures").unwrap(),
            row.try_get("auto_paused").unwrap(),
        )
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn successful_manual_update_clears_auto_pause() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping successful_manual_update_clears_auto_pause: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;
        let fx = build_fixture(FixtureState::Behind);
        let id = add(&pool, &git, fx.working_path()).await.expect("add");
        set_mode(&pool, id, "pull_ff_only").await;
        force_auto_paused(&pool, id).await;

        let result = update_now(&pool, &git, id, UpdateMode::PullFfOnly)
            .await
            .expect("update ok");
        assert_eq!(
            result.outcome, "updated",
            "a clean behind repo fast-forwards"
        );

        let (cf, ap) = read_failure_state(&pool, id).await;
        assert_eq!(
            cf, 0,
            "a successful manual update resets consecutive_failures"
        );
        assert_eq!(ap, 0, "a successful manual update clears auto_paused");
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn successful_manual_check_clears_auto_pause() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping successful_manual_check_clears_auto_pause: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;
        let fx = build_fixture(FixtureState::Clean);
        let id = add(&pool, &git, fx.working_path()).await.expect("add");
        force_auto_paused(&pool, id).await;

        let _ = check_now(&pool, &git, id).await.expect("check ok");

        let (cf, ap) = read_failure_state(&pool, id).await;
        assert_eq!(
            cf, 0,
            "a successful manual check resets consecutive_failures"
        );
        assert_eq!(ap, 0, "a successful manual check clears auto_paused");
    }
}
