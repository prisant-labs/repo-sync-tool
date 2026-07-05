//! store - owned by E-02 (persistence-backed data access for the IPC commands).
//!
//! The Tauri-free data-access layer behind the E-02 commands: list/get/remove/
//! enable repos, read/write the settings singleton, and scan a parent folder for
//! git repositories. Every function returns a FROZEN `ipc` payload type (E-06
//! contract) so the thin `src-tauri` handlers can hand the result straight back
//! across the IPC boundary.
//!
//! Uses the sqlx RUNTIME query API throughout (no compile-time macros, no
//! DATABASE_URL), consistent with `db.rs` and `repo.rs`.

use std::path::{Path, PathBuf};

use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::git::SystemGitEngine;
use crate::ipc::{
    GroupSummary, RepoDetail, RepoFilter, RepoId, RepoSummary, ScanCandidate, ScanResult, Settings,
    UpdateMode, UpdatePolicy,
};

/// The maximum directory depth a parent-folder scan descends (defense against a
/// pathological tree). The strategy doc bounds the walk; 6 covers the common
/// "one folder of clones, optionally one level of grouping" layout.
const SCAN_MAX_DEPTH: usize = 6;

// =============================================================================
// Repo registry reads
// =============================================================================

/// List tracked repos (summary view), applying `filter`.
///
/// Joins `repos` + `repo_local_state` + the cached latest release tag from
/// `repo_remote_meta`. Filters: `enabled_only` (only enabled repos), `host_type`
/// (exact match), and `query` (case-insensitive substring of name or path).
pub async fn repo_list(
    pool: &SqlitePool,
    filter: &RepoFilter,
) -> Result<Vec<RepoSummary>, AppError> {
    // A single query fetches every row; filtering happens in Rust so the
    // (small, single-user) result set stays simple and the SQL has no dynamic
    // WHERE assembly. Repo counts are in the tens-to-low-hundreds (Section 1.2).
    let rows = sqlx::query(
        "SELECT \
            r.id AS id, r.local_name AS local_name, r.local_path AS local_path, \
            r.host_type AS host_type, r.enabled AS enabled, \
            s.ahead_count AS ahead_count, s.behind_count AS behind_count, \
            s.is_dirty AS is_dirty, s.is_detached AS is_detached, \
            s.auto_paused AS auto_paused, s.last_checked_at AS last_checked_at, \
            s.last_error_code AS last_error_code, \
            m.latest_release_tag AS latest_release_tag \
         FROM repos r \
         LEFT JOIN repo_local_state s ON s.repo_id = r.id \
         LEFT JOIN repo_remote_meta m ON m.repo_id = r.id \
         ORDER BY r.local_name COLLATE NOCASE ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let local_name: String = r.try_get("local_name")?;
        let local_path: String = r.try_get("local_path")?;
        let host_type: String = r.try_get("host_type")?;
        let enabled: i64 = r.try_get("enabled")?;

        // Apply filters.
        if filter.enabled_only == Some(true) && enabled == 0 {
            continue;
        }
        if let Some(want_host) = &filter.host_type {
            if &host_type != want_host {
                continue;
            }
        }
        if let Some(q) = &filter.query {
            let needle = q.to_lowercase();
            if !needle.is_empty()
                && !local_name.to_lowercase().contains(&needle)
                && !local_path.to_lowercase().contains(&needle)
            {
                continue;
            }
        }

        out.push(RepoSummary {
            id: r.try_get("id")?,
            local_name,
            host_type,
            ahead_count: r.try_get("ahead_count")?,
            behind_count: r.try_get("behind_count")?,
            is_dirty: int_to_bool(r.try_get("is_dirty").unwrap_or(0)),
            is_detached: int_to_bool(r.try_get("is_detached").unwrap_or(0)),
            enabled: int_to_bool(enabled),
            auto_paused: int_to_bool(r.try_get("auto_paused").unwrap_or(0)),
            last_checked_at: r.try_get("last_checked_at")?,
            last_error_code: r.try_get("last_error_code")?,
            latest_release_tag: r.try_get("latest_release_tag")?,
        });
    }
    Ok(out)
}

