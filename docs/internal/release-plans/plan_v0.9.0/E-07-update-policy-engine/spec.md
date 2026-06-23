---
effort: E-07
title: Update-Policy Engine
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-04]
source: docs/internal/v1-architecture-and-decisions.md (Sections 4.6, 6)
---

# E-07 - Update-Policy Engine

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started.
- **Next:** write the failing unit tests for the decision table - each of the 7 fixture states crossed with each V1 update mode - before any engine code exists.
- **Blockers:** none beyond E-04 (the policy is tested against the fixture states the harness fabricates; the git-state types it consumes come from E-03).

## Context

This effort is the product's core safety story, expressed as pure logic. The engine answers one question and only that question: given a repo's observed git state and the policy configured for it, what should happen - an intended git action, or a skip with a typed reason. It performs **no I/O**: no git, no network, no DB, no clock. That purity is deliberate. It makes the engine exhaustively testable against the fixture states from E-04 (git fixture harness) in plain `cargo test`, and it keeps the decision that governs whether RepoSync touches a working tree fully decoupled from rendering, scheduling, and persistence.

The shape is a pure function: `(repo state, policy) -> intended action or skip-with-reason`. The caller (the scheduler in E-08, or a manual `check_now`/`update_now` command) is what actually executes the returned action through the git engine; the policy engine never executes anything itself. The same engine drives both scheduled and manual paths, so the safety rules are defined exactly once.

Two bodies of logic live here. The first is the **decision table**: the 7 repo states (clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream), with the diverged (ahead-and-behind) case tested explicitly within the behind row, crossed with the 3 V1 update modes (`check_only`, `fetch_only`, `pull_ff_only`), each cell resolving to a concrete action or a typed skip-reason. The second is the **failure-handling state machine**: how an auth failure, a network failure, an ff-not-possible result, and a run of consecutive failures move a repo between active and paused, including the 3-strikes auto-pause. The engine READS `repo_local_state.consecutive_failures` to compute the 3-strikes decision; when it reaches 3 the engine signals auto-pause, which E-08 persists by setting `repo_local_state.auto_paused = 1` and resetting the counter per policy. Skip-reasons and failure reasons cross-reference the `AppError` codes from E-05 so the contract is stable and machine-readable.

The brief's safety posture is explicit and inherited verbatim: pull is fast-forward-only and rebase was dropped; a dirty working tree is never mutated; a missing or deleted upstream is a typed state, not a crash. This effort encodes those rules so that no downstream caller can accidentally violate them.

## In scope

- The pure decision function in `crates/reposync-core/src/policy.rs`: `(repo state, policy) -> Action | Skip(reason)`, with no I/O of any kind.
- The full decision table mapping each of the **7 fixture states** (clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream), plus the **diverged (ahead-and-behind)** case tested explicitly within the behind row, against each of the **3 V1 update modes** (`check_only`, `fetch_only`, `pull_ff_only`) to either an action or a typed skip-reason. The grid is the contract itself, shipped explicitly below rather than left as a derivation an agent must reconstruct:

  | State \ Mode | `check_only` | `fetch_only` | `pull_ff_only` |
  | --- | --- | --- | --- |
  | clean (up to date) | report status (no action) | fetch | report up-to-date (no merge needed) |
  | dirty | report status (no action) | fetch (refs only, tree untouched) | `Skip(dirty)` |
  | ahead | report status (no action) | fetch | report ahead (no remote commits to ff; no action) |
  | behind (fast-forwardable) | report status (no action) | fetch | `pull --ff-only` (the one mutating action) |
  | behind-and-also-ahead (diverged, e.g. `up 2 down 5`) | report status (no action) | fetch | `Skip(ff-not-possible)` |
  | detached HEAD | report status (no action) | fetch | `Skip(detached-HEAD)` |
  | deleted-upstream | report status (no action) | `Skip(deleted-upstream)` | `Skip(deleted-upstream)` |
  | no-upstream | report status (no action) | `Skip(no-upstream)` | `Skip(no-upstream)` |

  `check_only` never mutates or fetches for any state; `fetch_only` updates refs only and never touches the working tree; `pull_ff_only` is the only mode that can mutate, and only on a clean, fast-forwardable behind repo. The diverged row resolves to `ff-not-possible` under `pull_ff_only` because a fast-forward is impossible when local history has diverged.
