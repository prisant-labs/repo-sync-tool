//! activity - owned by E-09 (the activity-log writer and the retention sweep).
//!
//! The audit trail: an append-only record of every git operation. Each row pairs
//! the GIT-CAPTURED raw-execution fields (command/stdout/stderr/exit/duration,
//! produced by E-03 `git/cli.rs`) with the CALLER-CLASSIFIED semantic fields
//! (action_type/status/reason_code/summary/commit_range, supplied by the E-07
//! policy engine and the E-08/E-03/E-10 call sites). The git CLI never classifies
//! a row; it only supplies the raw half.
//!
//! [`record`] is the single sink every git path writes through, so no operation
//! goes unlogged and every row is shaped consistently. [`sweep`] prunes rows
//! older than `settings.activity_retention_d` (default 90, read live) so the log
//! does not grow without bound.
//!
//! Tauri-free; sqlx RUNTIME query API; unix-seconds timestamps (no chrono).

use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::{Row, SqlitePool};

use crate::error::AppError;

/// The retention default (days), mirroring the schema's `activity_retention_d`
/// default. Used when the settings row is missing or unreadable.
pub const DEFAULT_RETENTION_DAYS: i64 = 90;

/// Seconds in a day, for the retention cutoff math.
const SECONDS_PER_DAY: i64 = 86_400;

