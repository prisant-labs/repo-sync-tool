---
effort: E-04
tracking-issue: 6
title: Git Fixture Test Harness
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-03]
source: docs/internal/v1-architecture-and-decisions.md (Section 6 "What we can build now" and Section 4 / Architecture subsection 6 "Git engine")
---

# E-04 - Git Fixture Test Harness

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** built and green. The harness ships as `git::fixtures` in `reposync-core`, gated behind `#[cfg(any(test, feature = "test-support"))]`, with the selector enum, `build_fixture(state) -> Fixture` entry point, named-field `ExpectedFacts`, and Drop-based tempdir cleanup. All six ACs are implemented and verified on this Windows host (git 2.40.1).
- **Review fixes (2026-06 code review, applied test-first):**
  - **M3 (deleted-upstream fixture not fetch-stable):** `RepoBuilder::delete_tracking_ref` now deletes the branch on the BARE side (`update-ref -d refs/heads/main`, handling the bare-HEAD constraint) and prunes the working clone, so the upstream is genuinely gone and a later `git fetch --all --prune` does NOT resurrect `refs/remotes/origin/main`. The fixture keeps modelling deleted-upstream for downstream fetch/check tests (E-08). The branch's `branch.<name>.merge`/`.remote` config is left intact (config still names a now-unresolvable upstream). Test: `deleted_upstream_survives_fetch` (asserts a fetch does not resurrect the upstream).
  - **M4 (no-upstream `bare_path` lied):** `Fixture.bare_path` is now `Option<PathBuf>` and `bare_path()` returns `Option<&Path>` - `None` for the standalone no-upstream state, `Some(bare)` for every paired state. It no longer falls back to the working path (which contradicted the "bare + working pair" API). Public-API change, done now since E-07/E-08 are not built yet. The fixture-API description in AC6 / Contract item 6 / In-scope is updated to match. Test: `bare_path_is_none_only_for_no_upstream`.
  - **L1 (determinism test was tautological):** `recipes_are_structurally_deterministic` was comparing each build's hand-declared `ExpectedFacts` to itself, proving nothing. Rewritten to run the real E-03 engine (`inspect` + `ahead_behind_read` + `for-each-ref`) against build A and build B and assert the OBSERVED branch / dirty / detached / ahead / behind AND the ref topology match (object ids dropped; SHA stability remains the separate `recipe_clean_sha_is_reproducible` check). Verified the rewrite catches a deliberately non-deterministic recipe.
  - Post-fix gate green: 91 reposync-core lib tests (was 79) + the feature-gated cross-check (`--features test-support`); clippy `-D warnings` clean; fmt clean; dependency-hygiene tree empty (no tauri/openssl/libssh2).
- **Next:** E-07 (update-policy engine) and E-08 (scheduler) consume the harness via `features = ["test-support"]` and `build_fixture(FixtureState::*)`; no further E-04 work is required for them to start. NOTE for E-08: `Fixture::bare_path()` is now `Option<&Path>` (None only for `NoUpstream`).
- **Blockers:** none.

### What landed

- `crates/reposync-core/src/git/fixtures.rs` - the builder primitive (`RepoBuilder`), the seven state recipes, the public API (`FixtureState`, `Fixture`, `ExpectedFacts`, `build_fixture`), and the in-crate test suite (recipe self-validation against the E-03 git2 engine, the parameterized git2-vs-CLI cross-check, the isolation/cleanup check, and the structural-determinism + bonus SHA-reproducibility checks).
- `crates/reposync-core/tests/git_fixture_cross_check.rs` - the same cross-check through the crate's PUBLIC feature-gated surface, proving the E-07/E-08 consumption path. Gated behind `#![cfg(feature = "test-support")]` so a plain `--all-targets` build stays green.
- `crates/reposync-core/Cargo.toml` - `tempfile` made an optional dependency plus a `test-support` feature that enables it; production builds never compile the harness.
- `crates/reposync-core/src/git/mod.rs` - registers the gated `fixtures` module.

### Gate outcomes (Windows, git 2.40.1, Rust 1.96.0)

- `cargo check --workspace --all-targets`: clean.
- `cargo clippy --all --all-targets -- -D warnings`: clean (also clean with `--features test-support`).
- `cargo test --workspace`: green; `reposync-core` lib went from 67 to 79 tests (12 new). With `--features test-support` the integration cross-check (`tests/git_fixture_cross_check.rs`) also runs and passes.
- `cargo fmt --all -- --check`: clean.
- `cargo tree -p reposync-core` (with and without `test-support`) for `tauri|openssl|libssh2`: EMPTY. git2 stays `vendored-libgit2`; the harness shells out to the system `git` CLI and adds no network transport.

### Cross-check ratification outcome (AC3)

