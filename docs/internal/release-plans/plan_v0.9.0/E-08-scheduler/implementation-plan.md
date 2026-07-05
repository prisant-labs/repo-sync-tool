---
effort: E-08
plan_for: spec.md
status: ready
---

# E-08 Implementation Plan

## Approach

Build the scheduler test-first around an injected clock, because every interesting property is either time-dependent (cadence, jitter, quiet hours, `next_check_at`) or concurrency-dependent (the semaphore-plus-per-repo-mutex composition), and both are untestable in any sane way against the real wall clock or against real git races. So the first move is to introduce the `Clock` trait (a time source) and an injectable randomness source; the second is to write failing unit tests that advance a fake clock and assert the exact due-set, jitter offsets, quiet-hours gating, and recomputed `next_check_at`; the third is to implement the tick loop and the concurrency composition that turn those tests green. The git execution and the DB are behind the seams E-03 and E-02 own, so they are stubbed/faked in tests - the scheduler's job is *orchestration*, and that is what the tests pin.

The two load-bearing correctness properties (per-repo mutex composed under the global semaphore, in that fixed order; and no DB transaction held across a network call) are asserted directly, not assumed.

## Steps

1. **Introduce the injected `Clock` and randomness seams.** Define a `Clock` trait (e.g. `now()` returning the current instant/local time) with a real implementation for production and a `FakeClock` for tests that the test can advance arbitrarily. Define an injectable jitter/randomness source likewise. Thread both into the scheduler's constructor so nothing inside the scheduler reads the wall clock or a global RNG directly.
2. **Define the scheduler's collaborators as traits/seams.** Behind small interfaces, model: the due-repo query (reads `enabled`, `auto_paused`, `next_check_at`, `check_frequency_min` and quiet-hours from `settings`), the per-job runner (decide via E-07, execute via E-03), and the outcome writer (persist `last_checked_at`/`last_updated_at`/`next_check_at`, persist `consecutive_failures`/`auto_paused`, request the activity write via E-09, request the event emit via E-06 - the scheduler requests those two; it does not write the activity row or emit events itself). Faking these keeps the scheduler tests pure orchestration.
3. **Write the failing due-query tests (TEST-FIRST).** With a `FakeClock`, assert the due-set is exactly the repos where `enabled = 1` AND `auto_paused = 0` AND `next_check_at <= now()` AND now is outside quiet hours. Cover: a disabled repo is never due; an auto-paused (`auto_paused = 1`) repo is never due and re-enters the due-set only after `auto_paused` resets to 0; a repo with `next_check_at` in the future is not due; a due repo inside quiet hours is **not selected** (held purely by the predicate, not a deferred queue) and is re-polled each tick; the same repo becomes due the instant quiet hours end (advance the fake clock to prove it - it is selected on the first tick now is outside the window, not pulled from a queue).
4. **Write the failing `next_check_at` math tests (TEST-FIRST).** Assert that after a job at fake-time T, `next_check_at` is recomputed as T + effective frequency, where the effective frequency is the per-repo `check_frequency_min` when set and the global 360-minute default otherwise. Cover both the override and the default path, and assert `last_checked_at`/`last_updated_at` are set from the clock.
4a. **Write the failing failure-counter / auto-pause persistence tests (TEST-FIRST).** Assert that on a job failure the scheduler increments `consecutive_failures`, and on a success resets it to 0; assert that when the E-07 policy engine signals 3 strikes the scheduler sets `auto_paused = 1` (and the persisted `consecutive_failures` matches the policy reset); assert that a successful manual check or an explicit user resume resets `auto_paused` to 0, which is what re-admits the repo to the due-set (cross-check with the auto-paused due-query case in step 3). The E-07 engine computes the decision against the read `consecutive_failures`; this effort only persists the returned values.
5. **Write the failing jitter tests (TEST-FIRST).** With the injectable randomness source pinned to known values, assert each due repo at startup is staggered by an offset within 0-30s, and that with a deterministic source the offsets are reproducible. Assert jitter does not leak past startup (steady-state ticks are not re-jittered, per the spec default).
6. **Write the failing concurrency tests (TEST-FIRST).** This is the heart of the correctness story:
   - **Global cap:** with the per-job runner faked to block on a barrier, assert no more than 4 jobs run concurrently (default semaphore size), and that the 5th starts only as one finishes.
   - **Per-repo serialization:** assert that two checks targeting the *same* `RepoId` (e.g. a scheduled check and a simulated manual check) never overlap - the second awaits the first via the per-repo mutex - while two checks on *different* repos may overlap up to the semaphore cap.
   - **Acquisition order:** assert the composition acquires the per-repo mutex before the semaphore permit (so a blocked-on-mutex repo does not hold a global permit while waiting), matching the brief's fixed order.
