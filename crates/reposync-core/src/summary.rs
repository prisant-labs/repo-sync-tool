//! summary - owned by E-11 (the daily summary engine).
//!
//! A read-only roll-up of "what changed today": how many repos updated, how many
//! new releases landed, how many currently need attention, and how many were
//! checked with no change. It produces no new data - it aggregates the
//! `activity_records` E-09 writes plus the cached `repo_local_state` /
//! `repo_remote_meta` E-02 owns, with no git or network calls (AC2).
//!
//! The value it returns is the FROZEN `ipc::DailySummary` (E-06 owns the transport
//! type; E-11 populates it). The spec's "E-11 owns the shape" language predates the
//! E-06 freeze; the frozen `ipc` type wins, so this module conforms to it.
//!
//! Tauri-free; sqlx RUNTIME query API; unix-seconds timestamps (no chrono). The day
//! boundary is injected as a [`DayWindow`] so the local-midnight decision lives at
//! the edge and the aggregation stays deterministic in tests.

use std::collections::{BTreeMap, HashSet};

use sqlx::{Row, SqlitePool};

use crate::error::AppError;
use crate::ipc::{DailySummary, SummaryItem, WeeklySummary};

/// The half-open day window `[start_unix, end_unix)` the daily summary aggregates
/// over, plus the `YYYY-MM-DD` label echoed into `DailySummary.date`.
///
/// The caller (the edge) computes local-midnight bounds and the label; tests inject
/// fixed values. Keeping timezone math out of `reposync-core` sidesteps the flagged
/// DST / local-time question and makes every aggregation test deterministic.
#[derive(Debug, Clone)]
pub struct DayWindow {
    /// The `YYYY-MM-DD` label for the day, echoed verbatim into `DailySummary.date`.
    pub date: String,
    /// Inclusive lower bound (unix seconds).
    pub start_unix: i64,
    /// Exclusive upper bound (unix seconds).
    pub end_unix: i64,
}

impl DayWindow {
    /// Construct a validated window (Codex review finding 4): a non-empty label and a
    /// non-empty, non-inverted range (`end > start`). The edge should build windows
    /// through this so a timezone / DST miscalculation cannot pass a malformed window.
    pub fn new(date: impl Into<String>, start_unix: i64, end_unix: i64) -> Result<Self, AppError> {
        let date = date.into();
        validate_window(&date, start_unix, end_unix)?;
        Ok(Self {
            date,
            start_unix,
            end_unix,
        })
    }
}

/// Validate a day window: a non-empty label and a non-empty, non-inverted range.
/// A malformed window is a caller bug (a timezone / DST miscalculation at the edge),
/// surfaced as an internal error rather than silently producing a mislabelled summary
/// (Codex review finding 4).
fn validate_window(date: &str, start_unix: i64, end_unix: i64) -> Result<(), AppError> {
    if date.is_empty() {
        return Err(AppError::Unexpected {
            context: "summary day window has an empty date label".into(),
        });
    }
    if end_unix <= start_unix {
        return Err(AppError::Unexpected {
            context: format!(
                "summary day window is empty or inverted: start={start_unix}, end={end_unix}"
            ),
        });
    }
    Ok(())
}

/// The daily bucket an activity row contributes to. Failures / warnings and admin
/// events (enable / disable / open / manual_retry) map to neither: a failure
/// surfaces via the attention STATE definition (AC5), and an admin event is not a
/// check, so neither inflates the updated / no-change tallies.
enum DayBucket {
    Updated,
    NoChange,
}

/// Classify one activity row into a daily bucket (AC4), handling BOTH the
/// implemented writer vocabulary (`check` / `update`, the E-03 / E-07 paths) and
/// the spec's idealized enum (`pull_ff` / `pull` / `rebase` / `fetch`), so the tally
/// stays correct if either evolves.
///
/// `has_range` is whether the row carries a `commit_range`. The range - not the
/// action label - is the "tree advanced" signal: the update path sets one only when a
/// fast-forward moved HEAD, and a successful `pull` / `rebase` can equally be a no-op.
/// So an update-like action counts as `Updated` ONLY with a range; without one it is
/// an up-to-date no-op (Codex review finding 1a).
fn classify_row(status: &str, action_type: &str, has_range: bool) -> Option<DayBucket> {
    match status {
        "success" => match action_type {
            "update" | "pull_ff" | "pull" | "rebase" => Some(if has_range {
                DayBucket::Updated
            } else {
                DayBucket::NoChange
            }),
            "check" | "fetch" => Some(DayBucket::NoChange),
            _ => None,
        },
        "skipped" => Some(DayBucket::NoChange),
        _ => None,
    }
}

