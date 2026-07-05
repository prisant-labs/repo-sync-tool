---
effort: E-11
tracking-issue: 13
title: Summary Engine (Daily)
status: ready
tier: SHOULD
scope: V1 (non-GUI)
depends_on: [E-09]
source: docs/internal/v1-architecture-and-decisions.md (Sections 3, 4.4); docs/internal/strategy-and-roadmap.md Section 4.2 (status/action_type enums)
---

# E-11 - Summary Engine (Daily)

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** done (2026-06-29). Built test-first in `crates/reposync-core/src/summary.rs` (+11 tests); commits `671cdb9` (build) + `32755e7` (adversarial-review fixes); reviewed via `codex:adversarial-review`. Issue #13 closed. NOTE: `summary_today` returns the FROZEN `ipc::DailySummary` (E-06 froze the shape, field `attention`); the spec's "E-11 owns the shape" language predates that freeze.
- **Next:** wiring only - expose `summary_today` as the IPC command (the edge wiring effort supplies the local-midnight `DayWindow`); the Dashboard "Today's read" card renders the value.
- **Blockers:** none. Carry-forward: new-release detection uses the mutable latest-release snapshot, so an immutable release-event source is needed for faithful past-day / multi-release history (BL-NI-16, resolve with the E-10 wiring + BL-NI-15). Weekly stays the V1.1 `summary_week` seam.

## Context

The daily summary is the cheap delight that makes the tray feel alive: a nightly read-out of what changed today - repos updated, new releases detected, what still wants attention. It is a SHOULD, not a MUST, and is the thing the descope trigger cuts first if week 5 slips (brief Section 3). Its whole job is aggregation over data that already exists: the `activity_records` from E-09 plus the cached state in `repo_local_state` / `repo_remote_meta`. No new data is produced; the summary is a read-only roll-up.

V1 is **daily only**. The brief cuts `summary_week()` / `WeeklySummary` to V1.1. This effort therefore delivers `summary_today() -> DailySummary` for real and leaves `summary_week()` as a documented, stubbed extension-point seam so the IPC contract (E-06) can name it without it being implemented.

This effort OWNS the `DailySummary` value shape: its field names and structure are defined here, and E-06 CONSUMES this shape when it generates the IPC type. This resolves the circular ownership (E-06's contract surface vs. E-11's produced value) in E-11's favor - E-11 is the source of truth for the shape, E-06 binds it to the transport.

This effort writes the aggregation. It does NOT write the activity records it reads (E-09) or render the summary card (the Dashboard "Today's read" surface is out of these efforts).

## In scope