/// Get the full detail of a single tracked repo, or [`AppError::NotFound`] if no
/// such repo exists. Joins `repos` + `repo_local_state` + `repo_remote_meta`.
pub async fn repo_get(pool: &SqlitePool, id: RepoId) -> Result<RepoDetail, AppError> {
    let row = sqlx::query(
        "SELECT \
            r.id AS id, r.local_name AS local_name, r.local_path AS local_path, \
            r.host_type AS host_type, r.remote_origin_url AS remote_origin_url, \
            r.default_branch AS default_branch, r.update_mode AS update_mode, \
            r.check_frequency_min AS check_frequency_min, r.enabled AS enabled, \
            r.created_at AS created_at, r.notes AS notes, \
            s.active_branch AS active_branch, s.head_sha AS head_sha, \
            s.upstream_branch AS upstream_branch, s.ahead_count AS ahead_count, \
            s.behind_count AS behind_count, s.is_dirty AS is_dirty, \
            s.is_detached AS is_detached, s.last_local_commit_at AS last_local_commit_at, \
            s.last_checked_at AS last_checked_at, s.last_updated_at AS last_updated_at, \
            s.last_attempted_at AS last_attempted_at, s.last_error_code AS last_error_code, \
            s.next_check_at AS next_check_at, s.auto_paused AS auto_paused, \
            s.consecutive_failures AS consecutive_failures, \
            m.description AS description, m.topics_json AS topics_json, \
            m.latest_release_tag AS latest_release_tag, m.latest_release_at AS latest_release_at, \
            m.latest_release_url AS latest_release_url, m.is_archived AS is_archived, \
            m.last_remote_sha AS last_remote_sha, m.last_fetched_at AS last_fetched_at \
         FROM repos r \
         LEFT JOIN repo_local_state s ON s.repo_id = r.id \
         LEFT JOIN repo_remote_meta m ON m.repo_id = r.id \
         WHERE r.id = ?",
    )
    .bind(id.0)
    .fetch_optional(pool)
    .await?;

    let r = row.ok_or_else(|| AppError::NotFound {
        entity: format!("repo {}", id.0),
    })?;

    Ok(RepoDetail {
        // RepoSummary fields.
        id: r.try_get("id")?,
        local_name: r.try_get("local_name")?,
        host_type: r.try_get("host_type")?,
        ahead_count: r.try_get("ahead_count")?,
        behind_count: r.try_get("behind_count")?,
        is_dirty: int_to_bool(r.try_get("is_dirty").unwrap_or(0)),
        is_detached: int_to_bool(r.try_get("is_detached").unwrap_or(0)),
        enabled: int_to_bool(r.try_get("enabled")?),
        auto_paused: int_to_bool(r.try_get("auto_paused").unwrap_or(0)),
        last_checked_at: r.try_get("last_checked_at")?,
        last_error_code: r.try_get("last_error_code")?,
        latest_release_tag: r.try_get("latest_release_tag")?,
        // repos.
        local_path: r.try_get("local_path")?,
        remote_origin_url: r.try_get("remote_origin_url")?,
        default_branch: r.try_get("default_branch")?,
        update_mode: r.try_get("update_mode")?,
        check_frequency_min: r.try_get("check_frequency_min")?,
        created_at: r.try_get("created_at")?,
        notes: r.try_get("notes")?,
        // repo_local_state.
        active_branch: r.try_get("active_branch")?,
        head_sha: r.try_get("head_sha")?,
        upstream_branch: r.try_get("upstream_branch")?,
        last_local_commit_at: r.try_get("last_local_commit_at")?,
        last_updated_at: r.try_get("last_updated_at")?,
        last_attempted_at: r.try_get("last_attempted_at")?,
        next_check_at: r.try_get("next_check_at")?,
        consecutive_failures: r.try_get("consecutive_failures").unwrap_or(0),
        // repo_remote_meta.
        description: r.try_get("description")?,
        topics_json: r.try_get("topics_json")?,
        latest_release_at: r.try_get("latest_release_at")?,
        latest_release_url: r.try_get("latest_release_url")?,
        is_archived: int_to_bool(r.try_get("is_archived").unwrap_or(0)),
        last_remote_sha: r.try_get("last_remote_sha")?,
        last_fetched_at: r.try_get("last_fetched_at")?,
    })
}

// =============================================================================
// Repo registry writes
// =============================================================================

/// Remove a tracked repo (does NOT touch the working tree). The `ON DELETE
/// CASCADE` foreign keys clear `repo_local_state`, `repo_remote_meta`,
/// `activity_records`, and `repo_groups`. [`AppError::NotFound`] if absent.
pub async fn repo_remove(pool: &SqlitePool, id: RepoId) -> Result<(), AppError> {
    let res = sqlx::query("DELETE FROM repos WHERE id = ?")
        .bind(id.0)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: format!("repo {}", id.0),
        });
    }
    Ok(())
}

/// Enable or disable scheduled checks for a repo. [`AppError::NotFound`] if the
/// repo does not exist.
pub async fn repo_set_enabled(
    pool: &SqlitePool,
    id: RepoId,
    enabled: bool,
) -> Result<(), AppError> {
    let res = sqlx::query("UPDATE repos SET enabled = ? WHERE id = ?")
        .bind(bool_to_int(enabled))
        .bind(id.0)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: format!("repo {}", id.0),
        });
    }
    Ok(())
}

/// Persist the per-repo update policy (E-07). [`AppError::NotFound`] if the repo
/// does not exist.
///
/// Of the [`UpdatePolicy`] fields, only `mode` has a v1 schema column
/// (`repos.update_mode`); `dirty_handling` and `branch_policy` are part of the
/// frozen IPC contract but have no v1 columns, so they are validated-and-accepted
/// here without storage (a per-repo dirty-handling override is a named V1.1
/// extension point in the E-07 spec). The mode string is the snake_case wire
/// value, matching the `update_mode` column's stored form.
///
/// A non-V1 mode (`pull_standard` / `pull_rebase`) is REJECTED with
/// [`AppError::InvalidPolicy`]: the engine would skip it as "mode not available
/// in V1", so persisting it would store a mode that can never execute. Rejecting
/// at the write boundary keeps the stored policy executable.
pub async fn repo_set_policy(
    pool: &SqlitePool,
    id: RepoId,
    policy: &UpdatePolicy,
) -> Result<(), AppError> {
    // Reject a non-V1 mode at the boundary (the closed-enum invariant, AC6).
    if crate::policy::V1Mode::from_update_mode(&policy.mode).is_none() {
        return Err(AppError::InvalidPolicy {
            detail: format!(
                "update mode {} is not available in V1",
                update_mode_str(&policy.mode)
            ),
        });
    }

    let res = sqlx::query("UPDATE repos SET update_mode = ? WHERE id = ?")
        .bind(update_mode_str(&policy.mode))
        .bind(id.0)
        .execute(pool)
        .await?;
    if res.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: format!("repo {}", id.0),
        });
    }
    Ok(())
}