/// Aggregate the day's activity and current state into a [`DailySummary`] (AC1).
///
/// Read-only (AC2): three grouped SELECTs and no writes, no git, no network.
/// Counts are DISTINCT repos, not rows. `updated` takes precedence over `no-change`
/// for a repo both checked and updated today. Attention is the E-07-free state
/// definition (AC5); releases are keyed on the release's own date (AC4).
pub async fn summary_today(
    pool: &SqlitePool,
    window: &DayWindow,
) -> Result<DailySummary, AppError> {
    // Reject a malformed window up front rather than silently mislabelling the result
    // (Codex review finding 4).
    validate_window(&window.date, window.start_unix, window.end_unix)?;

    // 1. Per-repo activity aggregate from today's rows, collapsed to distinct repos.
    //    `updated` wins over `no-change` for a repo with both today.
    struct Agg {
        local_name: String,
        updated: bool,
        no_change: bool,
        detail: Option<String>,
    }
    // BTreeMap keys (repo_id) iterate ascending, so the item lists are stable.
    let mut by_repo: BTreeMap<i64, Agg> = BTreeMap::new();

    let rows = sqlx::query(
        "SELECT ar.repo_id AS repo_id, ar.action_type AS action_type, ar.status AS status, \
                ar.commit_range AS commit_range, ar.summary AS summary, r.local_name AS local_name \
         FROM activity_records ar \
         JOIN repos r ON r.id = ar.repo_id \
         WHERE ar.timestamp >= ? AND ar.timestamp < ? \
         ORDER BY ar.timestamp ASC, ar.id ASC",
    )
    .bind(window.start_unix)
    .bind(window.end_unix)
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let repo_id: i64 = row.try_get("repo_id")?;
        let action_type: String = row.try_get("action_type")?;
        let status: String = row.try_get("status")?;
        let commit_range: Option<String> = row.try_get("commit_range")?;
        let summary: Option<String> = row.try_get("summary")?;
        let local_name: String = row.try_get("local_name")?;
        let has_range = commit_range.is_some();

        let entry = by_repo.entry(repo_id).or_insert_with(|| Agg {
            local_name,
            updated: false,
            no_change: false,
            detail: None,
        });
        match classify_row(&status, &action_type, has_range) {
            Some(DayBucket::Updated) => {
                entry.updated = true;
                // Prefer the commit range as the item detail; fall back to the row
                // summary. Rows are time-ordered, so the latest updated row wins.
                entry.detail = commit_range.or(summary);
            }
            Some(DayBucket::NoChange) => entry.no_change = true,
            None => {}
        }
    }

    // 2. Attention: the E-07-free current-STATE definition (AC5) - repos with
    //    `last_error_code` set OR `is_dirty` set, read straight from the state table.
    //
    //    By DESIGN this reads CURRENT state, not today's activity (Codex review finding
    //    3): the brief's "what STILL wants attention" is present-tense, so a repo that
    //    has been dirty / errored since before today is correctly still attention. The
    //    richer threshold- / window-based semantics E-07 owns are deferred (BL-TQ-03).
    //    The id set lets the no-change tally exclude a repo that needs attention, so a
    //    dirty repo with a clean check today is reported as attention, not as a calm
    //    "no change" (finding 1b: the buckets are disjoint, attention wins).
    let attention_rows = sqlx::query(
        "SELECT rls.repo_id AS repo_id, rls.last_error_code AS last_error_code, \
                rls.is_dirty AS is_dirty, r.local_name AS local_name \
         FROM repo_local_state rls \
         JOIN repos r ON r.id = rls.repo_id \
         WHERE rls.last_error_code IS NOT NULL OR rls.is_dirty = 1 \
         ORDER BY rls.repo_id ASC",
    )
    .fetch_all(pool)
    .await?;

    let mut attention: Vec<SummaryItem> = Vec::new();
    let mut attention_ids: HashSet<i64> = HashSet::new();
    for row in &attention_rows {
        let repo_id: i64 = row.try_get("repo_id")?;
        let last_error_code: Option<String> = row.try_get("last_error_code")?;
        let is_dirty: i64 = row.try_get("is_dirty")?;
        // Prefer the error code as the human-facing hint; otherwise note the dirty
        // working tree.
        let detail = match last_error_code {
            Some(code) => Some(code),
            None if is_dirty != 0 => Some("uncommitted changes".to_string()),
            None => None,
        };
        attention_ids.insert(repo_id);
        attention.push(SummaryItem {
            repo_id,
            local_name: row.try_get("local_name")?,
            detail,
        });
    }

    // 3. Updated / no-change tally over the per-repo activity aggregate. `updated`
    //    wins over `no-change`; a repo currently needing attention is excluded from
    //    no-change (it is reported as attention instead, finding 1b).
    let mut updated: Vec<SummaryItem> = Vec::new();
    let mut no_change_count: i64 = 0;
    for (repo_id, agg) in &by_repo {
        if agg.updated {
            updated.push(SummaryItem {
                repo_id: *repo_id,
                local_name: agg.local_name.clone(),
                detail: agg.detail.clone(),
            });
        } else if agg.no_change && !attention_ids.contains(repo_id) {
            no_change_count += 1;
        }
    }

    // 4. New releases for today. NOTE (Codex review finding 2): the only V1 source is
    //    the MUTABLE `repo_remote_meta` latest-release snapshot (one row per repo; the
    //    V1 schema has no release-event history). Keying on `latest_release_at` in the
    //    window is correct for the V1 contract (computed on demand, for today, with no
    //    persisted archive), but it cannot reconstruct a PAST day after a newer release
    //    overwrote the snapshot, and it collapses multiple same-day releases to one.
    //    The faithful fix needs an immutable release-event table and is coupled to the
    //    E-10 wiring effort; tracked as BL-NI-16.
    let release_rows = sqlx::query(
        "SELECT rrm.repo_id AS repo_id, rrm.latest_release_tag AS tag, r.local_name AS local_name \
         FROM repo_remote_meta rrm \
         JOIN repos r ON r.id = rrm.repo_id \
         WHERE rrm.latest_release_at IS NOT NULL \
           AND rrm.latest_release_at >= ? AND rrm.latest_release_at < ? \
         ORDER BY rrm.repo_id ASC",
    )
    .bind(window.start_unix)
    .bind(window.end_unix)
    .fetch_all(pool)
    .await?;

    let mut new_releases: Vec<SummaryItem> = Vec::new();
    for row in &release_rows {
        new_releases.push(SummaryItem {
            repo_id: row.try_get("repo_id")?,
            local_name: row.try_get("local_name")?,
            detail: row.try_get("tag")?,
        });
    }

    Ok(DailySummary {
        date: window.date.clone(),
        updated_count: updated.len() as i64,
        releases_count: new_releases.len() as i64,
        attention_count: attention.len() as i64,
        no_change_count,
        updated,
        new_releases,
        attention,
    })
}

