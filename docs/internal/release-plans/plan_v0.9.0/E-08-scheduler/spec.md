---
effort: E-08
tracking-issue: 10
title: Scheduler
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-04, E-02]
source: docs/internal/v1-architecture-and-decisions.md (Section 4.7)
---

# E-08 - Scheduler

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started.
- **Next:** introduce the injected `Clock` trait and write the failing tests for the due-query and `next_check_at` math against a fake clock, before the tick loop exists.
- **Blockers:** none beyond E-04 (the scheduler drives git work against the fixture states) and E-02 (it reads/writes `repo_local_state` and reads `settings` for cadence and quiet hours).

## Context

The scheduler is RepoSync's background heartbeat. The model is **resident-only**: there is no OS scheduler in V1, so checks happen only while the app is running (brief decision/risk 8.9). The whole engine is one `tokio::time::interval` that ticks every minute; on each tick it asks the database which repos are due, fans the due set out through a bounded concurrency limit, runs each repo's check (deciding the action via the E-07 policy engine and executing it through the E-03 git engine), then records the outcome and schedules the next check. It holds only short DB transactions and **never holds a lock across a network call**.

Two correctness properties are load-bearing and both come straight from the brief. The first is the **per-repo async mutex**, a fix the strategy doc missed: a scheduled check and a manual "Check now" can otherwise launch two `git` processes in the same working tree at once, producing `index.lock` collisions, partial fetches, or a corrupted index. The global semaphore caps *total* git concurrency but does nothing to stop *two ops on the same repo*. The fix is a `HashMap<RepoId, Arc<tokio::sync::Mutex<()>>>` whose entries serialize all git work per repo, composed in a fixed order: **acquire the per-repo mutex first, then the global semaphore permit, then run.** A manual check on a repo mid-scheduled-check simply awaits the mutex and runs immediately after, instead of racing it.

The second is the **injected clock**. Cadence, jitter, quiet hours, and `next_check_at` math are all time-dependent, and a scheduler tested against the real wall clock is slow and flaky. So the scheduler takes its time from an injected `Clock` trait (a time source), letting tests advance time deterministically and assert the exact due-set, jitter offset, quiet-hours behavior, and recomputed `next_check_at` without waiting a single real second.

This effort owns the tick loop, the due-query, the concurrency composition, startup jitter, quiet-hours gating, the per-repo frequency override, and the `next_check_at` recomputation. It also owns **persisting the failure counter and auto-pause**: after each job it increments `repo_local_state.consecutive_failures` on failure and resets it to 0 on success, and when the E-07 policy engine signals 3 strikes it sets `repo_local_state.auto_paused = 1`. The auto-paused repo is excluded from the due-query until it resumes: `auto_paused` resets to 0 on a successful manual check or an explicit user resume. It consumes the schema from E-02 (both `consecutive_failures` and `auto_paused` are added in E-02's initial migration), the policy from E-07 (transitively, via E-04's fixtures it is tested against), and the git engine from E-03.

## In scope

- The scheduler task built on **one `tokio::time::interval` ticking every minute**, started at app launch and living for the process lifetime. The production period is pinned to a named `const ONE_MINUTE` so the production cadence cannot drift; **only the test harness substitutes the period** (to drive ticks directly without real waits).
- The **due-query**: on each tick, select repos where `enabled = 1` AND `auto_paused = 0` AND `next_check_at <= now()` AND the current time is **not inside quiet hours**, where `now()` comes from the injected clock. An auto-paused repo (3-strikes) is simply not selected; it re-enters the due-set only after `auto_paused` resets to 0 (on a successful manual check or an explicit user resume).
- The **bounded concurrency model**: a global `tokio::sync::Semaphore` with **default 4** permits, composed with a **per-repo async mutex** via `HashMap<RepoId, Arc<tokio::sync::Mutex<()>>>`. The fixed acquisition order is per-repo mutex first, then the semaphore permit, then the git work.
- **Startup jitter**: on launch, due repos run but are staggered by a **random 0-30s** offset to avoid a thundering herd on metered networks; the randomness source is injectable for deterministic tests. **Agent default (not brief-pinned):** jitter applies only at startup and to a newly-added repo's first schedule; **steady-state ticks carry no jitter**. See Open questions; flag for jp if startup-only is preferred.
- **Quiet hours**: a configured window (from `settings`) during which no checks start. "Due repos wait until the window closes" means they are simply **not selected** by the due-query while `now()` is inside the window; they are re-polled on each later tick and become selectable the first tick `now()` is outside the window. There is **no explicit deferred-task queue** - the gating is purely the due-query predicate re-evaluated against the injected clock per tick.
- **Per-repo frequency override**: a repo's own `check_frequency_min` overrides the global default (360 = 6h) when computing its `next_check_at`.
- **`next_check_at` math**: after each job, recompute the repo's next check time from its effective frequency and the injected clock; persist `last_checked_at`/`last_updated_at` and the new `next_check_at` in a **short DB transaction**.
- The **injected `Clock` trait** (a time source) threaded through every time-dependent decision (due-query, jitter, quiet hours, `next_check_at`), so the entire scheduler is testable without real wall-clock waits.
- **Failure-counter and auto-pause persistence**: after each job, persist `repo_local_state.consecutive_failures` (increment on failure, reset to 0 on success) and set `repo_local_state.auto_paused = 1` when the E-07 policy engine signals 3 strikes; `auto_paused` resets to 0 on a successful manual check or an explicit user resume. The E-07 engine computes the decision; E-08 owns the writes.
- Per-job side effects ordering: short DB transaction to read due repos and to write outcomes (including `consecutive_failures`/`auto_paused`); the git op runs **outside** any held DB transaction; the activity-row write and the event emit are **requested** via the seams E-09/E-06 own (this effort calls them, does not implement them).