/// The snake_case `update_mode` column value for an [`UpdateMode`] (matching the
/// IPC enum's serde rename and the schema default `'fetch_only'`).
fn update_mode_str(mode: &UpdateMode) -> &'static str {
    match mode {
        UpdateMode::CheckOnly => "check_only",
        UpdateMode::FetchOnly => "fetch_only",
        UpdateMode::PullFfOnly => "pull_ff_only",
        UpdateMode::PullStandard => "pull_standard",
        UpdateMode::PullRebase => "pull_rebase",
    }
}

// =============================================================================
// Settings singleton
// =============================================================================

/// Read the settings singleton, seeding the row with schema defaults on first
/// call. Always returns a [`Settings`]; the row is created if absent so the
/// first `settings_get` returns the defaults instead of an error.
pub async fn settings_get(pool: &SqlitePool) -> Result<Settings, AppError> {
    // Seed the singleton on first read. INSERT OR IGNORE is a no-op once the row
    // exists, so this is idempotent and cheap. All defaults come from the schema.
    sqlx::query("INSERT OR IGNORE INTO settings (id) VALUES (1)")
        .execute(pool)
        .await?;

    let r = sqlx::query(
        "SELECT global_check_minutes, quiet_hours_start, quiet_hours_end, \
            notify_on_release, notify_on_failure, git_executable_path, \
            editor_command, terminal_command, autostart, activity_retention_d, \
            github_token_present \
         FROM settings WHERE id = 1",
    )
    .fetch_one(pool)
    .await?;

    Ok(Settings {
        global_check_minutes: r.try_get("global_check_minutes")?,
        quiet_hours_start: r.try_get("quiet_hours_start")?,
        quiet_hours_end: r.try_get("quiet_hours_end")?,
        notify_on_release: int_to_bool(r.try_get("notify_on_release")?),
        notify_on_failure: int_to_bool(r.try_get("notify_on_failure")?),
        git_executable_path: r.try_get("git_executable_path")?,
        editor_command: r.try_get("editor_command")?,
        terminal_command: r.try_get("terminal_command")?,
        autostart: int_to_bool(r.try_get("autostart")?),
        activity_retention_d: r.try_get("activity_retention_d")?,
        github_token_present: int_to_bool(r.try_get("github_token_present")?),
    })
}

/// Write the settings singleton (validating the inputs first). Upserts the
/// id = 1 row. `github_token_present` is NOT written here: it is a derived flag
/// owned by the keychain integration (E-10), never set from the wire payload.
pub async fn settings_set(pool: &SqlitePool, settings: &Settings) -> Result<(), AppError> {
    validate_settings(settings)?;

    sqlx::query(
        "INSERT INTO settings ( \
            id, global_check_minutes, quiet_hours_start, quiet_hours_end, \
            notify_on_release, notify_on_failure, git_executable_path, \
            editor_command, terminal_command, autostart, activity_retention_d) \
         VALUES (1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) \
         ON CONFLICT(id) DO UPDATE SET \
            global_check_minutes = excluded.global_check_minutes, \
            quiet_hours_start = excluded.quiet_hours_start, \
            quiet_hours_end = excluded.quiet_hours_end, \
            notify_on_release = excluded.notify_on_release, \
            notify_on_failure = excluded.notify_on_failure, \
            git_executable_path = excluded.git_executable_path, \
            editor_command = excluded.editor_command, \
            terminal_command = excluded.terminal_command, \
            autostart = excluded.autostart, \
            activity_retention_d = excluded.activity_retention_d",
    )
    .bind(settings.global_check_minutes)
    .bind(settings.quiet_hours_start)
    .bind(settings.quiet_hours_end)
    .bind(bool_to_int(settings.notify_on_release))
    .bind(bool_to_int(settings.notify_on_failure))
    .bind(&settings.git_executable_path)
    .bind(&settings.editor_command)
    .bind(&settings.terminal_command)
    .bind(bool_to_int(settings.autostart))
    .bind(settings.activity_retention_d)
    .execute(pool)
    .await?;

    Ok(())
}

/// Validate a [`Settings`] payload before persisting it.
fn validate_settings(s: &Settings) -> Result<(), AppError> {
    if s.global_check_minutes < 1 {
        return Err(AppError::InvalidSetting {
            field: "global_check_minutes".into(),
        });
    }
    if s.activity_retention_d < 1 {
        return Err(AppError::InvalidSetting {
            field: "activity_retention_d".into(),
        });
    }
    // Quiet hours are minutes-since-midnight when present: 0..=1439. Either both
    // bounds are set or neither (a half-open window is malformed).
    match (s.quiet_hours_start, s.quiet_hours_end) {
        (Some(start), Some(end)) => {
            if !(0..=1439).contains(&start) || !(0..=1439).contains(&end) {
                return Err(AppError::QuietHoursMalformed);
            }
        }
        (None, None) => {}
        _ => return Err(AppError::QuietHoursMalformed),
    }
    Ok(())
}

// =============================================================================
// Parent-folder scan
// =============================================================================