/// Current unix time in whole seconds.
fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// One activity-log row to append. The two provenance classes are explicit: the
/// caller classifies the semantic fields; E-03 supplies the raw execution
/// capture. `Option` for the raw fields lets a locally-decided operation (which
/// issued no git command) store NULL where there was nothing to capture.
#[derive(Debug, Clone, Default)]
pub struct ActivityInput {
    /// The owning repo (FK into `repos`).
    pub repo_id: i64,
    /// Caller-supplied timestamp (unix seconds). `None` -> now at insert time (an
    /// injected clock for deterministic tests).
    pub timestamp: Option<i64>,
    // --- caller-classified (E-07 / E-08 / E-03 / E-10), NOT parsed from git ---
    pub action_type: String,
    pub status: String,
    pub reason_code: Option<String>,
    pub summary: Option<String>,
    pub commit_range: Option<String>,
    // --- git-captured (E-03 git/cli.rs) ---
    pub raw_command: Option<String>,
    pub raw_stdout: Option<String>,
    pub raw_stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

/// Append one fully-populated `activity_records` row (the single sink, AC1).
///
/// BEST-EFFORT BY DESIGN: a logging failure must never abort the git operation
/// being recorded (the operation already happened). On a DB write error the
/// failure is logged and swallowed; this function returns `()` and never
/// propagates. Failed operations are recorded too (non-zero `exit_code` +
/// captured `raw_stderr`), never dropped (AC2).
pub async fn record(pool: &SqlitePool, input: &ActivityInput) {
    let ts = input.timestamp.unwrap_or_else(now_secs);
    let res = sqlx::query(
        "INSERT INTO activity_records \
         (repo_id, timestamp, action_type, status, reason_code, summary, commit_range, \
          raw_command, raw_stdout, raw_stderr, exit_code, duration_ms) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(input.repo_id)
    .bind(ts)
    .bind(&input.action_type)
    .bind(&input.status)
    .bind(&input.reason_code)
    .bind(&input.summary)
    .bind(&input.commit_range)
    .bind(&input.raw_command)
    .bind(&input.raw_stdout)
    .bind(&input.raw_stderr)
    .bind(input.exit_code)
    .bind(input.duration_ms)
    .execute(pool)
    .await;
    if let Err(e) = res {
        // Best-effort: the git operation already happened; a logging failure must
        // not abort it. Log and swallow.
        eprintln!(
            "activity: failed to record a '{}' row for repo {}: {e}",
            input.action_type, input.repo_id
        );
    }
}

/// Delete `activity_records` older than `settings.activity_retention_d` days
/// (read LIVE; default 90), relative to `now_unix`. Returns the rows pruned.
/// Short transaction, no lock held across anything else (AC3).
pub async fn sweep(pool: &SqlitePool, now_unix: i64) -> Result<u64, AppError> {
    let retention_days = read_retention_days(pool).await;
    let cutoff = now_unix - retention_days * SECONDS_PER_DAY;
    let res = sqlx::query("DELETE FROM activity_records WHERE timestamp < ?")
        .bind(cutoff)
        .execute(pool)
        .await?;
    Ok(res.rows_affected())
}

/// Run the retention sweep once at startup (no UI trigger, AC4). Best-effort: a
/// sweep failure is logged, not propagated, so app start is never gated on it.
pub async fn sweep_at_startup(pool: &SqlitePool) {
    match sweep(pool, now_secs()).await {
        Ok(n) if n > 0 => eprintln!("activity: startup retention sweep pruned {n} record(s)"),
        Ok(_) => {}
        Err(e) => eprintln!("activity: startup retention sweep failed: {e}"),
    }
}

/// Whether a daily sweep is due, given the last sweep's unix time (`None` if
/// never swept) and now. The once-per-day guard the scheduler tick / launch
/// wiring uses to attach the daily cadence (AC4); pure, so it is unit-tested
/// directly.
pub fn sweep_due(last_sweep_unix: Option<i64>, now_unix: i64) -> bool {
    match last_sweep_unix {
        None => true,
        Some(last) => now_unix - last >= SECONDS_PER_DAY,
    }
}

/// Read `settings.activity_retention_d` live; `DEFAULT_RETENTION_DAYS` if the row
/// is missing or unreadable.
async fn read_retention_days(pool: &SqlitePool) -> i64 {
    let row = sqlx::query("SELECT activity_retention_d FROM settings WHERE id = 1")
        .fetch_optional(pool)
        .await;
    match row {
        Ok(Some(r)) => r
            .try_get::<i64, _>("activity_retention_d")
            .unwrap_or(DEFAULT_RETENTION_DAYS),
        _ => DEFAULT_RETENTION_DAYS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use tempfile::TempDir;

    async fn fresh_pool(dir: &std::path::Path) -> SqlitePool {
        let pool = db::open_pool(&dir.join("activity-test.db"))
            .await
            .expect("open_pool");
        db::run_migrations(&pool).await.expect("migrations");
        pool
    }

    /// Insert a bare `repos` row (the FK target for activity rows); return its id.
    async fn seed_repo(pool: &SqlitePool, name: &str) -> i64 {
        sqlx::query("INSERT INTO repos (local_name, local_path, created_at) VALUES (?, ?, 0)")
            .bind(name)
            .bind(name)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid()
    }

    fn success_input(repo_id: i64, ts: i64) -> ActivityInput {
        ActivityInput {
            repo_id,
            timestamp: Some(ts),
            action_type: "update".into(),
            status: "success".into(),
            reason_code: None,
            summary: Some("update: outcome=updated".into()),
            commit_range: Some("aaa..bbb".into()),
            raw_command: Some("git pull --ff-only".into()),
            raw_stdout: Some("Updating aaa..bbb".into()),
            raw_stderr: Some(String::new()),
            exit_code: Some(0),
            duration_ms: Some(42),
        }
    }

    fn failure_input(repo_id: i64, ts: i64) -> ActivityInput {
        ActivityInput {
            repo_id,
            timestamp: Some(ts),
            action_type: "check".into(),
            status: "failed".into(),
            reason_code: Some("net.offline".into()),
            summary: Some("check: fetch failed".into()),
            commit_range: None,
            raw_command: Some("git fetch --all".into()),
            raw_stdout: Some(String::new()),
            raw_stderr: Some("fatal: unable to access".into()),
            exit_code: Some(128),
            duration_ms: Some(1500),
        }
    }

    async fn count(pool: &SqlitePool) -> i64 {
        sqlx::query("SELECT COUNT(*) AS c FROM activity_records")
            .fetch_one(pool)
            .await
            .unwrap()
            .try_get("c")
            .unwrap()
    }

    #[tokio::test]
    async fn record_appends_every_column_for_success_and_failure() {
        // AC1 + AC2: both rows persist with all columns, including the injected
        // timestamp, the caller-classified fields, and the failure's non-zero
        // exit_code + non-empty stderr.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let repo = seed_repo(&pool, "r").await;

        record(&pool, &success_input(repo, 1000)).await;
        record(&pool, &failure_input(repo, 2000)).await;

        let s = sqlx::query("SELECT * FROM activity_records WHERE status = 'success'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(s.try_get::<i64, _>("repo_id").unwrap(), repo);
        assert_eq!(s.try_get::<i64, _>("timestamp").unwrap(), 1000);
        assert_eq!(s.try_get::<String, _>("action_type").unwrap(), "update");
        assert_eq!(s.try_get::<Option<String>, _>("reason_code").unwrap(), None);
        assert_eq!(
            s.try_get::<Option<String>, _>("commit_range")
                .unwrap()
                .as_deref(),
            Some("aaa..bbb")
        );
        assert_eq!(
            s.try_get::<Option<String>, _>("raw_command")
                .unwrap()
                .as_deref(),
            Some("git pull --ff-only")
        );
        assert_eq!(s.try_get::<Option<i64>, _>("exit_code").unwrap(), Some(0));
        assert_eq!(
            s.try_get::<Option<i64>, _>("duration_ms").unwrap(),
            Some(42)
        );

        let f = sqlx::query("SELECT * FROM activity_records WHERE status = 'failed'")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(f.try_get::<i64, _>("timestamp").unwrap(), 2000);
        assert_eq!(f.try_get::<Option<i64>, _>("exit_code").unwrap(), Some(128));
        assert_eq!(
            f.try_get::<Option<String>, _>("raw_stderr")
                .unwrap()
                .as_deref(),
            Some("fatal: unable to access")
        );
        assert_eq!(
            f.try_get::<Option<String>, _>("reason_code")
                .unwrap()
                .as_deref(),
            Some("net.offline")
        );
    }

    #[tokio::test]
    async fn record_defaults_timestamp_to_now_when_absent() {
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let repo = seed_repo(&pool, "r").await;
        let mut input = success_input(repo, 0);
        input.timestamp = None;
        let before = now_secs();
        record(&pool, &input).await;
        let ts: i64 = sqlx::query("SELECT timestamp FROM activity_records")
            .fetch_one(&pool)
            .await
            .unwrap()
            .try_get("timestamp")
            .unwrap();
        assert!(
            ts >= before,
            "an absent timestamp defaults to now at insert"
        );
    }

    #[tokio::test]
    async fn record_is_best_effort_and_never_aborts_on_write_error() {
        // A guaranteed write error (the target table is gone) must be logged and
        // swallowed: record returns normally and does not panic / propagate, so a
        // logging hiccup never aborts the git operation that already happened.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        sqlx::query("DROP TABLE activity_records")
            .execute(&pool)
            .await
            .unwrap();
        // Reaching the next line without a panic IS the assertion.
        record(&pool, &success_input(1, 1000)).await;
    }

    #[tokio::test]
    async fn sweep_prunes_older_than_retention_and_honors_live_setting() {
        // AC3: at retention 90, a 91-day-old row is pruned and an 89-day-old row
        // kept; lowering the live setting prunes more on the next sweep.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let repo = seed_repo(&pool, "r").await;
        let now = 100 * SECONDS_PER_DAY;
        record(&pool, &success_input(repo, now - 91 * SECONDS_PER_DAY)).await;
        record(&pool, &success_input(repo, now - 89 * SECONDS_PER_DAY)).await;

        let pruned = sweep(&pool, now).await.unwrap();
        assert_eq!(pruned, 1, "only the 91-day-old row prunes at retention 90");
        assert_eq!(count(&pool).await, 1);

        sqlx::query(
            "INSERT INTO settings (id, activity_retention_d) VALUES (1, 30) \
             ON CONFLICT(id) DO UPDATE SET activity_retention_d = 30",
        )
        .execute(&pool)
        .await
        .unwrap();
        let pruned2 = sweep(&pool, now).await.unwrap();
        assert_eq!(pruned2, 1, "the live 30-day setting prunes the 89-day row");
        assert_eq!(count(&pool).await, 0);
    }

    #[tokio::test]
    async fn sweep_at_startup_runs_with_no_ui() {
        // AC4: the sweep runs from the startup entry point directly (no screen).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let repo = seed_repo(&pool, "r").await;
        let now = now_secs();
        record(&pool, &success_input(repo, now - 200 * SECONDS_PER_DAY)).await;
        sweep_at_startup(&pool).await;
        assert_eq!(
            count(&pool).await,
            0,
            "startup sweep prunes the 200-day-old row at default 90"
        );
    }

    #[test]
    fn sweep_due_is_a_once_per_day_guard() {
        // AC4: never swept -> due; swept < 24h ago -> not due; >= 24h ago -> due.
        assert!(sweep_due(None, 1_000_000));
        assert!(!sweep_due(Some(1_000_000 - 1000), 1_000_000));
        assert!(!sweep_due(
            Some(1_000_000 - (SECONDS_PER_DAY - 1)),
            1_000_000
        ));
        assert!(sweep_due(Some(1_000_000 - SECONDS_PER_DAY), 1_000_000));
        assert!(sweep_due(Some(0), 1_000_000));
    }

    async fn explain(pool: &SqlitePool, q: &'static str) -> Vec<String> {
        let rows = sqlx::query(q).fetch_all(pool).await.unwrap();
        rows.iter()
            .map(|r| r.try_get::<String, _>("detail").unwrap_or_default())
            .collect()
    }

    #[tokio::test]
    async fn recent_activity_read_uses_the_timestamp_index() {
        // AC5: a representative ordered read-back uses the E-02 indexes, not a full
        // scan. E-09 does not own the activity_list query (E-06/UI); this confirms
        // the access path is available.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let repo = seed_repo(&pool, "r").await;
        record(&pool, &success_input(repo, 1000)).await;

        let plan_repo = explain(
            &pool,
            "EXPLAIN QUERY PLAN SELECT id FROM activity_records WHERE repo_id = 1 ORDER BY timestamp DESC LIMIT 50",
        )
        .await;
        assert!(
            plan_repo
                .iter()
                .any(|d| d.contains("idx_activity_repo_time")),
            "repo+time read must use idx_activity_repo_time; plan was {plan_repo:?}"
        );

        let plan_time = explain(
            &pool,
            "EXPLAIN QUERY PLAN SELECT id FROM activity_records ORDER BY timestamp DESC LIMIT 50",
        )
        .await;
        assert!(
            plan_time.iter().any(|d| d.contains("idx_activity_time")),
            "time read must use idx_activity_time; plan was {plan_time:?}"
        );
    }
}
