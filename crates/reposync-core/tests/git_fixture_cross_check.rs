//! E-04 AC3 - the git2-vs-CLI cross-check (public-surface consumer smoke).
//!
//! Gated behind the `test-support` feature: this integration test imports the
//! harness exactly as E-07 (policy) and E-08 (scheduler) will - through the
//! crate's PUBLIC, feature-gated `git::fixtures` surface. Without the feature
//! the harness is not exposed, so this file compiles to nothing (keeping a plain
//! `--all-targets` build green). The in-crate `#[cfg(test)]` cross-check in
//! `git/fixtures.rs` covers the same agreement under a plain `cargo test`.
//!
//! It runs BOTH halves of the E-03 hybrid engine against each fabricated repo:
//!
//!   - the `git2` reads (`inspect` + `ahead_behind_read`, from `git/inspect.rs`);
//!   - the git CLI parsers (`rev-parse`, `status --porcelain=v2`, `for-each-ref`,
//!     `rev-list --left-right --count`, from `git/cli.rs`), via the public
//!     `SystemGitEngine` methods.
//!
//! and asserts the two engines AGREE on HEAD SHA, branch, dirty status, detached
//! state, and ahead/behind counts. This exercises four of E-03's five parsers;
//! `fetch` is deliberately EXCLUDED because it is a network/mutation op, not a
//! state read, and the cross-check compares reads of a fabricated LOCAL state,
//! never re-running network operations.
//!
//! For the no-upstream and deleted-upstream states, the two engines can
//! legitimately differ on raw ahead/behind, so E-03 states a provisional
//! contract (E-03 AC11): ahead/behind = `None` for both. This cross-check
//! encodes that `None` as the EXPECTED value (from the fixture's declared facts)
//! and asserts BOTH engines produce it - ratifying the contract rather than
//! deferring it. A divergence here is a signal to revisit E-03's spec, not to
//! silently edit E-03.
#![cfg(feature = "test-support")]

use reposync_core::git::fixtures::{build_fixture, FixtureState};
use reposync_core::git::{GitEngine, SystemGitEngine};

/// Whether the host has a usable git CLI (the cross-check needs it for both the
/// fabrication and the CLI-parser half).
fn git_resolvable() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Resolve HEAD's upstream ref shorthand for the CLI `rev-list` call, or `None`
/// when there is no comparison base (no-upstream / deleted-upstream / detached).
/// Uses the git2 read's `upstream_branch`, which is `None` precisely in those
/// cases - so the CLI ahead/behind is only attempted when a base exists.
fn upstream_ref(engine: &SystemGitEngine, working: &std::path::Path) -> Option<String> {
    engine.inspect(working).ok().and_then(|r| r.upstream_branch)
}

