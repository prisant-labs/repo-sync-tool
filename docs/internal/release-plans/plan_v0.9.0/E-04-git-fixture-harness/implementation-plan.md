---
effort: E-04
plan_for: spec.md
status: ready
---

# E-04 Implementation Plan

## Approach

This is the highest-leverage early testability investment in the whole build, and the plan treats it that way. Get a deterministic, reusable fixture bedrock in place before any breadth of git-touching logic exists, so E-07 (policy) and E-08 (scheduler) consume a stable, shared set of known states instead of each inventing repos. Build one small, composable primitive - "make a bare + working pair and drive it into commits/branches/states" - then express all seven states as recipes on top of it. Wire the git2-vs-CLI cross-check as a single parameterized test over the seven states so adding a state automatically extends the cross-check. Pin git in CI early so porcelain output is stable from the first run.

Build order inside the effort: the builder primitive, then the seven state recipes, then the cross-check, then the documented reusable API, then the CI pinning requirement.

## Steps

1. **Tempdir + builder primitive.** A helper that creates an isolated tempdir, initializes a **bare** repo and a **working** clone of it, and exposes small operations (commit on working, push to bare, create/move branches, detach HEAD, delete the upstream ref) used to fabricate states. Fix author/committer identity and commit messages, and control timestamps, so construction is deterministic. Guarantee cleanup on drop so nothing leaks.
2. **State recipes (all seven).** Express each state as a recipe over the primitive:
   - **clean** - working matches upstream, no local changes.
   - **dirty** - uncommitted working-tree modifications.
   - **ahead** - local commits not pushed to the bare upstream.
   - **behind** - bare upstream has commits the working clone lacks.
   - **detached HEAD** - HEAD checked out to a commit, not a branch.
   - **deleted-upstream** - the tracking branch's upstream ref removed from the bare repo.
   - **no-upstream** - a local branch with no configured upstream.
   Each recipe returns the ready pair plus the state's intended facts in the named-field expected-facts struct (expected branch, dirtiness, detached flag, ahead/behind), so tests assert against the recipe's own declared truth. For no-upstream and deleted-upstream, the expected ahead/behind is `None`, matching E-03's provisional contract (E-03 AC11), not `(0, 0)`.