- E-11 OWNS the `DailySummary` value shape. Its fields are enumerated concretely here, and E-06 consumes this shape (ownership resolved in E-11's favor). The shape is:
  - `updated_count: integer` - repos updated today.
  - `releases_count: integer` - new releases detected today.
  - `attention_count: integer` - repos currently needing attention (see the V1 definition below).
  - `no_change_count: integer` - repos checked today with no change.
  - `updated: list` - the updated-repo items (which repos updated and how).
  - `new_releases: list` - the new-release items (which releases were detected).
  - `needs_attention: list` - the needs-attention items (which repos need attention).
- `summary_today() -> DailySummary`: aggregate today's activity and state changes into this value.
- A cheap implementation: read-only queries over `activity_records` (E-09) and the cached `repo_local_state` / `repo_remote_meta` (E-02), no git or network calls.
- A documented `summary_week()` extension-point seam: a stub that names the V1.1 surface (weekly aggregation over the same data) without implementing it.
- Keeping the aggregation fast enough to run on demand (e.g. when the tray popup or Dashboard asks) without noticeable latency.
- A V1 attention definition that does NOT depend on E-07's thresholds: count repos with `last_error_code` set OR `is_dirty` set (read from `repo_local_state`). The richer "behind past threshold" semantics that the policy engine (E-07) owns are deferred and surfaced as an open question below.

## Out of scope

- Writing the `activity_records` the summary reads (E-09).
- Running any git operation or network fetch; the summary is pure aggregation over cached data.
- Implementing weekly aggregation / `WeeklySummary` - explicitly CUT to V1.1, left only as a stubbed seam here.
- The `DailySummary` rendering (Dashboard "Today's read" card, tray popup summary) and the Markdown export - UI surfaces out of these efforts.
- Persisting summaries as an archive; V1 computes on demand. (A `summaries` archive is a possible V1.1 additive extension, not built here.)

## Contract / deliverables

1. E-11 defines the `DailySummary` shape (`updated_count`, `releases_count`, `attention_count`, `no_change_count`, plus the `updated` / `new_releases` / `needs_attention` item lists) and `summary_today() -> DailySummary` returns the day's roll-up populated from `activity_records` and the cached state tables. E-06 consumes this shape.
2. The classification of `activity_records` rows into the tallies uses the `status` and `action_type` enums from `docs/internal/strategy-and-roadmap.md` Section 4.2. Mapping: an `(status='success', action_type IN ('pull_ff','pull','rebase'))` row counts as **updated**; an `(status='success', action_type IN ('check','fetch'))` row with no new commits (or `status='skipped'`) counts as **no-change**; a row with `status IN ('failed','warning')` (plus the state-table attention definition below) counts toward **attention**. New releases are detected from `repo_remote_meta.latest_release_at` falling in today's window.
3. The V1 `attention_count` is computed without E-07: count repos in `repo_local_state` with `last_error_code` set OR `is_dirty` set. The threshold-based "behind" semantics owned by E-07 are deferred (open question).
4. The implementation performs only read-only DB queries - no git, no network - and is cheap enough to call on demand.
5. `summary_week()` exists as a documented stub naming the V1.1 weekly aggregation, returning a not-implemented / empty result without breaking the build.
6. The daily aggregation is correct against the fixture/seed activity data (right counts, right release detection, right "no change" tallies).

## Acceptance criteria

- [x] AC1: E-11 owns and defines the `DailySummary` shape - `updated_count`, `releases_count`, `attention_count`, `no_change_count`, plus the `updated` / `new_releases` / `needs_attention` item lists - and `summary_today()` returns it aggregated from `activity_records` and the cached state tables. E-06 consumes this shape (circular ownership resolved in E-11's favor). Source: brief Section 4.4 (`summary_today() -> DailySummary`) and Section 6 (summary workstream).
- [x] AC2: The aggregation is read-only - no git operations, no network calls - and runs on demand cheaply. Source: brief Section 6 ("daily summary ... what makes the tray feel alive") and Section 3 (SHOULD: cheap to build).
- [x] AC3: Weekly aggregation is NOT implemented in V1; `summary_week()` is left as a documented stubbed seam for V1.1. Source: brief Section 4.4 (`summary_week()`/`WeeklySummary` cut to V1.1) and Section 3 (CUT to V1.1: weekly summary).
- [x] AC4: The daily counts (updated / releases / attention / no-change) are correct against seeded activity data, classified using the `status` and `action_type` enums and the (status, action_type) -> tally mapping: success + `pull_ff`/`pull`/`rebase` -> updated; success + `check`/`fetch` with no new commits (or `skipped`) -> no-change; `failed`/`warning` -> attention. Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (`status`/`action_type` enums), brief Section 6 (summary workstream) and Section 5.3 (the "Today's read" tallies the day with colored dots).
- [x] AC5: The V1 `attention_count` is computed without E-07's thresholds - count repos in `repo_local_state` with `last_error_code` set OR `is_dirty` set. The richer threshold-based attention semantics owned by E-07 are deferred and flagged as an open question. Source: V1 scoping decision to avoid a hard E-07 dependency (see open questions).

## Dependencies

- Upstream: E-09 (the `activity_records` the summary aggregates), transitively E-02 (the cached `repo_local_state` / `repo_remote_meta` and `activity_records` schema).
- Downstream: E-06 (consumes the `DailySummary` shape E-11 owns to generate the IPC type; the `WeeklySummary` seam is named here too); the Dashboard "Today's read" card and tray popup summary render the value (out of these efforts).
- Soft / deferred: E-07 (policy engine) owns the richer threshold-based attention semantics. V1's `attention_count` deliberately uses a simpler definition (`last_error_code` set OR `is_dirty`) that does NOT depend on E-07; aligning with E-07's thresholds is a deferred follow-up (open question).

## V1.1 extension points

- `summary_week() -> WeeklySummary`: the stubbed seam becomes a real weekly aggregation over the same `activity_records` / state data. This is the documented V1.1 extension; the seam exists in V1 so promoting it is additive.
- A persisted `summaries` archive (the "Summaries" nav area and Markdown export) that stores generated read-outs over time, rather than computing daily on demand.
- Richer narrative prose generation for the summary card; V1 produces the structured tallies, later work can shape the editorial copy.

## Open questions

- Whether `DailySummary` should carry pre-shaped prose or only the structured tallies (counts + item lists), leaving prose to the renderer. Default to structured tallies plus the item lists the card needs (updated repos, new releases, attention items), keeping prose generation in the UI layer; flag if the brief's editorial "Today's read" paragraph should be assembled backend-side instead. Source: brief Section 5.3 (the "Today's read" card mixes prose and tallies).
- **Attention definition vs. E-07's thresholds.** V1's `attention_count` uses a deliberately simple, E-07-free definition: count repos with `last_error_code` set OR `is_dirty` set. The policy engine (E-07) owns the richer "needs attention" semantics (e.g. behind-past-threshold, consecutive-failure / auto-pause state). FLAGGED: once E-07 lands, decide whether the summary should adopt E-07's threshold definition so the summary and the Repos view agree, or keep the simpler V1 definition. Until then, the two may diverge for repos that are behind-but-not-errored. Source: V1 scoping decision to avoid a hard E-07 dependency.
