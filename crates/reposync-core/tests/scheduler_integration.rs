//! E-08 integration: the scheduler driven through its REAL seams.
//!
//! Gated behind the `test-support` feature, this is the cross-check the E-08
//! implementation plan calls for: it builds a real migrated SQLite pool, adds two
//! E-04 fixture repos, and runs ONE scheduler tick through the production seams
//! (`DbDueQuery` -> `UpdateNowJobRunner` -> `DbOutcomeWriter`), so the whole
//! orchestration is validated end to end against real git states - not faked
//! collaborators. The unit grid in `scheduler::tests` pins the orchestration logic
//! against fakes; here the same flow runs through the live DB and git engine.
//!
//! A steady-state tick (not the startup pass) is used so NO real jitter sleep
//! runs: the tick is deterministic and fast.
#![cfg(feature = "test-support")]

use std::sync::Arc;

use reposync_core::db;
use reposync_core::git::fixtures::{build_fixture, FixtureState};
use reposync_core::git::SystemGitEngine;
use reposync_core::ipc::{BranchPolicy, DirtyHandling, UpdateMode, UpdatePolicy};
use reposync_core::repo;
use reposync_core::scheduler::{
    DbDueQuery, DbOutcomeWriter, Scheduler, SystemClock, SystemJitter, UpdateNowJobRunner,
};
use reposync_core::store;

/// Whether the host has a usable git CLI (needed to fabricate + drive fixtures).
fn git_resolvable() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[tokio::test]
async fn one_tick_checks_due_repos_then_next_check_gating_holds() {
    if !git_resolvable() {
        eprintln!("skipping one_tick_checks_due_repos...: git not resolvable");
        return;
    }
    let git = Arc::new(SystemGitEngine::discover().expect("git engine discovers on this host"));

    let dbtmp = tempfile::TempDir::new().expect("db tempdir");
    let pool = db::open_pool(&dbtmp.path().join("sched-it.db"))
        .await
        .expect("open_pool");
    db::run_migrations(&pool).await.expect("migrations");

    // Repo A: a clean, behind repo set to pull_ff_only - the one MUTATING cell, so
    // the scheduler drives a real fast-forward through update_now_scheduled.
    let behind = build_fixture(FixtureState::Behind);
    let behind_id = repo::add(&pool, &git, behind.working_path())
        .await
        .expect("add behind");
    store::repo_set_policy(
        &pool,
        behind_id,
        &UpdatePolicy {
            mode: UpdateMode::PullFfOnly,
            dirty_handling: DirtyHandling::Skip,
            branch_policy: BranchPolicy::DefaultBranchOnly,
        },
    )
    .await
    .expect("set pull_ff_only policy");

    // Repo B: a standalone no-upstream repo on the default fetch_only - a SKIP
    // path (no upstream to fetch), proving a skip is a success, not a failure.
    let no_up = build_fixture(FixtureState::NoUpstream);
    let no_up_id = repo::add(&pool, &git, no_up.working_path())
        .await
        .expect("add no-upstream");

    // Both repos have next_check_at = NULL after `add`, so both are due now.
    let scheduler = Scheduler::new(
        Arc::new(SystemClock::new()),
        Arc::new(SystemJitter::new()),
        DbDueQuery::new(pool.clone()),
        UpdateNowJobRunner::new(pool.clone(), git.clone()),
        DbOutcomeWriter::new(pool.clone()),
        4,
    );

    // One steady-state tick (no jitter -> no real sleep) runs BOTH due repos.
    let ran = scheduler.tick_once().await.expect("first tick");
    assert_eq!(ran, 2, "both newly-added repos are due on the first tick");

    // Repo A fast-forwarded: behind is now 0 and last_updated_at is set, and the
    // scheduler scheduled the next check + left it healthy and unpaused.
    let behind_after = store::repo_get(&pool, behind_id).await.expect("get behind");
    assert_eq!(
        behind_after.behind_count,
        Some(0),
        "the behind repo fast-forwarded to level with upstream"
    );
    assert!(
        behind_after.last_updated_at.is_some(),
        "a successful fast-forward sets last_updated_at"
    );
    assert!(
        behind_after.next_check_at.is_some(),
        "the scheduler scheduled the next check"
    );
    assert_eq!(
        behind_after.consecutive_failures, 0,
        "a successful check resets the failure counter"
    );
    assert!(
        !behind_after.auto_paused,
        "a healthy repo is not auto-paused"
    );

    // Repo B skipped (no upstream), which is a success: scheduled, not failed, not
    // paused.
    let no_up_after = store::repo_get(&pool, no_up_id)
        .await
        .expect("get no-upstream");
    assert!(
        no_up_after.next_check_at.is_some(),
        "a skipped check still schedules the next one"
    );
    assert_eq!(
        no_up_after.consecutive_failures, 0,
        "a no-upstream skip is a normal non-action, not a failure"
    );
    assert!(
        !no_up_after.auto_paused,
        "a skipped repo is not auto-paused"
    );

    // A SECOND immediate tick selects NOTHING: both repos now have a future
    // next_check_at, so the due-query (next_check_at <= now) excludes them. This is
    // the next_check_at gating proven end to end through the real DB.
    let ran_again = scheduler.tick_once().await.expect("second tick");
    assert_eq!(
        ran_again, 0,
        "next_check_at gating excludes repos until their next check is due"
    );
}