3. **The git2-vs-CLI cross-check.** A single parameterized test that, for each of the seven states, runs E-03's `inspect.rs` (git2) reads and E-03's `cli.rs` parsers (`status --porcelain=v2`, `for-each-ref`, `rev-list --left-right --count`, `rev-parse`) against the same repo and asserts the two engines agree on HEAD SHA, branch, dirty status, detached state, and ahead/behind. This exercises four of E-03's five parsers: `fetch` is deliberately excluded because it is a network/mutation op, not a state read, and the cross-check compares reads of a fabricated local state, never re-running network operations. This is the brief's named mechanism for catching libgit2-vs-CLI drift early (Section 4 / Architecture subsection 6, "Git engine"). For the no-upstream / deleted-upstream states, encode E-03's now-provisional contract (ahead/behind = `None`, E-03 AC11) as the expected value and ratify it, flagging back to E-03's spec only if the cross-check shows it cannot hold.
4. **Reusable, documented fixture API.** Promote the builder into a clean, documented test-support surface (a `dev-dependency`-grade module or a `#[cfg(test)]` support crate/module) that E-07 and E-08 import to obtain a repo in any of the seven states with one call. The concrete shape (this is the effort's payoff for E-07/E-08, so specify it, do not hand-wave "documented well enough"): a **selector enum** with one variant per state (`Clean`, `Dirty`, `Ahead`, `Behind`, `DetachedHead`, `DeletedUpstream`, `NoUpstream`); a builder entry point (for example `build_fixture(state) -> Fixture`) returning a `Fixture` struct that carries the bare repo path, the working repo path, an **expected-facts struct** with named fields (`branch: Option<String>`, `head_sha: String`, `dirty: bool`, `detached: bool`, `ahead: Option<u32>`, `behind: Option<u32>`), and the owned tempdir handle; and a **Drop-based cleanup contract** where dropping the `Fixture` (or its tempdir handle) removes the repos, so the consumer must hold it for the test's lifetime and the repo paths are valid only while it lives. Document each state's contract so downstream efforts do not re-read the builder internals. This reuse is the payoff that justifies the early investment.
5. **CI git pinning requirement.** Specify (for E-01's workflow to implement) that the **Windows runner** - where the fixture suite executes - installs a pinned `git` at or above the E-03 2.30 floor, with the rationale that pinned git keeps `status --porcelain=v2` / `for-each-ref` / `rev-list` output stable so the cross-check does not flake across runs. macOS is "compiles + bundles in CI" only per the brief and is not a test-execution target, so no fidelity-derived git pin is required there; running the suite on macOS, if ever wanted, is a separate decision for jp rather than a requirement this effort derives. Record the exact pinned version chosen.
6. **Verify.** Run the full fixture suite and the cross-check locally on Windows; confirm determinism by running twice and diffing structure; confirm the suite is green in CI on the pinned git.

## Test strategy

- **The harness is itself the test layer**, so "test strategy" here is largely "the harness validates itself": each state recipe asserts the engines report the state's declared facts, and the cross-check asserts both engines agree.
- **Determinism check:** run the builder twice and assert identical structure (and SHAs, if pinned per the spec's open question) to prove reproducibility across runs and runners (AC2).
- **Isolation check:** assert each fixture lives entirely within its tempdir and that cleanup removes it; no network, no real user repo (AC4).
- **Cross-check coverage:** parameterized over all seven states so coverage is total by construction; adding a state in step 2 automatically extends the cross-check in step 3.
- **Downstream reuse is the real validation:** E-07 and E-08 importing this API and testing exhaustively against the same states is the proof the investment paid off. Keep the API stable so those efforts are not forced to fork it.

## Files / modules touched

- Fixture builder + state recipes as a test-support module under `crates/reposync-core` (a `#[cfg(test)]` module or a small internal test-support surface, exposed so E-07/E-08 can import it).
- The git2-vs-CLI cross-check test (integration test in the crate's test tree, exercising E-03's `git/inspect.rs` and `git/cli.rs`).
- A short fixture-API doc (module docs or a README alongside the harness) enumerating the seven states and their contracts for downstream consumers.
- Coordination note for E-01's `.github/workflows/ci.yml`: the pinned-git step and chosen version (this effort specifies; E-01 implements).

## Risks and mitigations

- **Porcelain output drift across git versions makes the cross-check flake.** Mitigated by pinning git in CI (step 5) and the E-03 2.30 floor; parser fixtures and the cross-check both assume the pinned version.
- **Legitimate git2-vs-CLI disagreement on no-upstream / deleted-upstream** read as a harness bug. Mitigated by encoding the agreed expectation explicitly per state and flagging the semantics back to E-03's spec rather than forcing false agreement.
- **Non-determinism from timestamps or environment** breaking reproducibility. Mitigated by fixing identity, messages, and controlling timestamps; the determinism check (run-twice-and-diff) catches regressions.
- **API churn forcing E-07/E-08 to fork the harness.** Mitigated by freezing and documenting the fixture API in this effort (step 4) before those efforts start, since they depend on E-04.
- **Cross-platform tempdir/path quirks (Windows long paths, line endings).** Mitigated by running the suite on the Windows runner from day one and normalizing line endings in the working repos.

## Definition of done

All six acceptance criteria checked: all seven states built deterministically as reusable bare + working pairs, the parameterized git2-vs-CLI cross-check green across every state (four parsers exercised; `fetch` excluded as a network/mutation op), the no-upstream/deleted-upstream ahead/behind ratified against E-03's provisional `None` contract, tempdir isolation and cleanup proven, the CI git-pinning requirement specified for E-01 on the Windows runner (the suite's execution target; macOS is build/bundle-only and not pinned for test fidelity), and the concrete fixture API (selector enum, `Fixture` struct with named expected-facts fields, Drop cleanup contract) documented for E-07 and E-08. The suite is green on the pinned git on the Windows runner, and the branch is ready for self-merge per the visibility-tiered policy in `EXECUTION.md`. The durable output of this effort is the shared, stable fixture bedrock that E-07 and E-08 will build their exhaustive tests on.
