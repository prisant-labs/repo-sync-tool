//! repo - owned by E-02 / E-03 (the core flows where layers meet).
//!
//! Tracer slice: `add` (validate + inspect + persist) and `check_now`
//! (re-inspect + fetch + ahead/behind + record an activity row). Uses the sqlx
//! RUNTIME query API and unix-seconds timestamps (no chrono).

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::git::SystemGitEngine;
use crate::ipc::{CheckResult, RepoId};

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

    // 1. Look up the path and stored upstream.
    let row = sqlx::query("SELECT r.local_path AS local_path, s.upstream_branch AS upstream_branch \
         FROM repos r LEFT JOIN repo_local_state s ON s.repo_id = r.id \
         WHERE r.id = ?")
        .bind(repo_id)
        .fetch_one(pool)
        .await?;
    let local_path: String = row.try_get("local_path")?;
    let stored_upstream: Option<String> = row.try_get("upstream_branch")?;
    let path = Path::new(&local_path);

    // 2. Re-inspect local state.
    let inspect = git.inspect(path)?;
    let now = now_secs();

    // 3. Fetch (record raw output regardless of outcome).
    let fetch = git.fetch(path).await?;

    // 4. Resolve upstream: prefer the freshly inspected one, fall back to stored.
    let upstream = inspect
        .upstream_branch
        .clone()
        .or(stored_upstream);

    // 5. Ahead/behind when an upstream exists.
    let ahead_behind = match &upstream {
        Some(u) => git.ahead_behind(path, u).await?,
        None => crate::git::AheadBehind {
            ahead: None,
            behind: None,
        },
    };

    // 6. Tracer-inline decision policy.
    //
    // behind is Option<i64>: distinguish "no upstream / comparison failed"
    // (unknown) from Some(0) (known up to date). Collapsing None to 0 would
    // mislabel an un-compared repo as up to date, so unknown branches
    // explicitly to skip-with-reason rather than would-fast-forward.
    let behind = ahead_behind.behind;
    let (decision, reason): (String, Option<String>) = if !fetch.success {
        (
            "skip-with-reason".to_string(),
            Some("fetch failed".to_string()),
        )
    } else if inspect.is_detached {
        (
            "skip-with-reason".to_string(),
            Some("HEAD is detached".to_string()),
        )
    } else if inspect.is_dirty {
        (
            "skip-with-reason".to_string(),
            Some("working tree is dirty".to_string()),
        )
    } else if upstream.is_none() {
        (
            "skip-with-reason".to_string(),
            Some("no upstream".to_string()),
        )
    } else {
        match behind {
            None => (
                "skip-with-reason".to_string(),
                Some("comparison unavailable".to_string()),
            ),
            Some(0) => (
                "skip-with-reason".to_string(),
                Some("already up to date".to_string()),
            ),
            Some(_) => ("would-fast-forward".to_string(), None),
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
    use tempfile::TempDir;

    /// Build a git2 repo with one commit at `dir`. Returns nothing; panics on
    /// failure (test helper).
    fn init_repo_with_commit(dir: &Path) {
        let repo = git2::Repository::init(dir).expect("init repo");
        std::fs::write(dir.join("README.md"), "hello\n").expect("write file");

        let mut index = repo.index().expect("index");
        index
            .add_path(Path::new("README.md"))
            .expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("find tree");

        let sig = git2::Signature::now("Tracer Test", "tracer@example.com")
            .expect("signature");
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
            raw_command.as_deref().map(|s| s.contains("fetch")).unwrap_or(false),
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
}
