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
use crate::ipc;

/// The default and hard cap on rows returned by [`list`] when the filter's `limit`
/// is absent or unreasonably large, so a UI read can never pull the whole log.
pub const ACTIVITY_DEFAULT_LIMIT: i64 = 200;
pub const ACTIVITY_MAX_LIMIT: i64 = 1000;

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

/// Read `activity_records` newest-first, applying the optional [`ActivityFilter`]
/// (repo / action_type / status) and a row limit. The read-side counterpart to
/// [`record`]; E-09 owns only the writer + sweep, so this is the E-06/UI access
/// path (the `idx_activity_repo_time` / `idx_activity_time` indexes back the
/// `ORDER BY timestamp DESC` reads). The limit is clamped to
/// `ACTIVITY_DEFAULT_LIMIT` (absent) / `ACTIVITY_MAX_LIMIT` (cap) so a UI read can
/// never pull the whole log. Returns rows newest-first.
pub async fn list(
    pool: &SqlitePool,
    filter: &ipc::ActivityFilter,
) -> Result<Vec<ipc::ActivityRecord>, AppError> {
    // A non-positive limit is treated as unspecified (-> default), never as a
    // 0-row read; any positive limit is capped at the hard maximum.
    let limit = match filter.limit {
        Some(n) if n > 0 => n.min(ACTIVITY_MAX_LIMIT),
        _ => ACTIVITY_DEFAULT_LIMIT,
    };

    // sqlx 0.9's `SqlSafeStr` requires a compile-time-known query string, so the
    // filter cannot be a dynamically-built `String`. Use a STATIC query whose
    // `(? IS NULL OR col = ?)` guards make each filter optional: an absent filter
    // binds NULL (matching every row), a present one constrains. Each placeholder
    // is bound once (the Option is bound twice), so the values stay parameterized -
    // never interpolated. `ORDER BY timestamp DESC` is served by the E-02 indexes.
    let rows = sqlx::query(
        "SELECT id, repo_id, timestamp, action_type, status, reason_code, summary, \
         commit_range, raw_command, raw_stdout, raw_stderr, exit_code, duration_ms \
         FROM activity_records \
         WHERE (? IS NULL OR repo_id = ?) \
           AND (? IS NULL OR action_type = ?) \
           AND (? IS NULL OR status = ?) \
         ORDER BY timestamp DESC LIMIT ?",
    )
    .bind(filter.repo_id)
    .bind(filter.repo_id)
    .bind(filter.action_type.as_deref())
    .bind(filter.action_type.as_deref())
    .bind(filter.status.as_deref())
    .bind(filter.status.as_deref())
    .bind(limit)
    .fetch_all(pool)
    .await?;
    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        out.push(ipc::ActivityRecord {
            id: r.try_get("id")?,
            repo_id: r.try_get("repo_id")?,
            timestamp: r.try_get("timestamp")?,
            action_type: r.try_get("action_type")?,
            status: r.try_get("status")?,
            reason_code: r.try_get("reason_code")?,
            summary: r.try_get("summary")?,
            commit_range: r.try_get("commit_range")?,
            raw_command: r.try_get("raw_command")?,
            raw_stdout: r.try_get("raw_stdout")?,
            raw_stderr: r.try_get("raw_stderr")?,
            exit_code: r.try_get("exit_code")?,
            duration_ms: r.try_get("duration_ms")?,
        });
    }
    Ok(out)
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
        // A future/regressed last-sweep time (the clock went backward, or a
        // corrupted row is dated ahead of now) must not suppress sweeps forever:
        // treat `now < last` as due so pruning recovers. `now == last` is "just
        // swept", so it stays not-due.
        Some(last) => now_unix < last || now_unix - last >= SECONDS_PER_DAY,
    }
}