- **E-03's `None` contract HELD.** For `no-upstream`, `deleted-upstream`, AND `detached-head`, both the git2 read (`inspect::ahead_behind`) and the CLI path report ahead/behind = `None` (no comparison base), distinct from `Some(0)`. The cross-check encodes `None` as the expected value (from each fixture's declared `ExpectedFacts`) and both engines produce it.
- **No git2-vs-CLI disagreement was found** on HEAD SHA, branch, dirty, detached, or ahead/behind across all seven states. The two engines agree everywhere the cross-check compares them.
- **E-03 source was NOT modified.** No real E-03 bug surfaced; the provisional contract is ratified as written.
- Note on cross-check construction: the CLI ahead/behind is only invoked when a comparison base exists (HEAD has a resolvable upstream, read via git2's `upstream_branch`); otherwise the contract value `None`/`None` is used directly. This mirrors how the real engine behaves and avoids feeding `rev-list` a `HEAD...<deleted-ref>` range that git would reject. The `ahead`/`behind` states still exercise the live `rev-list` parser and assert `Some(2)` / `Some(1)`, so the CLI parser is genuinely covered, not bypassed.

### CI git pin chosen (AC5, for E-01 to implement)

- **Pinned version: git `2.40.1` on the Windows runner** (the fixture suite's only execution target). Rationale: `status --porcelain=v2`, `for-each-ref`, `rev-list --left-right --count`, and `rev-parse` are stable from the >= 2.30 floor, but pinning one exact version keeps porcelain output byte-stable run-to-run so the cross-check never flakes on a silent runner git bump. `2.40.1` is what this effort developed and verified against.
- macOS is "compiles + bundles in CI" only per the brief and is NOT a test-execution target, so it carries no fidelity-derived git pin. Running the suite on macOS, if ever wanted, is a separate decision for jp.
- The robust exact-pin MECHANISM (three approaches already failed in E-01) is tracked by **BL-NI-03 (robust exact git pin on CI)** in `docs/backlog.md`. Until that lands, CI verifies the >= 2.30 floor only; this effort defines the exact target version the mechanism should install.

## Context

The brief calls the fixture harness "the highest-leverage early investment" and "the single biggest testability multiplier." It makes the entire git engine, the policy engine, and the error taxonomy testable deterministically with no UI, no network, and no real personal repos. This effort delivers that harness.

The harness programmatically creates **bare + working repo pairs in tempdirs**, each fabricated into a known state, then runs **both** halves of the E-03 hybrid (the `git2` reads and the CLI parsers) against each state and asserts they agree. That cross-check is the mechanism the brief names for catching libgit2-vs-CLI drift early, while the codebase is small. The seven states are fixed by the brief: clean, dirty, ahead, behind, detached HEAD, deleted-upstream, and no-upstream. The harness builds all seven deterministically so every run produces byte-identical structure.

This is foundational tooling, not product code. Its consumers are downstream efforts: E-07 (update-policy engine) is "exhaustively unit-tested against the fixture states," and E-08 (scheduler) reuses them too. Getting this right now means every later git-touching effort tests against a stable, shared bedrock instead of inventing its own repos. The matching CI requirement - pinning the git version - keeps porcelain output stable across runners so the cross-check does not flake.

## In scope

- A fixture builder that creates **bare + working repo pairs** in tempdirs and fabricates each of the seven known states deterministically: **clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream**.
- Deterministic construction: fixed author/committer identity, fixed commit messages, and controlled timestamps so structure (and where feasible SHAs) is reproducible across runs and runners.
- Automatic tempdir lifecycle (created per test, cleaned up after) so runs leave no residue and never touch a real user repo.
- A **git2-vs-CLI cross-check** that, for each of the seven states, runs both the `inspect.rs` (git2) reads and the `cli.rs` parsers (`status --porcelain=v2`, `for-each-ref`, `rev-list --left-right --count`, `rev-parse`) and asserts agreement on HEAD SHA, branch, dirty status, detached state, and ahead/behind counts. The cross-check exercises four of E-03's five parsers; E-03's `fetch` parser is deliberately EXCLUDED because `fetch` is a network/mutation operation, not a state read, and the harness compares the two engines' reads of a fabricated local state rather than re-running network operations.
- The **CI git-pinning requirement**: a pinned `git` version (>= the E-03 2.30 floor) on the **Windows runner**, which is where the fixture suite actually executes, so porcelain output is stable across runs. The brief's macOS posture is "compiles + bundles in CI" only (Section 4 / Architecture subsection 2 and Section 2's acceptance rewrite), so macOS is NOT a test-execution target and gets no fidelity-derived git pin here. If running the fixture suite on macOS is ever wanted, that is a separate decision for jp, not a requirement this effort derives. (E-01 owns the workflow file; this effort owns the requirement and its rationale.)
- A reusable, concretely specified fixture API that E-07 and E-08 import to obtain a repo in any of the seven states. The API shape (this effort's payoff for downstream consumers):
  - A **selector enum** over the seven states (one variant each for `Clean`, `Dirty`, `Ahead`, `Behind`, `DetachedHead`, `DeletedUpstream`, `NoUpstream`), so callers pick a state by name rather than by string.
  - A single builder entry point (for example `build_fixture(state) -> Fixture`) that returns a **struct** carrying: the bare repo path (as `Option<&Path>` - `None` for the standalone no-upstream state, which has no bare upstream; `Some(bare)` for every paired state) and the working repo path, an **expected-facts struct** with named fields (for example `branch: Option<String>`, `head_sha: String`, `dirty: bool`, `detached: bool`, `ahead: Option<u32>`, `behind: Option<u32>`), and the owned tempdir handle.
  - A **tempdir-handle Drop cleanup contract**: cleanup happens when the returned handle is dropped, so the consumer MUST hold the `Fixture` (or its tempdir handle) for the lifetime of the test. The repo paths borrow from that handle and become invalid once it drops.

## Out of scope

- The git engine itself: the trait, the parsers, the git2 reads, discovery, version probing (all E-03).
- The update-policy engine that is tested against these fixtures (E-07).
- The scheduler that reuses these fixtures (E-08).
- The `AppError` taxonomy (E-05); the harness asserts engine agreement on state, not error mapping.
- Authoring the CI workflow YAML (E-01); this effort specifies the git-pinning requirement that workflow must satisfy.

## Contract / deliverables

1. A fixture builder API that returns a ready bare + working repo pair in a tempdir for any named state of the seven.
2. All seven states - clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream - built deterministically and verified to match their intended definitions.
3. A cross-check test that runs both engines against each state and fails on any disagreement in HEAD SHA, branch, dirty status, detached state, or ahead/behind counts.
4. Tempdir lifecycle that guarantees isolation and cleanup; no test touches anything outside its tempdir.
5. A stated CI requirement to pin git (>= 2.30) on the Windows runner (where the suite executes), with rationale (porcelain stability), ready for E-01's workflow to consume. macOS is build/bundle-only per the brief and is not a test-execution target, so it carries no fidelity-derived git pin here.
6. A concrete fixture API: a selector enum over the seven states, a builder entry point returning a `Fixture` struct (the working repo path; the bare repo path as `Option<&Path>`, `None` for the standalone no-upstream state and `Some(bare)` for the paired states; a named-field expected-facts struct; and the owned tempdir handle), and a documented Drop-based cleanup contract the consumer holds. E-07 and E-08 consume this without re-reading the builder internals.

## Acceptance criteria

- [x] AC1: The harness builds programmatic bare + working repo pairs in tempdirs for all seven states (clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream). Source: brief Section 6 (Git fixture TEST HARNESS row). **Done:** `RepoBuilder` + seven recipes in `git::fixtures`; each recipe is self-validated against the E-03 git2 engine (`recipe_*_matches_engine` tests).
- [x] AC2: Each state is constructed deterministically (fixed identity, messages, controlled timestamps) so runs are reproducible across runners. "Reproducible" here asserts that the branch topology, ref names, and ahead/behind relationships are identical across runs and runners; it does NOT assert exact commit SHAs (treated as a nice-to-have per the Open question below), so the determinism check diffs structure and relationships, not SHAs. Source: brief Section 6 ("makes the entire git engine ... testable deterministically"). **Done:** fixed identity (`RepoSync Fixture <fixture@reposync.test>`), fixed messages, controlled `GIT_AUTHOR_DATE`/`GIT_COMMITTER_DATE` from a fixed epoch; `recipes_are_structurally_deterministic` diffs structure across two builds, and a bonus `recipe_clean_sha_is_reproducible` shows the SHAs are in fact stable too.
- [x] AC3: A git2-vs-CLI cross-check runs both engines against each state and asserts agreement on HEAD SHA, branch, dirty status, detached state, and ahead/behind counts, exercising four of E-03's five parsers (`fetch` is excluded as a network/mutation op, not a state read). For the no-upstream and deleted-upstream states, the cross-check ratifies E-03's now-provisional ahead/behind contract (ahead/behind = `None`, per E-03's AC11) as the expected value rather than deferring the decision; a divergence is flagged back to E-03's spec. Source: brief Section 4 / Architecture subsection 6 (Git engine) and Section 6 (`inspect.rs` row: "Cross-checked against CLI output in the fixture harness to catch libgit2-vs-CLI drift early"). **Done + ratified:** `git2_and_cli_agree_across_all_states` (in-crate, plus the feature-gated integration mirror). The `None` contract held for no-upstream, deleted-upstream, and detached; no disagreement was found; E-03 source was not touched. See the Task Summary ratification block.
- [x] AC4: Tempdirs are isolated per test and cleaned up; no fixture touches a real user repo, the network, or any path outside its tempdir. Source: brief Section 6 (deterministic, "without any UI, network, or real repos"). **Done:** every repo lives under an owned `TempDir`; `fixture_is_isolated_and_cleans_up` asserts the bare + working paths start inside the tempdir and that dropping the `Fixture` removes the directory. All git is local (bare + working clone on disk); no network transport is linked (`cargo tree` hygiene EMPTY).
- [x] AC5: The effort states the CI requirement to pin git (>= 2.30) on the Windows runner, where the fixture suite executes, with the porcelain-stability rationale, for E-01's workflow to implement. macOS is "compiles + bundles in CI" only and is not a test-execution target (brief Section 4 / Architecture subsection 2 and Section 2's per-platform acceptance rewrite), so it carries no fidelity-derived git pin; running the suite on macOS, if ever wanted, is a separate decision for jp. Source: brief Section 6 ("Plus pinned git in CI ... keeps porcelain output stable across runners") and the macOS-posture sections above. **Done:** pinned version recorded as git `2.40.1` on the Windows runner (see Task Summary "CI git pin chosen"); mechanism tracked by BL-NI-03.
- [x] AC6: The fixture API is reusable and concrete so E-07 (policy) and E-08 (scheduler) consume the same states: a selector enum over the seven states, a builder entry point returning a `Fixture` struct (the working repo path; the bare repo path as `Option<&Path>`; a named-field expected-facts struct; and the owned tempdir handle), and a documented Drop-based cleanup contract the consumer must hold for the test's lifetime. Source: brief Section 6 (policy engine "exhaustively unit-tested against the fixture states") and the dependency graph (E-04 -> E-07, E-04 -> E-08). **Done:** `FixtureState` (7 variants + `ALL` + `name`), `build_fixture(state) -> Fixture`, `Fixture { tempdir, bare_path: Option<PathBuf>, working_path, expected: ExpectedFacts }` with `bare_path()` returning `Option<&Path>` (`None` only for `NoUpstream`; see review fix M4) and `ExpectedFacts { branch, head_sha, dirty, detached, ahead, behind }`, Drop cleanup, all module-documented. Consumed via `features = ["test-support"]`; the integration test exercises that exact path.

## Dependencies

- Upstream: E-03 (the `GitEngine` trait, `inspect.rs` reads, and `cli.rs` parsers the cross-check exercises).
- Downstream: E-07 (policy engine, exhaustively tested against these states), E-08 (scheduler, reuses the fixtures). E-12 may also lean on the builder for a real-repo tracer assertion.

## V1.1 extension points

- Additional fabricated states as new policy modes arrive (for example, diverged/conflicting upstream, multiple-remotes, shallow clones).
- A property-based or fuzz layer over the parsers, seeded from the deterministic fixtures, to widen the cross-check beyond the seven hand-built states.
- Reuse of the same builder in a CI smoke that bundles and runs the engine on a real Windows build (feeds E-12's packaging spike).

## Open questions

- ~~Whether SHAs should be fully pinned~~ **Resolved as built:** the harness pins identity + messages and controls timestamps, asserts structure + relationships (`recipes_are_structurally_deterministic`), and - as a bonus - the fixed inputs DO yield stable SHAs run-to-run (`recipe_clean_sha_is_reproducible` asserts it). The contract requires only structural determinism; the SHA stability is a verified nice-to-have, not a promise. If a downstream effort needs exact pinned SHAs as a contract, flag to jp.
- ~~Exact agreed semantics for ahead/behind in no-upstream / deleted-upstream~~ **Resolved + ratified:** the cross-check encodes E-03's provisional `None` contract (E-03 AC11) as the expected value and BOTH engines produce it for no-upstream, deleted-upstream, and detached HEAD. No divergence was found; the provisional contract holds and is ratified. Nothing to flag to jp.

## Follow-ups / notes for the orchestrator

- The harness is exposed via a `test-support` cargo feature (optional `tempfile`). Downstream test trees enable it with `reposync-core = { features = ["test-support"] }` (or `--features test-support` when testing the crate directly). The crate's own `#[cfg(test)]` tree gets the harness for free. Production builds never compile it.
- `cargo test --workspace` runs the in-crate cross-check (covers AC3 fully). The `tests/git_fixture_cross_check.rs` integration mirror only runs when the `test-support` feature is enabled; if CI wants it executed, add `--features test-support` to the core crate's test step. No new backlog item is strictly required (the in-crate cross-check already gates AC3), but the orchestrator may want CI to run the feature-gated form too.
- No new backlog items were created. BL-NI-03 (robust exact git pin on CI) already tracks the pin mechanism this effort feeds with the exact version (`2.40.1`).
