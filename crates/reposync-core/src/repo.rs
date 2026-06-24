//! repo - owned by E-02 / E-03 (the core flows where layers meet).
//!
//! Tracer slice: `add` (validate + inspect + persist) and `check_now`
//! (re-inspect + fetch + ahead/behind + record an activity row). Uses the sqlx
//! RUNTIME query API and unix-seconds timestamps (no chrono).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::git::{AheadBehind, InspectResult, SystemGitEngine};
use crate::ipc::{CheckResult, RepoId, UpdateMode, UpdateResult};
use crate::policy::{decide, Action, PolicyDecision, RepoState, SkipReason, UpstreamState};

/// Current unix time in whole seconds.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
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
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
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
    let insert = sqlx::query(
        "INSERT INTO repos \
         (local_name, local_path, remote_origin_url, host_type, created_at) \
         VALUES (?, ?, ?, ?, ?)",
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
    let row = sqlx::query(
        "SELECT r.local_path AS local_path, r.update_mode AS update_mode, \
         s.upstream_branch AS upstream_branch \
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
    let stored_upstream: Option<String> = row.try_get("upstream_branch")?;
    let mode_str: String = row.try_get("update_mode")?;
    let mode = parse_update_mode(&mode_str);
    let path = Path::new(&local_path);

    // 2. Re-inspect local state.
    let inspect = git.inspect(path)?;
    let now = now_secs();

    // 3. Fetch (record raw output regardless of outcome).
    let fetch = git.fetch(path).await?;

    // 4. Resolve upstream from the FRESH inspection. The DB's stored upstream is
    //    intentionally NOT a fallback: a fresh inspect of `None` is authoritative
    //    (the branch's upstream was removed, e.g. deleted-upstream), and falling
    //    back to a stale stored ref would re-introduce a comparison base that no
    //    longer exists.
    let upstream = resolve_upstream(inspect.upstream_branch.clone(), stored_upstream);

    // 5. Ahead/behind when an upstream exists.
    let ahead_behind = match &upstream {
        Some(u) => git.ahead_behind(path, u).await?,
        None => crate::git::AheadBehind {
            ahead: None,
            behind: None,
        },
    };

    // 6. The decision now runs through the E-07 policy engine (the same `decide`
    //    that drives the scheduler and update_now), so check_now and scheduled
    //    checks share one set of safety rules.
    //
    //    The fetch outcome is handled FIRST and separately: a failed fetch is an
    //    operational failure (auth/network), not a policy skip, so it surfaces
    //    its class as the reason and the activity row records "failed". A
    //    successful fetch refreshes the comparison base, then `decide` over the
    //    repo's configured mode produces the reported decision/reason.
    let (decision, reason): (String, Option<String>) = if !fetch.success {
        let why = match fetch.class {
            crate::git::FetchClass::AuthFailure => "git.auth_failed",
            crate::git::FetchClass::NetworkFailure => "net.offline",
            // Success / NoOp never reach this arm (fetch.success would be true).
            _ => "git.fetch_failed",
        };
        ("skip-with-reason".to_string(), Some(why.to_string()))
    } else {
        // Build the policy engine's view from the fresh reads and decide.
        let has_origin = has_origin_remote(path);
        let state = repo_state_from_reads(&inspect, &ahead_behind, has_origin);
        match decide(&state, &mode) {
            PolicyDecision::Act(Action::PullFastForward) => {
                ("would-fast-forward".to_string(), None)
            }
            // Fetch and ReportStatus are both "no mutation pending": from a
            // check's perspective there is nothing to update, so it reports
            // status with no reason. (A `fetch_only` repo never fast-forwards, so
            // its clean/behind cells report status here, which is correct: the
            // check itself already fetched.)
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

    // 8. Append the activity record.
    let status = if fetch.success { "success" } else { "failed" };
    let summary = format!(
        "check: decision={decision}, ahead={:?}, behind={:?}",
        ahead_behind.ahead, ahead_behind.behind
    );
    sqlx::query(
        "INSERT INTO activity_records \
         (repo_id, timestamp, action_type, status, reason_code, summary, \
          raw_command, raw_stdout, raw_stderr, exit_code, duration_ms) \
         VALUES (?, ?, 'check', ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(repo_id)
    .bind(now)
    .bind(status)
    .bind(&reason)
    .bind(&summary)
    .bind(&fetch.raw_command)
    .bind(&fetch.raw_stdout)
    .bind(&fetch.raw_stderr)
    .bind(fetch.exit_code)
    .bind(fetch.duration_ms)
    .execute(pool)
    .await?;

    // 9. A failed fetch records the activity row above, then surfaces the error.
    if !fetch.success {
        return Err(AppError::FetchFailed {
            exit_code: fetch.exit_code,
            stderr: fetch.raw_stderr,
        });
    }

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
pub async fn update_now(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    id: RepoId,
    mode: UpdateMode,
) -> Result<UpdateResult, AppError> {
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

    // 3. Fetch to refresh the comparison base (records raw output regardless).
    let fetch = git.fetch(path).await?;

    // 4. Resolve upstream + ahead/behind from the fresh reads.
    let upstream = resolve_upstream(before.upstream_branch.clone(), None);
    let ahead_behind = match &upstream {
        Some(u) => git.ahead_behind(path, u).await?,
        None => AheadBehind {
            ahead: None,
            behind: None,
        },
    };

    // 5. Decide via the policy engine (the same `decide` everywhere).
    let has_origin = has_origin_remote(path);
    let state = repo_state_from_reads(&before, &ahead_behind, has_origin);
    let decision = decide(&state, &mode);

    // 6. Execute the decided action. Only PullFastForward mutates; the fetch for
    //    the Fetch action already ran in step 3. A fetch failure short-circuits
    //    to a failed outcome regardless of the decided action (it broke the
    //    comparison base the decision rested on).
    let exec = execute_decision(git, path, &fetch, decision, head_before.as_deref()).await?;
    let outcome = exec.outcome;
    let status = exec.status;
    let reason_code = exec.reason_code;
    let commit_range = exec.commit_range;

    // 7. Refresh the cached local state from the post-action reads.
    let post = git.inspect(path).unwrap_or(before.clone());
    let post_ab = match &upstream {
        Some(u) => git.ahead_behind(path, u).await.unwrap_or(AheadBehind {
            ahead: ahead_behind.ahead,
            behind: ahead_behind.behind,
        }),
        None => AheadBehind {
            ahead: None,
            behind: None,
        },
    };
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

    // 8. Record a minimal activity row (the full writer + retention is E-09).
    let summary = format!("update: mode={mode_label}, outcome={outcome}");
    sqlx::query(
        "INSERT INTO activity_records \
         (repo_id, timestamp, action_type, status, reason_code, summary, commit_range, \
          raw_command, raw_stdout, raw_stderr, exit_code, duration_ms) \
         VALUES (?, ?, 'update', ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(repo_id)
    .bind(now)
    .bind(status)
    .bind(&reason_code)
    .bind(&summary)
    .bind(&commit_range)
    .bind(&exec.act_command)
    .bind(&exec.act_stdout)
    .bind(&exec.act_stderr)
    .bind(exec.act_exit)
    .bind(exec.act_duration)
    .execute(pool)
    .await?;

    Ok(UpdateResult {
        repo_id,
        mode: mode_label,
        outcome: outcome.to_string(),
        commit_range,
        ahead: post_ab.ahead,
        behind: post_ab.behind,
        updated_at: now,
    })
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
/// outcome accumulation is a single expression per branch (no dead-initialized
/// mutable state) and the decide -> execute mapping reads as a table.
///
/// `fetch` is the refreshing fetch already run by the caller; its capture is the
/// default activity record (a fast-forward pull replaces it with its own). A
/// failed fetch short-circuits to `failed` because the comparison the decision
/// rested on is gone.
async fn execute_decision(
    git: &SystemGitEngine,
    path: &Path,
    fetch: &crate::git::FetchOutcome,
    decision: PolicyDecision,
    head_before: Option<&str>,
) -> Result<ExecOutcome, AppError> {
    // Default activity capture = the fetch that always ran.
    let from_fetch = |outcome: &'static str,
                      status: &'static str,
                      reason_code: Option<String>,
                      commit_range: Option<String>| ExecOutcome {
        outcome,
        status,
        reason_code,
        commit_range,
        act_command: fetch.raw_command.clone(),
        act_stdout: fetch.raw_stdout.clone(),
        act_stderr: fetch.raw_stderr.clone(),
        act_exit: fetch.exit_code,
        act_duration: fetch.duration_ms,
    };

    // A failed refreshing fetch short-circuits: the update cannot proceed.
    if !fetch.success {
        let reason = match fetch.class {
            crate::git::FetchClass::AuthFailure => "git.auth_failed",
            crate::git::FetchClass::NetworkFailure => "net.offline",
            _ => "git.fetch_failed",
        };
        return Ok(from_fetch(
            "failed",
            "failed",
            Some(reason.to_string()),
            None,
        ));
    }

    match decision {
        PolicyDecision::Act(Action::PullFastForward) => {
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
            // The refreshing fetch already ran and succeeded; report it.
            let outcome = if fetch.class == crate::git::FetchClass::Success {
                "updated"
            } else {
                "up_to_date"
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

        // check_now writes an activity row with raw_command/exit_code/duration_ms
        // and updates last_checked_at. (No remote: fetch may fail, but the row
        // is still written.)
        let _ = check_now(&pool, &git, id).await;

        let act = sqlx::query(
            "SELECT raw_command, exit_code, duration_ms FROM activity_records \
             WHERE repo_id = ? AND action_type = 'check'",
        )
        .bind(id.0)
        .fetch_one(&pool)
        .await
        .expect("activity row present");
        let raw_command: Option<String> = act.try_get("raw_command").unwrap();
        let duration_ms: Option<i64> = act.try_get("duration_ms").unwrap();
        assert!(
            raw_command
                .as_deref()
                .map(|s| s.contains("fetch"))
                .unwrap_or(false),
            "raw_command should record the fetch invocation"
        );
        assert!(duration_ms.is_some(), "duration_ms should be populated");

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
}