- **Dirty handling:** an uncommitted working tree is never mutated; a mode that would write the tree (`pull_ff_only`) yields a typed "skipped: dirty" reason rather than an action.
- **Branch policy:** detached HEAD and no-upstream are typed skip-reasons, not errors; pull is strictly fast-forward-only, so a non-fast-forwardable behind state yields the "ff-not-possible" reason.
- **Failure-handling state machine:** classifies a run's outcome (success, ff-not-possible, auth failure, network failure) and computes the resulting repo status, reading `repo_local_state.consecutive_failures` as the prior count, including:
  - auth failure -> the repo pauses,
  - network failure -> retry (transient, no pause),
  - **3 consecutive failures -> auto-pause**: when the read `consecutive_failures` reaches 3 the engine signals auto-pause (E-08 persists this by setting `repo_local_state.auto_paused = 1`), with the consecutive-failure counter reset on any success.
- The closed `UpdateMode` enum's **invariant**: any mode that is not one of the three V1 modes (`check_only`/`fetch_only`/`pull_ff_only`) is **never executed as a V1 mode** - it returns a typed "mode not available in V1" skip-reason or is rejected at the boundary. The brief names no specific future modes, so exactly which future variants exist (if any) is an agent default / open question, not a fixed contract (see Open questions). Whatever variants are declared, the closed enum makes any later live behavior a deliberate, reviewable change.
- Cross-references from every skip-reason and failure reason to the corresponding `AppError` code defined in E-05 (e.g. ff-not-possible, dirty-skip, detached-HEAD, deleted-upstream, no-upstream, auth-failure, network-lost).

## Out of scope

- Executing any returned action: the fetch, the `pull --ff-only`, the rev-list - all belong to the git engine (E-03) and are driven by the scheduler (E-08) or a manual command.
- Reading or producing git state: the `RepoState` the engine consumes is computed by `git/inspect.rs` and `git/cli.rs` parsers (E-03).
- Persisting `repo_local_state.consecutive_failures`, `last_error_code`, or `repo_local_state.auto_paused`: the engine READS `consecutive_failures` to compute the 3-strikes decision and signals auto-pause when it reaches 3, but the writes belong to E-08. E-02's initial migration adds both columns (`consecutive_failures INTEGER NOT NULL DEFAULT 0` and `auto_paused INTEGER NOT NULL DEFAULT 0`); E-08 increments/resets `consecutive_failures` and sets `auto_paused = 1` on the signal.
- Scheduling, cadence, jitter, quiet hours, concurrency, and `next_check_at` math (E-08).
- Defining the `AppError` enum itself (E-05); this effort references its codes and uses placeholder reason types until E-05 lands, then maps onto the real codes.
- Any rendering of the mode pill, the skip reason, or the paused state (UI surface, out of these efforts).

## Contract / deliverables

1. A pure function in `policy.rs` of the shape `decide(state: &RepoState, policy: &UpdatePolicy) -> PolicyDecision`, where `PolicyDecision` is either an intended `Action` or a `Skip(SkipReason)`, performing no I/O.
2. A complete decision table - the explicit grid in "In scope" is the contract: all 7 fixture states x all 3 V1 modes, plus the diverged (ahead-and-behind) case tested within the behind row, resolve to a defined action or a typed skip-reason, with no implicit or panicking cells.
3. Dirty handling: `pull_ff_only` on a dirty tree returns `Skip` with a dirty reason; no mode ever returns an action that would mutate a dirty tree.
4. Branch policy: detached HEAD, no-upstream, and deleted-upstream each return a distinct typed skip-reason; a behind state that cannot fast-forward returns the ff-not-possible reason.
5. A failure-handling state machine that, given the prior failure count (read from `repo_local_state.consecutive_failures`) and a run outcome, returns the new repo status: active, retry, paused (auth), or auto-paused (when the count reaches 3 the engine signals auto-pause for E-08 to persist as `auto_paused = 1`), and resets the counter on success.
6. The closed `UpdateMode` enum upholds the invariant that any non-V1 mode is explicitly rejected (or routed to a "mode not available in V1" reason) rather than silently treated as a V1 mode. Which future variants (if any) the enum declares is an agent default, since the brief names none.
7. Every `SkipReason` and failure outcome carries the `AppError` code it corresponds to, so callers and the UI key off stable machine codes.

## Acceptance criteria

