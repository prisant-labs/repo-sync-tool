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

- **State:** not started.
- **Next:** build the fixture builder that fabricates a bare + working repo pair into the first known state (clean) in a tempdir.
- **Blockers:** none beyond E-03 (the `GitEngine` trait, `inspect.rs` reads, and `cli.rs` parsers must exist to be cross-checked).

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
  - A single builder entry point (for example `build_fixture(state) -> Fixture`) that returns a **struct** carrying: the bare repo path and the working repo path, an **expected-facts struct** with named fields (for example `branch: Option<String>`, `head_sha: String`, `dirty: bool`, `detached: bool`, `ahead: Option<u32>`, `behind: Option<u32>`), and the owned tempdir handle.
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
6. A concrete fixture API: a selector enum over the seven states, a builder entry point returning a `Fixture` struct (bare + working repo paths, a named-field expected-facts struct, and the owned tempdir handle), and a documented Drop-based cleanup contract the consumer holds. E-07 and E-08 consume this without re-reading the builder internals.

## Acceptance criteria

- [ ] AC1: The harness builds programmatic bare + working repo pairs in tempdirs for all seven states (clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream). Source: brief Section 6 (Git fixture TEST HARNESS row).
- [ ] AC2: Each state is constructed deterministically (fixed identity, messages, controlled timestamps) so runs are reproducible across runners. "Reproducible" here asserts that the branch topology, ref names, and ahead/behind relationships are identical across runs and runners; it does NOT assert exact commit SHAs (treated as a nice-to-have per the Open question below), so the determinism check diffs structure and relationships, not SHAs. Source: brief Section 6 ("makes the entire git engine ... testable deterministically").
- [ ] AC3: A git2-vs-CLI cross-check runs both engines against each state and asserts agreement on HEAD SHA, branch, dirty status, detached state, and ahead/behind counts, exercising four of E-03's five parsers (`fetch` is excluded as a network/mutation op, not a state read). For the no-upstream and deleted-upstream states, the cross-check ratifies E-03's now-provisional ahead/behind contract (ahead/behind = `None`, per E-03's AC11) as the expected value rather than deferring the decision; a divergence is flagged back to E-03's spec. Source: brief Section 4 / Architecture subsection 6 (Git engine) and Section 6 (`inspect.rs` row: "Cross-checked against CLI output in the fixture harness to catch libgit2-vs-CLI drift early").
- [ ] AC4: Tempdirs are isolated per test and cleaned up; no fixture touches a real user repo, the network, or any path outside its tempdir. Source: brief Section 6 (deterministic, "without any UI, network, or real repos").
- [ ] AC5: The effort states the CI requirement to pin git (>= 2.30) on the Windows runner, where the fixture suite executes, with the porcelain-stability rationale, for E-01's workflow to implement. macOS is "compiles + bundles in CI" only and is not a test-execution target (brief Section 4 / Architecture subsection 2 and Section 2's per-platform acceptance rewrite), so it carries no fidelity-derived git pin; running the suite on macOS, if ever wanted, is a separate decision for jp. Source: brief Section 6 ("Plus pinned git in CI ... keeps porcelain output stable across runners") and the macOS-posture sections above.
- [ ] AC6: The fixture API is reusable and concrete so E-07 (policy) and E-08 (scheduler) consume the same states: a selector enum over the seven states, a builder entry point returning a `Fixture` struct (bare + working repo paths, a named-field expected-facts struct, and the owned tempdir handle), and a documented Drop-based cleanup contract the consumer must hold for the test's lifetime. Source: brief Section 6 (policy engine "exhaustively unit-tested against the fixture states") and the dependency graph (E-04 -> E-07, E-04 -> E-08).

## Dependencies

- Upstream: E-03 (the `GitEngine` trait, `inspect.rs` reads, and `cli.rs` parsers the cross-check exercises).
- Downstream: E-07 (policy engine, exhaustively tested against these states), E-08 (scheduler, reuses the fixtures). E-12 may also lean on the builder for a real-repo tracer assertion.

## V1.1 extension points

- Additional fabricated states as new policy modes arrive (for example, diverged/conflicting upstream, multiple-remotes, shallow clones).
- A property-based or fuzz layer over the parsers, seeded from the deterministic fixtures, to widen the cross-check beyond the seven hand-built states.
- Reuse of the same builder in a CI smoke that bundles and runs the engine on a real Windows build (feeds E-12's packaging spike).

## Open questions

- Whether SHAs should be fully pinned (fixed timestamps + identity yield stable SHAs) or only structural shape asserted. Default: pin identity and messages, control timestamps, and assert structure + relationships; treat fully-pinned SHAs as a nice-to-have. Flag to jp if a downstream effort needs exact SHAs.
- Exact agreed semantics for ahead/behind in the no-upstream and deleted-upstream states (where git2 and `rev-list` can legitimately differ). E-03 now states a provisional contract (ahead/behind = `None` for both states, E-03 AC11); this cross-check ratifies that contract as its expectation, surfacing any divergence back to E-03's spec rather than both docs deferring. Flag for jp only if the cross-check finds the provisional contract cannot hold.