/// Scan `parent` for candidate git repositories (AC: `repo_scan_parent`).
///
/// Walks `parent` up to [`SCAN_MAX_DEPTH`] deep, identifying directories that are
/// git repositories via the E-03 [`SystemGitEngine`] (a successful `inspect`).
/// A discovered `.git` directory is NOT descended into. Each candidate is tagged
/// `already_tracked` if its canonical path is already in `repos`.
pub async fn repo_scan_parent(
    pool: &SqlitePool,
    git: &SystemGitEngine,
    parent: &Path,
) -> Result<ScanResult, AppError> {
    if !parent.exists() {
        return Err(AppError::PathMissing {
            path: parent.display().to_string(),
        });
    }
    if !parent.is_dir() {
        return Err(AppError::NotADirectory {
            path: parent.display().to_string(),
        });
    }

    // The set of already-tracked canonical paths, for the already_tracked flag.
    let tracked = tracked_paths(pool).await?;

    let mut found: Vec<PathBuf> = Vec::new();
    walk_for_repos(git, parent, 0, &mut found);

    let mut discovered = Vec::with_capacity(found.len());
    for path in found {
        // `dunce::canonicalize` resolves like `std::fs::canonicalize` but yields a
        // clean, non-verbatim path (no `\\?\` prefix) so the scanned `local_path`
        // opens directly at the edge.
        let canonical = dunce::canonicalize(&path).unwrap_or_else(|_| path.clone());
        let local_path = canonical.display().to_string();
        let local_name = canonical
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| local_path.clone());
        let already_tracked = tracked.iter().any(|t| paths_equal(t, &local_path));
        let remote_origin_url = origin_url(&canonical);

        discovered.push(ScanCandidate {
            local_path,
            local_name,
            already_tracked,
            remote_origin_url,
        });
    }
    // Stable, name-sorted order for a predictable preview list.
    discovered.sort_by_key(|c| c.local_name.to_lowercase());

    Ok(ScanResult {
        parent_path: parent.display().to_string(),
        discovered,
    })
}

/// Recursively collect git-repository directories under `dir`, bounded by
/// [`SCAN_MAX_DEPTH`]. A directory that inspects as a repo is recorded and NOT
/// descended into (no nested-submodule recursion); other directories are walked.
fn walk_for_repos(git: &SystemGitEngine, dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth > SCAN_MAX_DEPTH {
        return;
    }
    // Fast path: a directory containing a `.git` entry is a working tree. We
    // still confirm via inspect() so a stray `.git` file/dir that is not a real
    // repo does not produce a false positive.
    if git.inspect(dir).is_ok() {
        out.push(dir.to_path_buf());
        return;
    }

    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // Only descend into directories. Skip symlinks to avoid cycles and skip
        // the `.git` internals of any repo we are about to recognize.
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        if path.file_name().and_then(|s| s.to_str()) == Some(".git") {
            continue;
        }
        walk_for_repos(git, &path, depth + 1, out);
    }
}

/// The canonical `local_path` strings of every tracked repo.
async fn tracked_paths(pool: &SqlitePool) -> Result<Vec<String>, AppError> {
    let rows = sqlx::query("SELECT local_path FROM repos")
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(r.try_get::<String, _>("local_path")?);
    }
    Ok(out)
}

/// Best-effort origin remote URL via git2. `None` if absent or unreadable.
fn origin_url(path: &Path) -> Option<String> {
    let repo = git2::Repository::open(path).ok()?;
    let remote = repo.find_remote("origin").ok()?;
    remote.url().ok().map(|s| s.to_string())
}

/// Case-insensitive, separator-normalized path-string equality (Windows paths).
fn paths_equal(a: &str, b: &str) -> bool {
    normalize(a) == normalize(b)
}

fn normalize(p: &str) -> String {
    p.replace('\\', "/").trim_end_matches('/').to_lowercase()
}

// =============================================================================
// Groups / tags (N:M repos <-> groups)
// =============================================================================

/// List every group with its member repo count (the group-management view).
///
/// A LEFT JOIN + GROUP BY yields one row per group, `repo_count` being the number
/// of `repo_groups` memberships (0 for an empty group), name-ordered.
pub async fn groups_list(pool: &SqlitePool) -> Result<Vec<GroupSummary>, AppError> {
    let rows = sqlx::query(
        "SELECT g.id AS id, g.name AS name, g.color AS color, \
            COUNT(rg.repo_id) AS repo_count \
         FROM groups g \
         LEFT JOIN repo_groups rg ON rg.group_id = g.id \
         GROUP BY g.id, g.name, g.color \
         ORDER BY g.name",
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(GroupSummary {
            id: r.try_get("id")?,
            name: r.try_get("name")?,
            color: r.try_get("color")?,
            repo_count: r.try_get("repo_count")?,
        });
    }
    Ok(out)
}

/// Create a group, returning it as a [`GroupSummary`] (a fresh group has
/// `repo_count` 0). A duplicate name (the `UNIQUE(name)` constraint) maps to
/// [`AppError::InvalidSetting`] with `field: "name"` so the caller gets a clear
/// "that name is taken" rather than a raw database error.
pub async fn group_create(
    pool: &SqlitePool,
    name: &str,
    color: Option<&str>,
) -> Result<GroupSummary, AppError> {
    let res = sqlx::query("INSERT INTO groups (name, color) VALUES (?, ?)")
        .bind(name)
        .bind(color)
        .execute(pool)
        .await;
    let inserted = match res {
        Ok(inserted) => inserted,
        Err(e) => {
            if is_unique_violation(&e) {
                return Err(AppError::InvalidSetting {
                    field: "name".into(),
                });
            }
            return Err(AppError::from(e));
        }
    };
    Ok(GroupSummary {
        id: inserted.last_insert_rowid(),
        name: name.to_string(),
        color: color.map(|s| s.to_string()),
        repo_count: 0,
    })
}

