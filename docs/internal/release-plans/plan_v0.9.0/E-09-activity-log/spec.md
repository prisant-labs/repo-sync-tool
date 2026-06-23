---
effort: E-09
tracking-issue: 11
title: Activity Log Writer and Retention
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-02]
source: docs/internal/v1-architecture-and-decisions.md (Sections 4.5, 6); docs/internal/strategy-and-roadmap.md Section 4.2 (activity_records DDL)
---

# E-09 - Activity Log Writer and Retention

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started.
- **Next:** implement the append writer that records every git operation into `activity_records` with full context.
- **Blockers:** none beyond E-02 (the `activity_records` table, its indexes, and the `settings.activity_retention_d` column are defined there).

## Context

The activity log is RepoSync's audit trail: an append-only record of every git invocation. Each row combines two kinds of data - the literal raw-execution fields captured by the git engine (command, exit code, stdout, stderr, duration) and the semantic classification supplied by the caller (action type, status, reason code, a human summary, the commit range). The git CLI supplies only the raw-execution half; it does not classify the row. The log is what lets the Activity surface later show users the exact commands RepoSync ran ("engineering in the data"), and it is a **backend invariant that exists independently of how Activity is ever displayed**. No screen design is required to build it correctly; the table is already the contract (E-02), and this effort fills it.

The audit trail is paired with a retention sweep so the log does not grow without bound: old records are pruned per `settings.activity_retention_d` (default 90 days). The writer is the consumer the git engine (E-03) and scheduler (E-08) call after every operation; this effort owns the write path and the sweep, not the git operations themselves.

This effort writes the append + retention logic. It does NOT define the schema (E-02), run the git operations whose output it records (E-03), or schedule when operations happen (E-08). It is the sink those efforts write through.

## In scope

- An append writer that inserts one `activity_records` row per git operation, capturing the full context. The input is split into two provenance classes:
  - **Git-captured fields** (produced by E-03 `git/cli.rs` from the subprocess invocation): `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`.
  - **Caller-classified fields** (produced by the policy engine E-07 and the call sites E-08/E-03/E-10, NOT parsed out of git CLI output): `action_type`, `status`, `reason_code`, `summary`, `commit_range`.
  Plus the owning `repo_id` and a `timestamp`. The git CLI does NOT classify the row; it only supplies the raw-execution fields. The semantic classification (what the operation was, whether it succeeded, why, a human summary, the commit range) is the caller's responsibility.
- A retention sweep that deletes `activity_records` older than `settings.activity_retention_d` (default 90), reading the value live from `settings` so a user change takes effect.
- Invocation points: the writer is called from every git-operation path (manual `check_now`/`update_now`, scheduled checks, metadata fetches) so no operation goes unrecorded.
- Use of the E-02 indexes `(repo_id, timestamp DESC)` and `(timestamp DESC)` for the queries that read back recent activity.
- The retention sweep scheduled on a cheap cadence so it is not tied to any UI action. Pre-E-08, the sweep runs at startup only; the daily cadence attaches to E-08's scheduler tick when it lands (a soft coupling to E-08, not a hard dependency).

## Out of scope

- The `activity_records` schema and indexes themselves (E-02).
- Running the git subprocess and capturing its raw output (E-03 `git/cli.rs` produces `raw_command`/`raw_stdout`/`raw_stderr`/`exit_code`/`duration_ms`; this effort persists them).
- The scheduler loop that decides when checks happen (E-08).
- The `activity_list` IPC command and any rendering of the log (E-06 defines the type; the Activity screen is out of these efforts).
- The daily summary aggregation over this data (E-11), which reads `activity_records` but is a separate effort.

## Contract / deliverables

1. A writer function that appends a fully-populated `activity_records` row for any git operation, with all columns set: the git-captured fields (`raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`) from E-03, and the caller-classified fields (`action_type`, `status`, `reason_code`, `summary`, `commit_range`) from the policy engine (E-07) and the call sites (E-08/E-03/E-10), plus `repo_id` and `timestamp`. Column set per the full DDL in `docs/internal/strategy-and-roadmap.md` Section 4.2.
2. Every git-operation code path (manual, scheduled, metadata) records exactly one activity row, including failures (a failed fetch is recorded with its non-zero `exit_code` and stderr).
3. A retention sweep that prunes records older than `settings.activity_retention_d`, honoring the live setting (default 90).
4. The sweep runs without any UI trigger and holds only short DB transactions. Pre-E-08 it runs at startup only; the daily cadence attaches to E-08's tick once that effort lands (soft E-08 coupling).
5. The E-02 indexes `(repo_id, timestamp DESC)` and `(timestamp DESC)` exist and are usable by ordered read-back queries; E-09 does not own the activity-list read query (that is E-06/UI), so it verifies the access path with a representative read rather than owning it.

## Acceptance criteria

- [ ] AC1: Every git operation appends one `activity_records` row populated with the git-captured fields (`raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`, from E-03) and the caller-classified fields (`action_type`, `status`, `reason_code`, `summary`, `commit_range`, from the policy engine E-07 and the call sites E-08/E-03/E-10), plus `repo_id` and a `timestamp`. The classified fields are supplied by the caller, NOT parsed from git CLI output. Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (`activity_records` columns) and brief Section 6 (activity-writer workstream).
- [ ] AC2: Failed operations are recorded too, with their non-zero `exit_code` and captured `raw_stderr`, not dropped. Source: brief Section 6 ("append every operation ... with full context") and Section 4.6 (full stdout/stderr/exit-code capture for the audit trail).
- [ ] AC3: A retention sweep deletes records older than `settings.activity_retention_d`, defaulting to 90 days, reading the value live from `settings`. Source: brief Section 6 (retention sweep honoring `activity_retention_d`, default 90).
- [ ] AC4: The audit trail is written and pruned with no dependency on any UI surface; the sweep runs on a startup cadence (pre-E-08) and gains a daily cadence attached to E-08's tick once it lands, never on a screen action. Source: brief Section 6 ("the audit trail is a backend invariant ... independent of how Activity is ever displayed").
- [ ] AC5: The E-02 indexes `(repo_id, timestamp DESC)` and `(timestamp DESC)` exist, and a representative ordered read-back query uses them (verified via `EXPLAIN QUERY PLAN`). E-09 does not own the `activity_list` read query (that is E-06/UI); this AC confirms the access path is available, not that E-09 owns the query. Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (indexed by those keys).

## Dependencies

- Upstream: E-02 (the `activity_records` table + indexes and the `settings.activity_retention_d` column).
- Downstream: E-11 (daily summary aggregates over `activity_records`); E-06 (the `activity_list` / `ActivityRecord` IPC type renders this data); E-03 and E-08 call the writer.

## V1.1 extension points

- Configurable per-action-type retention (e.g. keep failures longer than successes) extends the sweep without schema change beyond an additive column.
- An exported-log archive (Activity "Export markdown") reads the same records; the writer needs no change.
- A size-cap retention mode (prune by row count, not just age) is an additive option on the sweep.

## Open questions

- Whether the writer should redact or truncate very large `raw_stdout`/`raw_stderr` blobs before storing. Default to storing full output for fidelity (the audit trail's whole value is the literal output); flag if DB growth becomes a concern, since a truncation cap is an easy additive change.