/// Read `settings.activity_retention_d` live; `DEFAULT_RETENTION_DAYS` if the row
/// is missing or unreadable.
async fn read_retention_days(pool: &SqlitePool) -> i64 {
    let row = sqlx::query("SELECT activity_retention_d FROM settings WHERE id = 1")
        .fetch_optional(pool)
        .await;
    let stored = match row {
        Ok(Some(r)) => r
            .try_get::<i64, _>("activity_retention_d")
            .unwrap_or(DEFAULT_RETENTION_DAYS),
        _ => DEFAULT_RETENTION_DAYS,
    };
    // Defense-in-depth for a DESTRUCTIVE delete: a 0 or negative retention sets the
    // cutoff at or after `now` and would wipe the whole log. settings_set validates
    // >= 1 on the normal write path, but a direct DB edit, a future migration, or a
    // corrupted row must never be trusted here - fall back to the default.
    if stored < 1 {
        DEFAULT_RETENTION_DAYS
    } else {
        stored
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

    #[tokio::test]
    async fn list_filters_orders_newest_first_and_falls_back_on_bad_limit() {
        // The E-06/UI read path: newest-first ordering, each optional filter
        // (repo / status) constrains, the limit caps, and a non-positive limit
        // falls back to the default rather than reading nothing or the whole log.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let r1 = seed_repo(&pool, "r1").await;
        let r2 = seed_repo(&pool, "r2").await;
        record(&pool, &success_input(r1, 1000)).await; // update / success
        record(&pool, &failure_input(r1, 2000)).await; // check  / failed
        record(&pool, &success_input(r2, 3000)).await; // update / success

        let none = ipc::ActivityFilter {
            repo_id: None,
            action_type: None,
            status: None,
            limit: None,
        };

        // No filter -> all three, newest (largest timestamp) first.
        let all = list(&pool, &none).await.unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].timestamp, 3000, "newest first");
        assert_eq!(all[2].timestamp, 1000, "oldest last");
        // Mapping carries the full row (the failed middle row's classified fields).
        assert_eq!(all[1].status, "failed");
        assert_eq!(all[1].reason_code.as_deref(), Some("net.offline"));
        assert_eq!(all[1].exit_code, Some(128));

        // Filter by repo.
        let only_r1 = list(
            &pool,
            &ipc::ActivityFilter {
                repo_id: Some(r1),
                ..none.clone()
            },
        )
        .await
        .unwrap();
        assert_eq!(only_r1.len(), 2);
        assert!(only_r1.iter().all(|a| a.repo_id == r1));

        // Filter by status.
        let failed = list(
            &pool,
            &ipc::ActivityFilter {
                status: Some("failed".into()),
                ..none.clone()
            },
        )
        .await
        .unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].action_type, "check");

        // A positive limit keeps the newest N.
        let limited = list(
            &pool,
            &ipc::ActivityFilter {
                limit: Some(1),
                ..none.clone()
            },
        )
        .await
        .unwrap();
        assert_eq!(limited.len(), 1);
        assert_eq!(limited[0].timestamp, 3000, "limit keeps the newest");

        // A non-positive limit is treated as unspecified -> default, not 0 rows.
        let zero = list(
            &pool,
            &ipc::ActivityFilter {
                limit: Some(0),
                ..none.clone()
            },
        )
        .await
        .unwrap();
        assert_eq!(
            zero.len(),
            3,
            "limit <= 0 falls back to the default, not an empty read"
        );
    }

    // --- Codex E-09 review fixes -------------------------------------------------

    #[tokio::test]
    async fn sweep_clamps_nonpositive_retention_to_default_not_wipe() {
        // HIGH (Codex E-09): a 0 or negative activity_retention_d must NOT wipe the
        // log. settings_set validates >= 1 on the write path, but the destructive
        // sweep must not trust the read value (a direct DB edit / future migration /
        // corruption could set it). read_retention_days falls back to the default
        // for any non-positive value, so a recent row always survives.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let repo = seed_repo(&pool, "r").await;
        let now = 100 * SECONDS_PER_DAY;
        record(&pool, &success_input(repo, now - SECONDS_PER_DAY)).await; // 1 day old
        record(&pool, &success_input(repo, now - 200 * SECONDS_PER_DAY)).await; // ancient

        for bad in [0_i64, -1, -90] {
            sqlx::query(
                "INSERT INTO settings (id, activity_retention_d) VALUES (1, ?) \
                 ON CONFLICT(id) DO UPDATE SET activity_retention_d = excluded.activity_retention_d",
            )
            .bind(bad)
            .execute(&pool)
            .await
            .unwrap();
            let pruned = sweep(&pool, now).await.unwrap();
            assert!(
                pruned <= 1,
                "a non-positive retention ({bad}) must not mass-delete, pruned {pruned}"
            );
        }
        assert_eq!(
            count(&pool).await,
            1,
            "the recent (1-day-old) row must survive any 0/negative retention"
        );
    }

    #[test]
    fn sweep_due_recovers_from_a_future_or_regressed_last_sweep() {
        // MEDIUM (Codex E-09): a last-sweep time AFTER now (clock regression or a
        // corrupted future-dated row) must not suppress sweeps forever; treat it as
        // due so pruning recovers. now == last is still "just swept" -> not due.
        assert!(
            sweep_due(Some(2_000_000), 1_000_000),
            "a far-future last-sweep must read as due (recovery), not suppressed"
        );
        assert!(
            sweep_due(Some(1_000_001), 1_000_000),
            "even 1s in the future reads as due"
        );
        assert!(
            !sweep_due(Some(1_000_000), 1_000_000),
            "swept this exact instant is not due"
        );
    }
}