## Out of scope

- The decision of *what* git action a repo's state implies: that is the E-07 policy engine, which the scheduler invokes per repo.
- The git execution itself (fetch, `pull --ff-only`, rev-list): the E-03 git engine, driven by the scheduler.
- The schema and the actual SQL for `repo_local_state`, `settings`, and the columns read/written (`enabled`, `next_check_at`, `check_frequency_min`, `last_checked_at`, `last_updated_at`, and the auto-pause pair `consecutive_failures`/`auto_paused` added in E-02's initial migration): defined by E-02; this effort issues reads/writes against them.
- Writing the `activity_records` row body and running retention (E-09); the scheduler triggers the write via the E-09 seam.
- Emitting the typed events (`scheduler:tick`, `repo:check-*`, `repo:update-*`): the event payload types and emit helpers are E-06; the scheduler requests the emits.
- Pausing/auto-pausing logic state transitions: computed by the E-07 failure state machine; the scheduler applies the returned status and persists it.
- The `AppError` variants surfaced on failure (E-05).
- Any UI rendering of cadence, quiet hours, or the scheduler tick (UI surface, out of these efforts).

## Contract / deliverables

1. A scheduler driven by a single `tokio::time::interval` at a one-minute period pinned to a named `const ONE_MINUTE` (only the test harness substitutes the period), started at launch and resident for the process lifetime.
2. A due-query selecting `enabled = 1` AND `auto_paused = 0` AND `next_check_at <= now()` AND not-in-quiet-hours, with `now()` taken from the injected clock.
3. A bounded global `Semaphore` (default 4) composed with a per-repo `HashMap<RepoId, Arc<tokio::sync::Mutex<()>>>`, acquired in the fixed order: per-repo mutex, then semaphore permit, then git work - so at most one git op runs per working tree and at most four run globally.
4. Startup jitter of a random 0-30s per due repo, with an injectable randomness source for deterministic tests.
5. Quiet-hours gating that prevents checks from starting inside the configured window and resumes them after it.
6. Per-repo `check_frequency_min` overriding the global 360-minute default in the `next_check_at` computation.
7. `next_check_at` recomputed after each job and persisted alongside `last_checked_at`/`last_updated_at` in a short DB transaction, with no DB transaction held across the git/network call.
8. Failure-counter and auto-pause persistence: `consecutive_failures` incremented on failure and reset to 0 on success, and `auto_paused = 1` set when E-07 signals 3 strikes; `auto_paused` resets to 0 on a successful manual check or an explicit user resume, which is what re-admits the repo to the due-query.
9. A `Clock` trait (time source) injected throughout, so cadence, jitter, quiet hours, and `next_check_at` are all tested deterministically with a fake clock and no real waits.

## Acceptance criteria

- [ ] AC1: The scheduler is one `tokio::time::interval` ticking every minute under the resident-only model (no OS scheduler in V1), with the production period pinned to a named `const ONE_MINUTE` that only the test harness substitutes. Source: brief Section 4.7 ("One `tokio::time::interval` ticks every minute"; resident-only).
- [ ] AC2: The due-query selects repos where `enabled = 1` AND `auto_paused = 0` AND `next_check_at <= now()` AND not inside quiet hours; an auto-paused (3-strikes) repo is excluded until `auto_paused` resets to 0 on a successful manual check or explicit user resume. Source: brief Section 4.7 ("query the DB for repos where `enabled = 1` AND `next_check_at <= now()` AND not inside quiet hours") plus the ratified auto-pause persistence (`repo_local_state.auto_paused`).
- [ ] AC3: Concurrency is a bounded `Semaphore` defaulting to 4, composed with a per-repo `HashMap<RepoId, Arc<tokio::sync::Mutex<()>>>`, acquiring the per-repo mutex first, then the semaphore permit, then running. Source: brief Section 4.7 ("Fan out through a bounded `Semaphore` (default 4)"; "acquire the per-repo mutex first, then the semaphore permit, then run").
- [ ] AC4: At-most-one git op runs per working tree: a scheduled check and a manual check on the same repo serialize via the per-repo mutex and cannot run two git ops concurrently in one tree. Source: brief Section 4.7 (per-repo locking correctness fix).
- [ ] AC5: On startup, due repos run but are staggered with a random 0-30s jitter; steady-state ticks carry no jitter (only startup and a newly-added repo's first schedule are jittered) - the agent default noted in In scope / Open questions. Source: brief Section 4.7 ("stagger with jitter (random 0-30s)").
- [ ] AC6: After each job the scheduler updates `last_checked_at`/`last_updated_at`, recomputes `next_check_at` (honoring the per-repo `check_frequency_min` override over the 360-minute default), persists `consecutive_failures` (increment on failure, reset to 0 on success) and `auto_paused` (set to 1 on the E-07 3-strikes signal), **requests** the activity-row write (via the E-09 seam) and the event emit (via the E-06 seam) - it does not write the activity row or emit events itself, since E-09/E-06 own those - using only short DB transactions and never holding a lock across a network call. Source: brief Section 4.7 ("After each job ... never holds a lock across a network call") and Section 4.5 (`check_frequency_min` default 360).
- [ ] AC7: All time-dependent behavior (cadence, jitter, quiet hours, `next_check_at`) is driven by an injected `Clock`/time source and is tested deterministically without real wall-clock waits. Source: brief Section 6 (Scheduler row: "Built with an injected clock (a `Clock` trait or time source) so cadence, jitter, quiet hours, and next-check math are tested deterministically").

## Dependencies

- Upstream: E-04 (fixture harness - the scheduler is exercised against the 7 states through the git engine) and E-02 (schema: `repo_local_state`, `settings`, and the `repos`/`repo_local_state` columns `enabled`, `next_check_at`, `check_frequency_min`, plus the auto-pause pair `consecutive_failures`/`auto_paused` added in E-02's initial migration). Transitively it drives E-03 (git engine) and E-07 (policy engine).
- Downstream: E-09 (the scheduler triggers each activity-row write), E-06 (the scheduler requests the typed event emits), and the manual `repo_check_now`/`repo_update_now` command path, which shares the same per-repo mutex so manual and scheduled git ops never collide.

## V1.1 extension points

- An optional OS-scheduler path (Windows Task Scheduler / launchd) for non-resident checks could be added behind the same due-query, replacing only the trigger source while the per-repo mutex and policy stay intact.
- A backoff schedule that lengthens `next_check_at` for repeatedly-failing or auto-paused repos, layered onto the existing `next_check_at` math.
- Quiet-hours per-repo overrides, extending the single global window already honored here.

## Open questions

- Whether jitter applies only at startup (per the brief's "On startup") or also to the first scheduled run of a newly-added repo. Agent default: apply jitter at startup and on a newly-added repo's first-ever schedule, and **steady-state ticks carry no jitter**; flag for jp if startup-only is preferred. This default is the one asserted in AC5 and the jitter tests.
- Quiet-hours windows that cross midnight and DST transitions need a defined convention; the injected clock makes both testable. Default: store quiet hours as local wall-clock start/end and evaluate against the clock's local time, treating a wrap-around window as spanning midnight; flag the DST edge for jp.
- The eviction policy for the per-repo mutex `HashMap` (entries for removed repos). Default: lazily insert on first use and drop the entry when a repo is removed; flag if a periodic sweep is preferred.