/// Rename a group. A duplicate name maps to [`AppError::InvalidSetting`]
/// (`field: "name"`); a missing id (0 rows affected) is [`AppError::NotFound`].
pub async fn group_rename(pool: &SqlitePool, id: i64, name: &str) -> Result<(), AppError> {
    let res = sqlx::query("UPDATE groups SET name = ? WHERE id = ?")
        .bind(name)
        .bind(id)
        .execute(pool)
        .await;
    let updated = match res {
        Ok(updated) => updated,
        Err(e) => {
            if is_unique_violation(&e) {
                return Err(AppError::InvalidSetting {
                    field: "name".into(),
                });
            }
            return Err(AppError::from(e));
        }
    };
    if updated.rows_affected() == 0 {
        return Err(AppError::NotFound {
            entity: format!("group {id}"),
        });
    }
    Ok(())
}

/// Delete a group. Idempotent (a missing id is not an error). The `ON DELETE
/// CASCADE` on `repo_groups.group_id` clears every membership of the group.
pub async fn group_delete(pool: &SqlitePool, id: i64) -> Result<(), AppError> {
    sqlx::query("DELETE FROM groups WHERE id = ?")
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Assign a repo to a group. Idempotent: `INSERT OR IGNORE` swallows the
/// duplicate-membership primary-key collision so re-assigning is a no-op.
///
/// Foreign keys are enforced on the pool (`db::open_pool` sets `foreign_keys(true)`),
/// and `OR IGNORE` does NOT suppress a FOREIGN KEY violation, so a missing repo or
/// group surfaces as an error here; it is mapped to [`AppError::NotFound`].
pub async fn group_assign(pool: &SqlitePool, repo_id: i64, group_id: i64) -> Result<(), AppError> {
    let res = sqlx::query("INSERT OR IGNORE INTO repo_groups (repo_id, group_id) VALUES (?, ?)")
        .bind(repo_id)
        .bind(group_id)
        .execute(pool)
        .await;
    if let Err(e) = res {
        if is_foreign_key_violation(&e) {
            return Err(AppError::NotFound {
                entity: format!("repo {repo_id} or group {group_id}"),
            });
        }
        return Err(AppError::from(e));
    }
    Ok(())
}

/// Remove a repo from a group. Idempotent (deleting a nonexistent membership is
/// not an error).
pub async fn group_unassign(
    pool: &SqlitePool,
    repo_id: i64,
    group_id: i64,
) -> Result<(), AppError> {
    sqlx::query("DELETE FROM repo_groups WHERE repo_id = ? AND group_id = ?")
        .bind(repo_id)
        .bind(group_id)
        .execute(pool)
        .await?;
    Ok(())
}

/// The ids of every group a repo belongs to (ascending).
pub async fn groups_for_repo(pool: &SqlitePool, repo_id: i64) -> Result<Vec<i64>, AppError> {
    let rows = sqlx::query("SELECT group_id FROM repo_groups WHERE repo_id = ? ORDER BY group_id")
        .bind(repo_id)
        .fetch_all(pool)
        .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(r.try_get::<i64, _>("group_id")?);
    }
    Ok(out)
}

/// Whether a sqlx error is a SQLite UNIQUE constraint violation. Mirrors the
/// `repo.rs` helper: check the extended/primary result codes first, then fall
/// back to the message for portability across sqlx versions.
fn is_unique_violation(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        if let Some(code) = db_err.code() {
            if code == "2067" || code == "1555" || code == "19" {
                return true;
            }
        }
        return db_err
            .message()
            .to_ascii_lowercase()
            .contains("unique constraint failed");
    }
    false
}

/// Whether a sqlx error is a SQLite FOREIGN KEY constraint violation. SQLite
/// reports these with extended code "787" (`SQLITE_CONSTRAINT_FOREIGNKEY`) / the
/// primary "19"; the message check is the portable fallback.
fn is_foreign_key_violation(err: &sqlx::Error) -> bool {
    if let sqlx::Error::Database(db_err) = err {
        if let Some(code) = db_err.code() {
            if code == "787" || code == "19" {
                return true;
            }
        }
        return db_err
            .message()
            .to_ascii_lowercase()
            .contains("foreign key constraint failed");
    }
    false
}

/// SQLite stores booleans as 0/1 INTEGERs; map to Rust `bool`.
fn int_to_bool(v: i64) -> bool {
    v != 0
}