7. **Write the failing "no lock across network" test (TEST-FIRST).** Using the faked DB seam, assert the scheduler opens only short transactions and that the per-job git call happens with no DB transaction open (e.g. the fake DB records transaction open/close timestamps and the fake git runner records its call window; assert they do not overlap).
8. **Implement the tick loop.** Build the single `tokio::time::interval` at a one-minute period pinned to a named `const ONE_MINUTE`, so production cadence cannot drift; **only the test harness substitutes the period** to drive ticks directly rather than waiting. On each tick: run the due-query, then for each due repo spawn the composed job.
9. **Implement the concurrency composition.** For each due repo: look up/insert its `Arc<Mutex<()>>` in the `HashMap<RepoId, ...>`, acquire that mutex, then acquire a global `Semaphore` permit (default 4), then run the per-job runner; release in reverse order. This single ordering is the AC3/AC4 guarantee.
10. **Implement jitter, quiet hours, `next_check_at`, and failure-counter persistence.** Apply the 0-30s startup stagger from the injected randomness (startup and a newly-added repo's first schedule only; steady-state ticks carry no jitter); gate the due-query on quiet hours from the injected clock so an in-window repo is simply **not selected** until a later tick (re-polling, no deferred queue); recompute and persist `next_check_at` from the effective frequency after each job in a short transaction; in the same outcome write, persist `consecutive_failures` (increment on failure, reset to 0 on success) and set `auto_paused = 1` when E-07 signals 3 strikes.
11. **Wire the manual-check path to the same mutex.** Ensure `repo_check_now`/`repo_update_now` acquire the same per-repo mutex from the shared map, so manual and scheduled git ops on one repo serialize. Add a test that interleaves a manual and a scheduled check on one repo and asserts no overlap.
12. **Verify.** Confirm the suite is deterministic (no real sleeps), `cargo test` is green, `cargo clippy --all -- -D warnings` is clean, and the concurrency tests fail loudly if the acquisition order is swapped.

## Test strategy

- **TEST-FIRST is mandatory.** Per the brief and the efforts README, the pure background engine (E-08, with E-07) is built test-first: the due-query, `next_check_at`, jitter, quiet-hours, and concurrency tests in steps 3-7 are written and failing before the implementation in steps 8-11.
- **Deterministic via the injected clock and randomness source - no real wall-clock waits.** Every time-dependent assertion advances a `FakeClock`; every jitter assertion uses a pinned randomness source. The suite has no `sleep` against real time, so it is fast and flake-free, exactly the property the brief calls for.
- **Concurrency tested with barriers/counters, not timing.** The semaphore cap and per-repo serialization are proven with synchronization primitives (a shared counter of in-flight jobs, a barrier the fake runner blocks on), not by hoping a `sleep` interleaves a certain way.
- **Exercised against the 7 fixture states (E-04).** Integration-level tests drive the scheduler with the per-job runner backed by the E-04 fixtures so the orchestration is validated end to end through real git states once E-04 lands; unit tests use the faked runner.
- **The fixed acquisition order is an explicit assertion.** A test that swaps mutex/semaphore order must fail, so the ordering is regression-protected, not just documented.

## Files / modules touched

- `crates/reposync-core/src/scheduler.rs` (the tick loop, due-query, concurrency composition, jitter, quiet hours, `next_check_at`, the `Clock`/randomness seams, and the unit/integration tests).
- A small `Clock` trait + real/fake implementations (in `scheduler.rs` or a sibling time module within `reposync-core`).
- Read/write dependency on the E-02 schema columns via the DB seam (`repo_local_state`, `settings`, `repos.enabled/next_check_at/check_frequency_min`, and the auto-pause pair `repo_local_state.consecutive_failures`/`auto_paused`).
- Invokes the E-07 policy engine (`policy::decide`) and the E-03 `GitEngine`; requests writes/emits through the E-09 and E-06 seams.
- The shared per-repo `HashMap<RepoId, Arc<Mutex<()>>>` is exposed so the `src-tauri` manual-check commands acquire the same mutex (the command wrappers themselves live in E-06/E-12, not here).

## Risks and mitigations

- **Deadlock or starvation from the two-lock composition.** Mitigation: a single, tested acquisition order (per-repo mutex, then semaphore permit) and reverse release; a test that asserts a repo blocked on its mutex is not holding a global permit, which is the property that prevents the cap from being starved by waiters.
- **A DB lock accidentally held across a network call.** Mitigation: the explicit "no lock across network" test (step 7) fails if a transaction overlaps the git call; the implementation keeps the git op strictly between two short transactions.
- **Quiet-hours and jitter time math (midnight wrap, DST).** Mitigation: the injected clock makes both fully testable; pin the wrap-around and a DST-edge case in tests and flag the DST convention for jp per the spec's open question.
- **Per-repo `HashMap` growth / stale entries.** Mitigation: lazy insert plus drop-on-remove (spec default); a later sweep is a V1.1 extension if it ever matters.
- **Interval drift or missed ticks under load.** Mitigation: drive ticks through an injectable source in tests so behavior is deterministic; in production rely on `tokio::time::interval`'s catch-up semantics and keep per-tick work bounded by the semaphore.

## Definition of done

All seven acceptance criteria checked; the due-query, `next_check_at`, jitter, quiet-hours, and concurrency (global cap, per-repo serialization, acquisition order, no-lock-across-network) tests are present and green; every time-dependent test runs against the injected clock with zero real-time sleeps; the per-repo mutex is shared with the manual-check path so scheduled and manual ops on one repo cannot collide; `cargo test`, `cargo clippy --all -- -D warnings`, and `cargo fmt --check` are green; and the branch is ready for self-merge per the visibility-tiered policy in `EXECUTION.md`.