#[tokio::test]
async fn git2_and_cli_agree_across_all_states() {
    if !git_resolvable() {
        eprintln!("skipping git2_and_cli_agree_across_all_states: git not resolvable");
        return;
    }
    let engine = SystemGitEngine::discover().expect("git engine should discover on this host");

    for state in FixtureState::ALL {
        let fx = build_fixture(state);
        let working = fx.working_path();
        let label = state.name();

        // --- git2 reads (inspect.rs) ---------------------------------------
        let g2 = engine.inspect(working).expect("git2 inspect ok");
        let g2_ab = engine
            .ahead_behind_read(working)
            .expect("git2 ahead_behind_read ok");

        // --- CLI parsers (cli.rs), via the public engine -------------------
        // rev-parse HEAD (cli::rev_parse).
        let cli_head = engine
            .rev_parse(working, "HEAD")
            .await
            .expect("cli rev-parse ok");
        // status --porcelain=v2 (cli::status).
        let cli_status = engine.status(working).await.expect("cli status ok");
        // for-each-ref (cli::for_each_ref) - exercised + used for branch agreement.
        let cli_refs = engine
            .for_each_ref(working)
            .await
            .expect("cli for-each-ref ok");
        // rev-list --left-right --count (cli::ahead_behind), only when a
        // comparison base exists; otherwise the contract value is None/None.
        let cli_ab = match upstream_ref(&engine, working) {
            Some(upstream) => engine
                .ahead_behind(working, &upstream)
                .await
                .expect("cli ahead_behind ok"),
            None => reposync_core::git::AheadBehind {
                ahead: None,
                behind: None,
            },
        };

        // --- 1. HEAD SHA agreement -----------------------------------------
        assert_eq!(
            g2.head_sha.as_deref(),
            cli_head.as_deref(),
            "[{label}] HEAD SHA: git2 vs CLI disagree"
        );
        // And both agree with the fixture's declared fact.
        assert_eq!(
            g2.head_sha.as_deref(),
            Some(fx.expected.head_sha.as_str()),
            "[{label}] HEAD SHA: git2 disagrees with declared fact"
        );

        // --- 2. dirty agreement --------------------------------------------
        assert_eq!(
            g2.is_dirty,
            cli_status.is_dirty(),
            "[{label}] dirty: git2 vs CLI disagree"
        );
        assert_eq!(
            g2.is_dirty, fx.expected.dirty,
            "[{label}] dirty: git2 disagrees with declared fact"
        );

        // --- 3. detached agreement -----------------------------------------
        // The CLI detached signal: rev-parse --symbolic-full-name HEAD is empty /
        // not a branch ref when detached. We derive it from for-each-ref + the
        // git2 branch instead: a detached HEAD has no active branch.
        let cli_detached = cli_branch_is_detached(working);
        assert_eq!(
            g2.is_detached, cli_detached,
            "[{label}] detached: git2 vs CLI disagree"
        );
        assert_eq!(
            g2.is_detached, fx.expected.detached,
            "[{label}] detached: git2 disagrees with declared fact"
        );

        // --- 4. branch agreement -------------------------------------------
        // The CLI branch is read independently via `symbolic-ref` so the
        // cross-check compares two real readings, not one value twice.
        let cli_branch = cli_active_branch(working);
        assert_eq!(
            g2.active_branch, cli_branch,
            "[{label}] branch: git2 vs CLI disagree"
        );
        assert_eq!(
            g2.active_branch, fx.expected.branch,
            "[{label}] branch: git2 disagrees with declared fact"
        );

        // for-each-ref (cli::for_each_ref) is exercised as a state read: when on
        // a branch, the parsed rows must include HEAD's branch ref pointing at
        // the HEAD SHA. (A detached HEAD still has the branch ref present in the
        // ref store; the assertion below holds in both cases.)
        if let Some(branch) = g2.active_branch.as_deref() {
            let fq = format!("refs/heads/{branch}");
            let row = cli_refs
                .iter()
                .find(|r| r.refname == fq)
                .unwrap_or_else(|| panic!("[{label}] for-each-ref missing {fq}: {cli_refs:?}"));
            assert_eq!(
                Some(row.object_id.as_str()),
                g2.head_sha.as_deref(),
                "[{label}] for-each-ref branch SHA disagrees with HEAD SHA"
            );
        }

        // --- 5. ahead/behind agreement (the ratification) ------------------
        assert_eq!(
            g2_ab.ahead, cli_ab.ahead,
            "[{label}] ahead: git2 vs CLI disagree"
        );
        assert_eq!(
            g2_ab.behind, cli_ab.behind,
            "[{label}] behind: git2 vs CLI disagree"
        );
        // Ratify against the declared (E-03 AC11) contract: None for
        // no-upstream / deleted-upstream / detached; Some otherwise.
        let declared_ahead = fx.expected.ahead.map(|n| n as i64);
        let declared_behind = fx.expected.behind.map(|n| n as i64);
        assert_eq!(
            g2_ab.ahead, declared_ahead,
            "[{label}] ahead: engines disagree with the declared E-03 contract"
        );
        assert_eq!(
            g2_ab.behind, declared_behind,
            "[{label}] behind: engines disagree with the declared E-03 contract"
        );
    }
}

/// CLI detached probe via the git CLI: `symbolic-ref -q HEAD` exits non-zero
/// when HEAD is detached (it points at a SHA, not a branch ref).
fn cli_branch_is_detached(working: &std::path::Path) -> bool {
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(working)
        .args(["symbolic-ref", "-q", "HEAD"])
        .output()
        .expect("git symbolic-ref spawn");
    !status.status.success()
}

/// The active branch shorthand derived from the CLI side: `symbolic-ref --short
/// HEAD` when on a branch, `None` when detached. This is the CLI counterpart of
/// git2's `active_branch`, kept independent so the cross-check compares two real
/// readings rather than one value twice.
fn cli_active_branch(working: &std::path::Path) -> Option<String> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(working)
        .args(["symbolic-ref", "--short", "-q", "HEAD"])
        .output()
        .expect("git symbolic-ref spawn");
    if !out.status.success() {
        return None;
    }
    let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}