fn bool_to_int(v: bool) -> i64 {
    if v {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::path::Path;
    use tempfile::TempDir;

    /// A migrated, on-disk SQLite pool in a fresh tempdir.
    async fn fresh_pool(dir: &Path) -> SqlitePool {
        let db_file = dir.join("store-test.db");
        let pool = db::open_pool(&db_file).await.expect("open_pool");
        db::run_migrations(&pool).await.expect("migrations");
        pool
    }

    /// Init a git repo with one commit at `dir` (test helper).
    fn init_repo_with_commit(dir: &Path) {
        let repo = git2::Repository::init(dir).expect("init repo");
        std::fs::write(dir.join("README.md"), "hello\n").expect("write file");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("README.md")).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("Store Test", "store@example.com").expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .expect("commit");
    }

    #[tokio::test]
    async fn add_then_list_and_get() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping add_then_list_and_get: git not resolvable");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let repotmp = TempDir::new().unwrap();
        init_repo_with_commit(repotmp.path());
        let id = crate::repo::add(&pool, &git, repotmp.path())
            .await
            .expect("add ok");

        // list with an empty filter returns the one repo.
        let empty = RepoFilter {
            enabled_only: None,
            host_type: None,
            query: None,
        };
        let all = repo_list(&pool, &empty).await.expect("list");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, id.0);
        assert!(all[0].enabled, "newly added repo defaults to enabled");

        // get returns the full detail with schema defaults.
        let detail = repo_get(&pool, id).await.expect("get");
        assert_eq!(detail.id, id.0);
        assert_eq!(detail.update_mode, "fetch_only");
        assert_eq!(
            detail.check_frequency_min, 0,
            "a newly-added repo inherits the global cadence (check_frequency_min = 0), \
             not the old 360 per-repo default"
        );
        assert_eq!(detail.consecutive_failures, 0);

        // get of an unknown id is NotFound.
        let missing = repo_get(&pool, RepoId(9999)).await;
        assert!(matches!(missing, Err(AppError::NotFound { .. })));
    }

    #[tokio::test]
    async fn list_filters_apply() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping list_filters_apply: git not resolvable");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let repotmp = TempDir::new().unwrap();
        init_repo_with_commit(repotmp.path());
        let id = crate::repo::add(&pool, &git, repotmp.path())
            .await
            .expect("add ok");

        // Disable it, then enabled_only must exclude it.
        repo_set_enabled(&pool, id, false).await.expect("disable");
        let enabled_only = RepoFilter {
            enabled_only: Some(true),
            host_type: None,
            query: None,
        };
        assert!(
            repo_list(&pool, &enabled_only).await.unwrap().is_empty(),
            "a disabled repo must be excluded by enabled_only"
        );

        // host_type filter that does not match excludes it.
        let wrong_host = RepoFilter {
            enabled_only: None,
            host_type: Some("gitlab".into()),
            query: None,
        };
        assert!(repo_list(&pool, &wrong_host).await.unwrap().is_empty());

        // A query substring of the name matches.
        let name = repotmp
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let q = RepoFilter {
            enabled_only: None,
            host_type: None,
            query: Some(name),
        };
        assert_eq!(repo_list(&pool, &q).await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn set_enabled_round_trips_and_remove_cascades() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping set_enabled_round_trips_and_remove_cascades: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let repotmp = TempDir::new().unwrap();
        init_repo_with_commit(repotmp.path());
        let id = crate::repo::add(&pool, &git, repotmp.path())
            .await
            .expect("add ok");

        // Toggle enabled off then on.
        repo_set_enabled(&pool, id, false).await.unwrap();
        assert!(!repo_get(&pool, id).await.unwrap().enabled);
        repo_set_enabled(&pool, id, true).await.unwrap();
        assert!(repo_get(&pool, id).await.unwrap().enabled);

        // set_enabled on a missing repo is NotFound.
        assert!(matches!(
            repo_set_enabled(&pool, RepoId(9999), true).await,
            Err(AppError::NotFound { .. })
        ));

        // remove deletes the repo and cascades to repo_local_state.
        repo_remove(&pool, id).await.expect("remove");
        let state_count: i64 =
            sqlx::query("SELECT COUNT(*) AS c FROM repo_local_state WHERE repo_id = ?")
                .bind(id.0)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("c")
                .unwrap();
        assert_eq!(state_count, 0, "ON DELETE CASCADE must clear local_state");

        // remove again is NotFound.
        assert!(matches!(
            repo_remove(&pool, id).await,
            Err(AppError::NotFound { .. })
        ));
    }

    #[tokio::test]
    async fn set_policy_persists_mode_and_rejects_non_v1() {
        use crate::ipc::{BranchPolicy, DirtyHandling, UpdateMode, UpdatePolicy};

        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping set_policy_persists_mode_and_rejects_non_v1: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let repotmp = TempDir::new().unwrap();
        init_repo_with_commit(repotmp.path());
        let id = crate::repo::add(&pool, &git, repotmp.path())
            .await
            .expect("add ok");

        // The schema default is fetch_only.
        assert_eq!(repo_get(&pool, id).await.unwrap().update_mode, "fetch_only");

        // Persisting a V1 mode updates the column.
        let policy = UpdatePolicy {
            mode: UpdateMode::PullFfOnly,
            dirty_handling: DirtyHandling::Skip,
            branch_policy: BranchPolicy::DefaultBranchOnly,
        };
        repo_set_policy(&pool, id, &policy)
            .await
            .expect("set policy");
        assert_eq!(
            repo_get(&pool, id).await.unwrap().update_mode,
            "pull_ff_only"
        );

        // A non-V1 mode is rejected at the boundary and does NOT change the
        // stored mode.
        let non_v1 = UpdatePolicy {
            mode: UpdateMode::PullStandard,
            dirty_handling: DirtyHandling::Skip,
            branch_policy: BranchPolicy::DefaultBranchOnly,
        };
        let rejected = repo_set_policy(&pool, id, &non_v1).await;
        assert!(
            matches!(rejected, Err(AppError::InvalidPolicy { .. })),
            "a non-V1 mode must be rejected, got {rejected:?}"
        );
        assert_eq!(
            repo_get(&pool, id).await.unwrap().update_mode,
            "pull_ff_only",
            "a rejected policy must leave the stored mode unchanged"
        );

        // set_policy on a missing repo is NotFound.
        let missing = repo_set_policy(&pool, RepoId(9999), &policy).await;
        assert!(
            matches!(missing, Err(AppError::NotFound { .. })),
            "set_policy on a missing repo must be NotFound, got {missing:?}"
        );
    }

    #[tokio::test]
    async fn settings_seed_and_round_trip() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        // First get seeds the singleton with schema defaults.
        let defaults = settings_get(&pool).await.expect("seed + get");
        assert_eq!(defaults.global_check_minutes, 360);
        assert_eq!(defaults.activity_retention_d, 90);
        assert!(defaults.notify_on_release);
        assert!(!defaults.autostart);
        assert!(!defaults.github_token_present);

        // Write a modified copy and read it back unchanged.
        let updated = Settings {
            global_check_minutes: 120,
            quiet_hours_start: Some(1320),
            quiet_hours_end: Some(420),
            notify_on_release: false,
            notify_on_failure: true,
            git_executable_path: Some("C:/git/git.exe".into()),
            editor_command: Some("code".into()),
            terminal_command: Some("wt".into()),
            autostart: true,
            activity_retention_d: 30,
            github_token_present: false,
        };
        settings_set(&pool, &updated).await.expect("set");
        let back = settings_get(&pool).await.expect("get");
        assert_eq!(back.global_check_minutes, 120);
        assert_eq!(back.quiet_hours_start, Some(1320));
        assert_eq!(back.activity_retention_d, 30);
        assert!(back.autostart);
        assert_eq!(back.editor_command.as_deref(), Some("code"));

        // Still exactly one settings row (singleton upsert, not insert).
        let count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM settings")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("c")
            .unwrap();
        assert_eq!(count, 1, "settings must remain a singleton across set");
    }

    #[tokio::test]
    async fn settings_set_validates() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;
        let base = settings_get(&pool).await.unwrap();

        // Zero check interval is invalid.
        let bad = Settings {
            global_check_minutes: 0,
            ..base.clone()
        };
        assert!(matches!(
            settings_set(&pool, &bad).await,
            Err(AppError::InvalidSetting { .. })
        ));

        // A half-open quiet-hours window is malformed.
        let half = Settings {
            quiet_hours_start: Some(60),
            quiet_hours_end: None,
            ..base.clone()
        };
        assert!(matches!(
            settings_set(&pool, &half).await,
            Err(AppError::QuietHoursMalformed)
        ));

        // Out-of-range quiet hours are malformed.
        let oob = Settings {
            quiet_hours_start: Some(60),
            quiet_hours_end: Some(5000),
            ..base
        };
        assert!(matches!(
            settings_set(&pool, &oob).await,
            Err(AppError::QuietHoursMalformed)
        ));
    }

    #[tokio::test]
    async fn scan_parent_finds_repos_and_marks_tracked() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping scan_parent_finds_repos_and_marks_tracked: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        // A parent dir holding two git repos and one plain folder.
        let parent = TempDir::new().unwrap();
        let repo_a = parent.path().join("alpha");
        let repo_b = parent.path().join("beta");
        let plain = parent.path().join("not-a-repo");
        std::fs::create_dir_all(&repo_a).unwrap();
        std::fs::create_dir_all(&repo_b).unwrap();
        std::fs::create_dir_all(&plain).unwrap();
        std::fs::write(plain.join("file.txt"), "x").unwrap();
        init_repo_with_commit(&repo_a);
        init_repo_with_commit(&repo_b);

        // Track repo_a so the scan marks it already_tracked.
        crate::repo::add(&pool, &git, &repo_a).await.expect("add a");

        let result = repo_scan_parent(&pool, &git, parent.path())
            .await
            .expect("scan");

        assert_eq!(result.discovered.len(), 2, "two git repos discovered");
        let names: Vec<&str> = result
            .discovered
            .iter()
            .map(|c| c.local_name.as_str())
            .collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        // The plain folder is NOT reported.
        assert!(!names.contains(&"not-a-repo"));

        let alpha = result
            .discovered
            .iter()
            .find(|c| c.local_name == "alpha")
            .unwrap();
        let beta = result
            .discovered
            .iter()
            .find(|c| c.local_name == "beta")
            .unwrap();
        assert!(alpha.already_tracked, "alpha was added, so it is tracked");
        assert!(!beta.already_tracked, "beta was never added");
    }

    #[tokio::test]
    async fn scan_parent_rejects_missing_and_file() {
        let Ok(git) = SystemGitEngine::discover() else {
            eprintln!("skipping scan_parent_rejects_missing_and_file: git missing");
            return;
        };
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let missing = Path::new("Z:/definitely/not/here");
        assert!(matches!(
            repo_scan_parent(&pool, &git, missing).await,
            Err(AppError::PathMissing { .. })
        ));

        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("a-file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(matches!(
            repo_scan_parent(&pool, &git, &file).await,
            Err(AppError::NotADirectory { .. })
        ));
    }

    // =========================================================================
    // Groups / tags
    // =========================================================================

    /// Insert a bare `repos` row directly (only the NOT NULL columns without a
    /// default), returning its id. Git-independent, so the group tests always run
    /// rather than skipping when git is absent.
    async fn insert_repo(pool: &SqlitePool, name: &str, path: &str) -> i64 {
        sqlx::query("INSERT INTO repos (local_name, local_path, created_at) VALUES (?, ?, ?)")
            .bind(name)
            .bind(path)
            .bind(0_i64)
            .execute(pool)
            .await
            .expect("insert repo")
            .last_insert_rowid()
    }

    #[tokio::test]
    async fn group_create_lists_with_repo_count() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        // Create two groups; a fresh group reports repo_count 0.
        let backend = group_create(&pool, "backend", Some("#3b82f6"))
            .await
            .expect("create backend");
        assert_eq!(backend.repo_count, 0);
        assert_eq!(backend.color.as_deref(), Some("#3b82f6"));
        group_create(&pool, "frontend", None)
            .await
            .expect("create frontend");

        // Assign two repos to backend so its count reflects the memberships.
        let r1 = insert_repo(&pool, "alpha", "C:/repos/alpha").await;
        let r2 = insert_repo(&pool, "beta", "C:/repos/beta").await;
        group_assign(&pool, r1, backend.id)
            .await
            .expect("assign r1");
        group_assign(&pool, r2, backend.id)
            .await
            .expect("assign r2");

        // list is name-ordered and carries the counts.
        let groups = groups_list(&pool).await.expect("list");
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].name, "backend");
        assert_eq!(groups[0].repo_count, 2);
        assert_eq!(groups[1].name, "frontend");
        assert_eq!(groups[1].repo_count, 0);
    }

    #[tokio::test]
    async fn duplicate_name_on_create_and_rename_maps_to_invalid_setting() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        group_create(&pool, "backend", None).await.expect("create");
        let other = group_create(&pool, "frontend", None)
            .await
            .expect("create other");

        // A duplicate name on create maps to InvalidSetting { field: "name" }.
        let dup = group_create(&pool, "backend", None).await;
        assert!(
            matches!(&dup, Err(AppError::InvalidSetting { field }) if field == "name"),
            "duplicate create name must map to InvalidSetting, got {dup:?}"
        );

        // A rename that collides with an existing name maps the same way.
        let clash = group_rename(&pool, other.id, "backend").await;
        assert!(
            matches!(&clash, Err(AppError::InvalidSetting { field }) if field == "name"),
            "duplicate rename name must map to InvalidSetting, got {clash:?}"
        );

        // Renaming a missing group id is NotFound.
        let missing = group_rename(&pool, 9999, "whatever").await;
        assert!(
            matches!(missing, Err(AppError::NotFound { .. })),
            "rename of a missing group must be NotFound"
        );
    }

    #[tokio::test]
    async fn assign_lists_and_unassign_round_trip() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let g1 = group_create(&pool, "one", None).await.expect("g1").id;
        let g2 = group_create(&pool, "two", None).await.expect("g2").id;
        let repo = insert_repo(&pool, "alpha", "C:/repos/alpha").await;

        // Assign to both groups; assigning twice is idempotent (INSERT OR IGNORE).
        group_assign(&pool, repo, g1).await.expect("assign g1");
        group_assign(&pool, repo, g2).await.expect("assign g2");
        group_assign(&pool, repo, g1)
            .await
            .expect("assign g1 again");

        // groups_for_repo is ascending and de-duplicated by the primary key.
        let mut expected = vec![g1, g2];
        expected.sort_unstable();
        assert_eq!(
            groups_for_repo(&pool, repo).await.expect("for repo"),
            expected
        );

        // Unassign one; the other remains. Unassigning again is a no-op.
        group_unassign(&pool, repo, g1).await.expect("unassign g1");
        group_unassign(&pool, repo, g1)
            .await
            .expect("unassign g1 again");
        assert_eq!(
            groups_for_repo(&pool, repo).await.expect("for repo"),
            vec![g2]
        );
    }

    #[tokio::test]
    async fn assign_missing_repo_or_group_is_not_found() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let group = group_create(&pool, "one", None).await.expect("group").id;
        let repo = insert_repo(&pool, "alpha", "C:/repos/alpha").await;

        // A missing group id (foreign key on repo_groups.group_id) is NotFound.
        let bad_group = group_assign(&pool, repo, 9999).await;
        assert!(
            matches!(bad_group, Err(AppError::NotFound { .. })),
            "assigning to a missing group must be NotFound, got {bad_group:?}"
        );

        // A missing repo id (foreign key on repo_groups.repo_id) is NotFound.
        let bad_repo = group_assign(&pool, 9999, group).await;
        assert!(
            matches!(bad_repo, Err(AppError::NotFound { .. })),
            "assigning a missing repo must be NotFound, got {bad_repo:?}"
        );
    }

    #[tokio::test]
    async fn delete_cascades_memberships_and_is_idempotent() {
        let dbtmp = TempDir::new().unwrap();
        let pool = fresh_pool(dbtmp.path()).await;

        let group = group_create(&pool, "one", None).await.expect("group").id;
        let repo = insert_repo(&pool, "alpha", "C:/repos/alpha").await;
        group_assign(&pool, repo, group).await.expect("assign");
        assert_eq!(groups_for_repo(&pool, repo).await.unwrap(), vec![group]);

        // Deleting the group cascades away the repo_groups membership.
        group_delete(&pool, group).await.expect("delete");
        assert!(
            groups_for_repo(&pool, repo).await.unwrap().is_empty(),
            "ON DELETE CASCADE must clear the repo's membership"
        );
        let membership_count: i64 = sqlx::query("SELECT COUNT(*) AS c FROM repo_groups")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("c")
            .unwrap();
        assert_eq!(membership_count, 0, "no orphaned memberships remain");

        // Deleting again (and deleting a never-existent id) is a no-op, not an error.
        group_delete(&pool, group).await.expect("delete again");
        group_delete(&pool, 9999).await.expect("delete missing");
        assert!(groups_list(&pool).await.unwrap().is_empty());
    }
}
