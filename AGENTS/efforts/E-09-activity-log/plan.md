---
effort: E-09
plan_for: spec.md
status: ready
---

# E-09 Implementation Plan

## Approach

Two small, independent pieces: an append writer and a retention sweep, both operating on the `activity_records` table that E-02 froze. The writer is a pure DB sink that takes a value combining the git-captured raw-execution fields (from E-03) and the caller-classified semantic fields (from E-07/E-08/E-03/E-10) and inserts one row; the git CLI supplies only the raw half, the caller classifies the rest. Keeping the writer a single chokepoint means every git path records consistently and no operation can silently skip the log. The sweep is a cheap periodic `DELETE` driven by the live `settings.activity_retention_d` value. Everything lives in `activity.rs` in `reposync-core`, stays Tauri-free, and is tested against an on-disk tempdir SQLite so the indexes and transactions behave exactly as in production.

## Steps

1. **Define the activity-record input shape.** In `activity.rs`, define the in-memory struct the writer consumes, with its two provenance classes made explicit:
   - Git-captured (from E-03 `git/cli.rs`): `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`.
   - Caller-classified (from the policy engine E-07 and the call sites E-08/E-03/E-10, NOT parsed from git output): `action_type`, `status`, `reason_code`, `summary`, `commit_range`.
   Plus `repo_id` and an optional caller-supplied `timestamp`. The git CLI supplies only the raw-execution fields; the caller classifies the semantic fields and passes them in. Timestamp handling: accept an optional caller-supplied timestamp (an injected clock) for deterministic tests, defaulting to "now" at insert time when the caller does not provide one.
2. **Implement the append writer.** A single `record(...)` function that inserts one row into `activity_records` inside a short transaction. It is infallible-by-design from the caller's perspective in the sense that a logging failure must not abort the git operation itself; on a DB write error, log the failure and continue (the operation already happened). Populate every column (AC1); never drop failed operations (AC2).
3. **Wire the invocation points.** Make `record(...)` the single sink called from each git-operation path - manual `check_now`/`update_now`, scheduled checks (E-08), and metadata fetches (E-10) - so exactly one row is written per operation, success or failure. Where E-03/E-08 land later, expose `record(...)` as the agreed seam so they call it rather than writing rows themselves.
4. **Implement the retention sweep.** A `sweep(retention_days)` function that reads `settings.activity_retention_d` live (default 90 if unset) and deletes `activity_records` where the timestamp is older than `now - retention_days`. Use a short transaction; do not hold a lock across anything else.
5. **Schedule the sweep without UI.** Pre-E-08, run the sweep at startup only. Once E-08 lands, attach the daily cadence to the scheduler's tick with a once-per-day guard (a soft coupling to E-08, not a hard dependency). It must not depend on any screen action (AC4).
6. **Confirm the index access path.** E-09 does not own the `activity_list` read query (that is E-06/UI). Confirm the E-02 indexes `(repo_id, timestamp DESC)` and `(timestamp DESC)` exist and that a representative ordered read-back query (order by `timestamp DESC`, optionally filtered by `repo_id`) uses them via an `EXPLAIN QUERY PLAN` assertion, rather than asserting ownership of the read query (AC5).
7. **Verify.** Run the tests below; confirm a sweep with a low retention value prunes old rows and keeps recent ones, and that a failed operation is recorded with its non-zero exit code and stderr.

## Test strategy

- **Append fidelity test.** Call `record(...)` with a representative success and a representative failure, supplying both the git-captured fields and the caller-classified fields (`action_type`, `status`, `reason_code`, `summary`, `commit_range`) plus an injected timestamp; read the rows back and assert every column matches, including the classified fields, the injected timestamp, and non-zero `exit_code` and non-empty `raw_stderr` for the failure (AC1, AC2).
- **Logging-never-aborts test.** Simulate a DB write error in the writer and assert the caller-side flow is not aborted (the error is logged, the operation result is still returned).
- **Retention boundary test.** Seed records straddling the cutoff (e.g. 89 vs 91 days old with retention 90); run the sweep and assert only the older rows are deleted. Then change `settings.activity_retention_d` and assert the next sweep honors the new value live (AC3).
- **No-UI cadence test.** Assert the sweep runs from the startup path with no screen involvement (AC4), exercised by calling the startup entry point directly. Pre-E-08 this is startup-only; the daily cadence is exercised once E-08's tick is wired.
- **Index-usage check.** `EXPLAIN QUERY PLAN` on a representative recent-activity read confirms it uses the `timestamp DESC` index rather than a full scan (AC5). This verifies the access path is available, not that E-09 owns the read query (that is E-06/UI).
- All run in `cargo test` against a tempdir SQLite migrated by E-02's runner; no Tauri host.

## Files / modules touched

- `crates/reposync-core/src/activity.rs` (the writer + the sweep; replaces the E-01 stub).
- The git-operation call sites (E-03 `git/cli.rs`, E-08 scheduler) call `activity::record(...)` - the seam is defined here, wired as those efforts land.
- `crates/reposync-core/src/lib.rs` (export the activity module API if not already).
- Tests under `crates/reposync-core` (unit/integration against migrated tempdir DB).

## Risks and mitigations

- **Logging failure masking a git result.** If a DB write error aborted the operation, a transient DB hiccup would look like a git failure. Mitigate by making the writer best-effort: log-and-continue on insert error, never propagate it into the operation's own result path.
- **Unbounded row growth between sweeps.** Heavy fetch cadence on many repos grows the table fast. Mitigated by the daily sweep and the bounded default retention; a row-count-cap mode is a documented additive extension if needed.
- **Large raw output bloating the DB.** Storing full stdout/stderr is the point, but pathological output could bloat the file. Default to storing full output; flag a truncation cap as an easy additive change (spec open question).
- **Index not used after a query rewrite.** A later change to the read query could drop index usage silently. Mitigated by the `EXPLAIN QUERY PLAN` test guarding the access path.

## Definition of done

All five acceptance criteria checked, the tests green in `cargo test` on Windows and in CI, `record(...)` is the single sink every git path uses, the sweep runs on the startup + daily cadence with no UI dependency, `reposync-core` still has no `tauri` in its tree, and the branch ready for self-merge per `EXECUTION.md`.