- [ ] AC1: `policy.rs` exposes a pure `(repo state, policy) -> action or skip-with-reason` function that performs no git, network, DB, or clock I/O. Source: brief Section 6 (Update-policy engine row: "Pure function ... No I/O, no git, no DB").
- [ ] AC2: A decision table covers all 7 fixture states (clean, dirty, ahead, behind, detached HEAD, deleted-upstream, no-upstream) crossed with `check_only`, `fetch_only`, and `pull_ff_only`, each cell resolving to a defined action or typed skip-reason, matching the explicit grid in "In scope". The diverged (ahead-and-behind, e.g. the brief mockup's `up 2 down 5` repo) case is covered explicitly - tested within the behind row and resolving to `ff-not-possible` under `pull_ff_only` - not assumed. Source: brief Section 6 (Update-policy engine; fixture states) and Section 4.6.
- [ ] AC3: Dirty handling never mutates a dirty working tree; `pull_ff_only` on a dirty repo yields a typed "skipped: dirty" reason. Source: brief Section 5, State semantics (the `### 4` subsection under `## 5`) (dirty: "Skipped per policy") and Section 6 (dirty handling).
- [ ] AC4: Branch policy yields distinct typed skip-reasons for detached HEAD, no-upstream, and deleted-upstream, and pull is strictly fast-forward-only with an ff-not-possible reason when a behind state cannot fast-forward (including the diverged ahead-and-behind case). Source: brief Section 6 (branch policy, ff-not-possible) and Section 5, State semantics (the `### 4` subsection under `## 5`) (pull is "intentionally fast-forward-only and rebase was dropped").
- [ ] AC5: The failure-handling state machine pauses on auth failure, retries on network failure, and signals auto-pause after 3 consecutive failures (reading the prior count from `repo_local_state.consecutive_failures`; E-08 persists the auto-pause as `auto_paused = 1`), resetting the consecutive counter on any success. Source: brief Section 6 (failure handling: "auth failure -> pause, network -> retry, 3 consecutive -> auto-pause").
- [ ] AC6: The real invariant holds: any mode that is not one of the three V1 modes is never silently executed as a V1 mode - it returns a "mode not available in V1" skip-reason or is rejected at the boundary, enforced by the closed `UpdateMode` enum. Which future variants (if any) the enum declares is an agent default, since the brief names none (see Open questions); the invariant, not the variant list, is the contract.
- [ ] AC7: Each skip-reason and failure outcome maps to a stable `AppError` code from E-05 (ff-not-possible, dirty, detached HEAD, deleted upstream, no upstream, auth failure, network lost). Source: brief Section 6 (`AppError` taxonomy row) and Section 4.6.

## Dependencies

- Upstream: E-04 (git fixture harness - the 7 states the engine is tested against), which itself depends on E-03 (the `RepoState`/inspect types the engine consumes). E-05 (`AppError`) is referenced for reason codes; the engine uses placeholder reason types until E-05 lands.
- Downstream: E-08 (the scheduler calls this engine on every tick to decide each repo's action), and the manual `repo_check_now`/`repo_update_now` command path, which calls the same engine so scheduled and manual decisions are identical.

## V1.1 extension points

- The stubbed future update modes (anything beyond `check_only`/`fetch_only`/`pull_ff_only`) go live by adding behavior to their existing enum variants; the closed enum means each addition is a deliberate, reviewable change.
- A per-repo dirty-handling override (e.g. autostash) could be added as a new policy field without changing the decision-table shape.
- Richer failure classification (distinguishing transient network classes, credential-helper-specific auth errors) can refine the failure state machine once real captures from E-03/E-09 are observed in the field.

## Open questions

- Which future (non-V1) update modes the closed `UpdateMode` enum should declare, if any. The brief names none, so this is an agent default rather than a brief-sourced requirement: declare only what is needed to keep the enum closed and the "mode not available in V1" path testable, and flag for jp if a specific future mode set should be pinned now. The invariant (any non-V1 mode is never executed as a V1 mode) holds regardless of which variants exist.
- The exact behind-state distinction between "fast-forwardable" and "ff-not-possible" depends on ahead/behind semantics that E-03/E-04 pin for the no-upstream and deleted-upstream cases. The decision grid above treats the behind-and-also-ahead (diverged) cell as ff-not-possible by default; flag any divergence the E-04 cross-check surfaces back to this spec.
- Whether `fetch_only` on a deleted-upstream or no-upstream repo should attempt the fetch (and surface the failure) or skip up front with a typed reason. Default: skip up front with the typed upstream reason, since a fetch with no resolvable upstream is a known no-op; flag for jp if a "try and report" posture is preferred for observability.