/// The V1.1 weekly-aggregation seam (AC3): an INERT documented stub.
///
/// V1 ships daily only; weekly is CUT to V1.1 (brief Section 3 / 4.4). This names
/// the future surface - a window of [`DailySummary`] days over the same
/// `activity_records` / state data - so E-06 can bind it without it doing real work.
/// It performs no aggregation: it echoes the requested `week_start` and returns no
/// days. Promoting it to a real weekly roll-up in V1.1 is purely additive.
pub fn summary_week(week_start: &str) -> WeeklySummary {
    WeeklySummary {
        week_start: week_start.to_string(),
        days: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{self, ActivityInput};
    use crate::db;
    use tempfile::TempDir;

    const DAY: i64 = 86_400;

    async fn fresh_pool(dir: &std::path::Path) -> SqlitePool {
        let pool = db::open_pool(&dir.join("summary-test.db"))
            .await
            .expect("open_pool");
        db::run_migrations(&pool).await.expect("migrations");
        pool
    }

    /// Insert a bare `repos` row (the FK target); return its id.
    async fn seed_repo(pool: &SqlitePool, name: &str) -> i64 {
        sqlx::query("INSERT INTO repos (local_name, local_path, created_at) VALUES (?, ?, 0)")
            .bind(name)
            .bind(name)
            .execute(pool)
            .await
            .unwrap()
            .last_insert_rowid()
    }

    /// One clean synthetic day, with a label to assert echoing.
    fn window() -> DayWindow {
        DayWindow {
            date: "2026-06-29".into(),
            start_unix: 1000 * DAY,
            end_unix: 1001 * DAY,
        }
    }

    /// Record an `update` row the way the E-07 update path does (commit_range Some
    /// only when a fast-forward advanced the tree).
    async fn record_update(
        pool: &SqlitePool,
        repo: i64,
        ts: i64,
        status: &str,
        range: Option<&str>,
    ) {
        activity::record(
            pool,
            &ActivityInput {
                repo_id: repo,
                timestamp: Some(ts),
                action_type: "update".into(),
                status: status.into(),
                summary: Some("update".into()),
                commit_range: range.map(|s| s.to_string()),
                ..Default::default()
            },
        )
        .await;
    }

    /// Record a `check` row the way the E-03/E-07 check path does (commit_range
    /// always None - a check never advances the tree).
    async fn record_check(pool: &SqlitePool, repo: i64, ts: i64, status: &str) {
        activity::record(
            pool,
            &ActivityInput {
                repo_id: repo,
                timestamp: Some(ts),
                action_type: "check".into(),
                status: status.into(),
                summary: Some("check".into()),
                commit_range: None,
                ..Default::default()
            },
        )
        .await;
    }

    async fn seed_state(pool: &SqlitePool, repo: i64, err: Option<&str>, dirty: i64) {
        sqlx::query(
            "INSERT INTO repo_local_state (repo_id, last_error_code, is_dirty) VALUES (?, ?, ?)",
        )
        .bind(repo)
        .bind(err)
        .bind(dirty)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn seed_release(pool: &SqlitePool, repo: i64, tag: &str, at: i64) {
        sqlx::query(
            "INSERT INTO repo_remote_meta (repo_id, latest_release_tag, latest_release_at) \
             VALUES (?, ?, ?)",
        )
        .bind(repo)
        .bind(tag)
        .bind(at)
        .execute(pool)
        .await
        .unwrap();
    }

    async fn count_rows(pool: &SqlitePool, table: &'static str) -> i64 {
        let sql = format!("SELECT COUNT(*) FROM {table}");
        sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(sql))
            .fetch_one(pool)
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn updated_and_no_change_count_distinct_repos_by_precedence() {
        // AC4: a successful update that advanced the tree (commit_range present) is
        // `updated`; a successful check, or a successful update no-op (no range), is
        // `no-change`. Counts are DISTINCT repos, and `updated` wins over `no-change`
        // for a repo that was both checked and updated today. A failed row tallies to
        // neither (it surfaces via the attention STATE definition, AC5).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();
        let noon = w.start_unix + DAY / 2;

        let a = seed_repo(&pool, "a").await; // updated (range)
        let b = seed_repo(&pool, "b").await; // no-change (check)
        let c = seed_repo(&pool, "c").await; // no-change (update no-op)
        let d = seed_repo(&pool, "d").await; // checked then updated -> updated
        let e = seed_repo(&pool, "e").await; // failed -> neither

        record_update(&pool, a, noon, "success", Some("aaa..bbb")).await;
        record_check(&pool, b, noon, "success").await;
        record_update(&pool, c, noon, "success", None).await;
        record_check(&pool, d, noon - 3600, "success").await;
        record_update(&pool, d, noon + 3600, "success", Some("ccc..ddd")).await;
        record_check(&pool, e, noon, "failed").await;

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(s.updated_count, 2, "a and d updated");
        assert_eq!(
            s.no_change_count, 2,
            "b and c no-change; d not double-counted"
        );
        let updated_ids: Vec<i64> = s.updated.iter().map(|i| i.repo_id).collect();
        assert_eq!(
            updated_ids,
            vec![a, d],
            "updated items are a and d, repo-id ordered"
        );
        assert_eq!(s.updated[0].detail.as_deref(), Some("aaa..bbb"));
        assert_eq!(s.updated[1].detail.as_deref(), Some("ccc..ddd"));
        assert_eq!(s.date, "2026-06-29");
    }

    #[tokio::test]
    async fn releases_detected_only_within_today_window() {
        // AC4: a release is counted only when its OWN date (`latest_release_at`) falls
        // in today's window, keyed on the release date not the fetch time, so it is
        // not re-counted on later days.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();

        let f = seed_repo(&pool, "f").await;
        let g = seed_repo(&pool, "g").await;
        seed_release(&pool, f, "v2.0.0", w.start_unix + 100).await; // today
        seed_release(&pool, g, "v1.0.0", w.start_unix - 100).await; // yesterday

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(s.releases_count, 1);
        assert_eq!(s.new_releases.len(), 1);
        assert_eq!(s.new_releases[0].repo_id, f);
        assert_eq!(s.new_releases[0].detail.as_deref(), Some("v2.0.0"));
    }

    #[tokio::test]
    async fn attention_counts_error_or_dirty_state_rows() {
        // AC5: attention is the E-07-FREE current-state definition - count repos in
        // repo_local_state with last_error_code set OR is_dirty set. Not derived from
        // today's activity rows.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();

        let h = seed_repo(&pool, "h").await;
        let i = seed_repo(&pool, "i").await;
        let j = seed_repo(&pool, "j").await;
        seed_state(&pool, h, Some("git.fetch_failed"), 0).await;
        seed_state(&pool, i, None, 1).await;
        seed_state(&pool, j, None, 0).await;

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(s.attention_count, 2);
        let ids: Vec<i64> = s.attention.iter().map(|x| x.repo_id).collect();
        assert_eq!(ids, vec![h, i]);
        assert_eq!(s.attention[0].detail.as_deref(), Some("git.fetch_failed"));
        assert_eq!(
            s.attention[1].detail.as_deref(),
            Some("uncommitted changes")
        );
    }

    #[tokio::test]
    async fn day_window_is_half_open_start_inclusive_end_exclusive() {
        // The day boundary is `[start, end)`: a row at `start` counts, a row at `end`
        // does not, so adjacent days never bleed in.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();

        let before = seed_repo(&pool, "before").await;
        let at_start = seed_repo(&pool, "at_start").await;
        let end_minus_1 = seed_repo(&pool, "end_minus_1").await;
        let at_end = seed_repo(&pool, "at_end").await;
        record_update(&pool, before, w.start_unix - 1, "success", Some("x..y")).await;
        record_update(&pool, at_start, w.start_unix, "success", Some("x..y")).await;
        record_update(&pool, end_minus_1, w.end_unix - 1, "success", Some("x..y")).await;
        record_update(&pool, at_end, w.end_unix, "success", Some("x..y")).await;

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(
            s.updated_count, 2,
            "only start and end-1 fall in [start, end)"
        );
        let ids: Vec<i64> = s.updated.iter().map(|x| x.repo_id).collect();
        assert_eq!(ids, vec![at_start, end_minus_1]);
    }

    #[tokio::test]
    async fn empty_day_returns_zeroed_summary_not_error() {
        // The calm "nothing changed" state: zero tallies, empty lists, valid date, no
        // error.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(s.updated_count, 0);
        assert_eq!(s.releases_count, 0);
        assert_eq!(s.attention_count, 0);
        assert_eq!(s.no_change_count, 0);
        assert!(s.updated.is_empty() && s.new_releases.is_empty() && s.attention.is_empty());
        assert_eq!(s.date, "2026-06-29");
    }

    #[tokio::test]
    async fn summary_today_is_read_only() {
        // AC2: the aggregation only reads. Seed a mix, snapshot row counts, run the
        // summary, and assert nothing was written.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();
        let noon = w.start_unix + DAY / 2;
        let r = seed_repo(&pool, "r").await;
        record_update(&pool, r, noon, "success", Some("a..b")).await;
        seed_state(&pool, r, Some("git.fetch_failed"), 1).await;
        seed_release(&pool, r, "v1.0.0", noon).await;

        let before = (
            count_rows(&pool, "activity_records").await,
            count_rows(&pool, "repo_local_state").await,
            count_rows(&pool, "repo_remote_meta").await,
        );
        let _ = summary_today(&pool, &w).await.unwrap();
        let after = (
            count_rows(&pool, "activity_records").await,
            count_rows(&pool, "repo_local_state").await,
            count_rows(&pool, "repo_remote_meta").await,
        );
        assert_eq!(before, after, "summary_today must not write any rows");
    }

    #[tokio::test]
    async fn spec_vocab_update_without_range_is_no_change_not_updated() {
        // Codex review finding 1a: a success pull_ff / pull / rebase can be a no-op
        // (already up to date). Without a commit_range - the universal "tree advanced"
        // signal - it must count as no-change, consistent with the `update` path, not
        // be inflated as an update.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();
        let noon = w.start_unix + DAY / 2;

        let p = seed_repo(&pool, "p").await; // pull_ff success, NO range -> no-op
        let q = seed_repo(&pool, "q").await; // rebase success WITH range -> updated
        for (repo, action, range) in [(p, "pull_ff", None), (q, "rebase", Some("a..b"))] {
            activity::record(
                &pool,
                &ActivityInput {
                    repo_id: repo,
                    timestamp: Some(noon),
                    action_type: action.into(),
                    status: "success".into(),
                    summary: Some(action.into()),
                    commit_range: range.map(|s: &str| s.to_string()),
                    ..Default::default()
                },
            )
            .await;
        }

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(
            s.updated_count, 1,
            "only the rebase with a range is an update"
        );
        assert_eq!(s.updated[0].repo_id, q);
        assert_eq!(s.no_change_count, 1, "the no-op pull_ff is a no-change");
    }

    #[tokio::test]
    async fn a_repo_needing_attention_is_excluded_from_no_change() {
        // Codex review finding 1b: a repo checked today with no change but CURRENTLY
        // dirty (or errored) must surface in attention, not be reported as a calm
        // "no change". The buckets are disjoint - attention wins over no-change.
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let w = window();
        let noon = w.start_unix + DAY / 2;

        let p = seed_repo(&pool, "p").await; // clean check today, but currently dirty
        record_check(&pool, p, noon, "success").await;
        seed_state(&pool, p, None, 1).await;
        let q = seed_repo(&pool, "q").await; // clean check today, calm state
        record_check(&pool, q, noon, "success").await;

        let s = summary_today(&pool, &w).await.unwrap();
        assert_eq!(s.attention_count, 1);
        assert_eq!(s.attention[0].repo_id, p);
        assert_eq!(
            s.no_change_count, 1,
            "only q is a calm no-change; p is reported as attention, not double-counted"
        );
    }

    #[tokio::test]
    async fn inverted_day_window_is_rejected() {
        // Codex review finding 4: an inverted window (end <= start) is a caller bug (a
        // timezone / DST miscalculation at the edge); reject it rather than silently
        // labelling a summary for one day while counting another (or none).
        let tmp = TempDir::new().unwrap();
        let pool = fresh_pool(tmp.path()).await;
        let bad = DayWindow {
            date: "2026-06-29".into(),
            start_unix: 1001 * DAY,
            end_unix: 1000 * DAY,
        };
        assert!(
            summary_today(&pool, &bad).await.is_err(),
            "an inverted window must be rejected, not silently mislabelled"
        );
    }

    #[test]
    fn day_window_new_validates_bounds() {
        // Codex review finding 4: the validated constructor rejects an inverted /
        // empty window and an empty date label, so the edge cannot pass a malformed
        // window unnoticed.
        assert!(DayWindow::new("2026-06-29", 0, 10).is_ok());
        assert!(
            DayWindow::new("2026-06-29", 10, 10).is_err(),
            "end == start is empty/invalid"
        );
        assert!(
            DayWindow::new("2026-06-29", 10, 0).is_err(),
            "end < start is inverted"
        );
        assert!(
            DayWindow::new("", 0, 10).is_err(),
            "an empty date label is invalid"
        );
    }

    #[test]
    fn summary_week_is_inert_v1_1_stub() {
        // AC3: weekly is CUT to V1.1; the seam is callable and inert (echoes the
        // requested week_start, returns no days), never panicking.
        let wk = summary_week("2026-06-22");
        assert_eq!(wk.week_start, "2026-06-22");
        assert!(
            wk.days.is_empty(),
            "weekly aggregation is a V1.1 seam, inert in V1"
        );
    }
}
