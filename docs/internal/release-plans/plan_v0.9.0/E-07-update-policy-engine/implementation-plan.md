---
effort: E-07
plan_for: spec.md
status: ready
---

# E-07 Implementation Plan

## Approach

Build the engine test-first, because it is pure logic over a fixed, enumerable input space. The full input space is small and known up front: 7 fixture states x 3 V1 modes for the decision table, plus a handful of run-outcome transitions for the failure state machine. So the right order is: write the failing tests that pin every cell and every transition, then implement the smallest decision function that turns them green, then add the stubbed future modes last as a closed-enum guard. No git, no DB, no clock is ever touched, so every test is deterministic and runs in plain `cargo test`.

The decision table is the contract; the tests are the table written as assertions. Implementation is just the function that satisfies them.

## Steps

1. **Define the input and output types (signatures only).** In `policy.rs`, declare `UpdateMode` (the closed enum: `CheckOnly`, `FetchOnly`, `PullFfOnly`, plus the stubbed future variants), `UpdatePolicy` (mode + any per-repo policy fields), the `RepoState` the engine consumes (clean/dirty/ahead/behind/detached/deleted-upstream/no-upstream, sourced from E-03's inspect types), and the output `PolicyDecision` = `Action(...)` or `Skip(SkipReason)`. Use placeholder `SkipReason`/failure-reason types now; they map onto E-05 `AppError` codes when E-05 lands. Leave the function bodies `unimplemented!()` so the tests compile and fail.
2. **Write the failing decision-table tests (TEST-FIRST).** One test per cell: for each of the 7 fixture states, assert the decided `PolicyDecision` under `check_only`, `fetch_only`, and `pull_ff_only`. This is 21 assertions and they are the heart of the effort. Encode the safety rules as explicit expectations:
   - `check_only` never returns a mutating or fetching action for any state (it only reports).
   - `fetch_only` returns a fetch action where an upstream resolves; returns a typed upstream skip-reason for no-upstream and deleted-upstream.
   - `pull_ff_only` returns the ff action only for a clean, behind, fast-forwardable repo; returns dirty-skip on a dirty tree; returns ff-not-possible on a diverged/non-fast-forwardable behind; returns detached-HEAD and upstream skip-reasons for those states.
   - The **diverged (ahead-and-behind, e.g. the brief mockup's `up 2 down 5`) case** is an explicit assertion within the behind row, not an assumed cell: under `pull_ff_only` it must resolve to `ff-not-possible`. The asserted cells match the explicit decision grid in the spec.
   These tests fail against the `unimplemented!()` body, proving the harness exercises the function.
3. **Write the failing failure-state-machine tests (TEST-FIRST).** Table-drive the transitions: given `(prior_consecutive_failures, run_outcome)` - where `prior_consecutive_failures` is the value E-08 reads from `repo_local_state.consecutive_failures` - assert the new repo status. Cover: success resets the counter and keeps the repo active; a single network failure yields retry without pause; an auth failure pauses immediately regardless of count; the 1st and 2nd consecutive failures stay active/retry; the **3rd consecutive failure signals auto-pause** (which E-08 persists as `auto_paused = 1`); a success after two failures resets so a later failure starts the count again.
4. **Implement the decision function.** Write the smallest `decide(state, policy)` that turns the 21 cell-tests green. Express it as an explicit match over `(mode, state)` so every cell is visible and exhaustive - no catch-all that hides a missing case. Keep dirty handling and branch policy as named guards so they read as the safety rules they are.
5. **Implement the failure state machine.** Write the transition function `next_status(prior_failures, outcome) -> RepoStatus` that turns the failure tests green: classify the outcome, increment/reset the consecutive counter (the value E-08 persists to `repo_local_state.consecutive_failures`), apply auth->pause and 3-strikes->signal-auto-pause (E-08 persists `auto_paused = 1`). The engine reads the prior count and returns the new status; it never writes the columns itself.
6. **Add the non-V1-mode guard (the invariant, not a fixed variant list).** Ensure any `UpdateMode` that is not one of the three V1 modes either returns a typed "mode not available in V1" skip-reason or is rejected at the boundary; add a test asserting such a mode is never treated as one of the three live modes. Which future variants the closed enum declares is an agent default (the brief names none), so declare only what keeps the enum closed and the guard testable. The enum is closed, so a later `match` addition is a deliberate change, not a silent default.
7. **Wire reason codes to `AppError`.** Once E-05 is available, replace placeholder reason types with the real `AppError` codes (ff-not-possible, dirty, detached-HEAD, deleted-upstream, no-upstream, auth-failure, network-lost) and update the tests to assert on the stable codes. Until then, keep a single mapping point so the swap is localized.
8. **Verify exhaustiveness.** Confirm the decision-table test count equals 7 x 3 with no gaps, and that `cargo clippy --all -- -D warnings` is clean (no unreachable arms, no missing match arms).

## Test strategy

- **TEST-FIRST is mandatory here.** Per the brief and the efforts README, the pure engines (E-07, E-08) are built test-first. The failing decision-table and failure-machine tests are written in steps 2-3, before the implementation in steps 4-5.
- **The 7 fixture states from E-04 are the test inputs.** The engine is pure, so the tests construct `RepoState` values matching the seven states the E-04 harness fabricates (clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream). The engine tests do not need the real harness repos - they assert over the state values - but the state definitions must stay in lockstep with what E-04 produces, so an integration test that feeds real harness-derived states through `decide` is a valuable cross-check to add once E-04 lands.
- **Exhaustive over the input space.** Every cell of the 7x3 table is a named assertion; every failure-machine transition is a table-driven case. There is no sampling - the space is small enough to cover fully.
- **No I/O, no time, no flakes.** Because the engine touches no git, DB, network, or clock, the suite is fully deterministic and fast; it is the cheapest high-value test surface in the project.

## Files / modules touched

- `crates/reposync-core/src/policy.rs` (the engine: types, `decide`, the failure state machine, the stubbed-mode guard, and the unit tests).
- Read-only dependency on the `RepoState`/inspect types from `crates/reposync-core/src/git/` (E-03) and, once available, the `AppError` codes in `crates/reposync-core/src/error.rs` (E-05).
- No `src-tauri` changes: this is pure core logic with no command wrapper of its own (callers in E-08 and the manual command path invoke it).

## Risks and mitigations

- **The behind/diverged vs. fast-forwardable distinction is subtle.** Mitigation: pin it explicitly in the decision-table tests (diverged behind -> ff-not-possible) and reconcile with the ahead/behind semantics E-04's git2-vs-CLI cross-check establishes; flag any divergence back to the spec.
- **Reason codes drifting from E-05.** Mitigation: keep a single placeholder-to-`AppError` mapping point so adopting the real codes is one localized edit, and assert on codes (not message strings) in tests.
- **A future mode silently behaving like a V1 mode.** Mitigation: the closed enum plus an explicit "mode not available in V1" test makes any accidental live behavior fail a test rather than ship.
- **State definitions diverging from the E-04 harness.** Mitigation: add the optional integration cross-check that runs real harness-derived states through `decide` once E-04 exists, so the two cannot quietly drift.

## Definition of done

All seven acceptance criteria checked; the decision-table tests cover all 7x3 cells and the failure-machine tests cover every transition including the 3-strikes auto-pause; the engine performs no I/O; reason codes map onto E-05 `AppError` codes (or a single, clearly-marked placeholder mapping pending E-05); `cargo test`, `cargo clippy --all -- -D warnings`, and `cargo fmt --check` are green; and the branch is ready for self-merge per the visibility-tiered policy in `EXECUTION.md`.
