//! E-07 AC2 - the decision-table matrix against the REAL E-04 fixtures.
//!
//! Gated behind the `test-support` feature, this integration test is the
//! cross-check the E-07 implementation plan calls for: it drives each of the 7
//! E-04 fixture states through the REAL E-03 inspect engine, builds the policy
//! engine's `RepoState` from those live reads, runs `decide` for each of the 3
//! V1 modes, and asserts the exact cell from the spec's grid. The grid is the
//! test matrix.
//!
//! This is distinct from the in-crate `policy::tests` unit grid (which pins the
//! same cells against hand-built `RepoState` values): here the `RepoState` comes
//! from running the actual git engine over a fabricated repo, so the engine's
//! state model is proven to stay in lockstep with what E-04 fabricates and E-03
//! reads. A divergence here means the engine's `RepoState` mapping drifted from
//! reality, not that a cell is wrong.
//!
//! The seven states map to grid rows as follows (matching `git::fixtures`):
//!   clean, dirty, ahead, behind, detached-head, deleted-upstream, no-upstream.
//! The diverged (ahead-and-behind) row is covered by the in-crate unit grid (the
//! E-04 harness fabricates no diverged fixture), so it is asserted there, not
//! here.
#![cfg(feature = "test-support")]

use std::path::Path;

use reposync_core::git::fixtures::{build_fixture, FixtureState};
use reposync_core::git::{GitEngine, SystemGitEngine};
use reposync_core::ipc::UpdateMode;
use reposync_core::policy::{decide, Action, PolicyDecision, RepoState, SkipReason, UpstreamState};

/// Whether the host has a usable git CLI (needed to fabricate + inspect).
fn git_resolvable() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Whether the working repo has an `origin` remote configured. This is what
/// distinguishes deleted-upstream (cloned, so `origin` exists, but the tracking
/// ref was pruned) from no-upstream (a standalone repo with no remote at all):
/// both report `upstream_branch = None` from inspect, so the remote presence is
/// the disambiguator the caller uses to pick the right `UpstreamState`.
fn has_origin_remote(working: &Path) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(working)
        .args(["remote", "get-url", "origin"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Build the policy engine's `RepoState` from the LIVE E-03 inspect reads of a
/// fabricated repo. This is the mapping a real caller (repo.rs / the scheduler)
/// performs: it classifies the upstream relationship from observable facts so
/// the engine never has to guess from a bare `None`/`None` ahead/behind.
fn repo_state_from_reads(engine: &SystemGitEngine, working: &Path) -> RepoState {
    let inspect = engine.inspect(working).expect("git2 inspect ok");
    let ab = engine
        .ahead_behind_read(working)
        .expect("git2 ahead_behind_read ok");

    // Classify the upstream relationship:
    //   - inspect reports an upstream branch  -> Tracking (a real comparison);
    //   - no upstream branch, but an origin remote exists -> Deleted (the
    //     tracking ref was pruned but config/remote remain);
    //   - no upstream branch and no origin remote -> None (standalone repo).
    // A detached HEAD carries `None` here (its branch-less HEAD has no upstream),
    // and the engine keys its detached handling off the `is_detached` flag, not
    // off this classification.
    let upstream = if inspect.upstream_branch.is_some() {
        UpstreamState::Tracking
    } else if !inspect.is_detached && has_origin_remote(working) {
        UpstreamState::Deleted
    } else {
        UpstreamState::None
    };

    RepoState::new(
        inspect.is_dirty,
        inspect.is_detached,
        upstream,
        ab.ahead,
        ab.behind,
    )
}

/// The expected decision for a `(state, mode)` cell, taken verbatim from the
/// spec's grid. Returns `None` for a cell that this fixture-driven matrix does
/// not assert (there are none - every fixture cell is pinned).
fn expected_cell(state: FixtureState, mode: &UpdateMode) -> PolicyDecision {
    use Action::*;
    use FixtureState::*;
    use PolicyDecision::{Act, Skip};
    use UpdateMode::*;

    match (state, mode) {
        // check_only: report status for every state, no fetch, no mutation.
        (_, CheckOnly) => Act(ReportStatus),

        // fetch_only: fetch where an upstream/remote resolves; skip for the two
        // upstream-config states. Detached still fetches (it has remotes).
        (Clean | Dirty | Ahead | Behind | DetachedHead, FetchOnly) => Act(Fetch),
        (DeletedUpstream, FetchOnly) => Skip(SkipReason::DeletedUpstream),
        (NoUpstream, FetchOnly) => Skip(SkipReason::NoUpstream),

        // pull_ff_only: the only mutating mode.
        (Clean, PullFfOnly) => Act(ReportStatus), // level: up to date, no merge.
        (Dirty, PullFfOnly) => Skip(SkipReason::Dirty),
        (Ahead, PullFfOnly) => Act(ReportStatus), // no remote commits to ff.
        (Behind, PullFfOnly) => Act(PullFastForward), // the one mutating cell.
        (DetachedHead, PullFfOnly) => Skip(SkipReason::DetachedHead),
        (DeletedUpstream, PullFfOnly) => Skip(SkipReason::DeletedUpstream),
        (NoUpstream, PullFfOnly) => Skip(SkipReason::NoUpstream),

        // This matrix only drives the three V1 modes; the non-V1 modes
        // (pull_standard / pull_rebase) are the closed-enum guard's job and are
        // asserted in the in-crate unit grid, never here.
        (_, PullStandard | PullRebase) => {
            unreachable!("the fixture matrix only exercises the three V1 modes")
        }
    }
}

#[tokio::test]
async fn decision_grid_holds_against_real_fixtures() {
    if !git_resolvable() {
        eprintln!("skipping decision_grid_holds_against_real_fixtures: git not resolvable");
        return;
    }
    let engine = SystemGitEngine::discover().expect("git engine should discover on this host");

    let modes = [
        UpdateMode::CheckOnly,
        UpdateMode::FetchOnly,
        UpdateMode::PullFfOnly,
    ];

    let mut asserted_cells = 0usize;
    for state in FixtureState::ALL {
        let fx = build_fixture(state);
        let working = fx.working_path();
        let repo_state = repo_state_from_reads(&engine, working);

        for mode in &modes {
            let got = decide(&repo_state, mode);
            let want = expected_cell(state, mode);
            assert_eq!(
                got,
                want,
                "[{} x {:?}] decision grid cell mismatch (repo_state = {:?})",
                state.name(),
                mode,
                repo_state
            );
            asserted_cells += 1;
        }
    }

    // 7 fixture states x 3 V1 modes = 21 cells, all driven through the real
    // engine, none skipped.
    assert_eq!(
        asserted_cells, 21,
        "the fixture-driven matrix must cover all 7x3 cells"
    );
}
