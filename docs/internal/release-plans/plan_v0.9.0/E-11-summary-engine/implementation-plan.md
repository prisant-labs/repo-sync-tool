---
effort: E-11
plan_for: spec.md
status: ready
---

# E-11 Implementation Plan

## Approach

This is a thin, read-only aggregation layer over data E-02 and E-09 already produce, so the work is mostly SQL and shaping, not new machinery. Build `summary_today()` as a small set of grouped queries over `activity_records` for the current day plus a read of the cached state tables for the current attention count and new releases, then assemble the result into a `DailySummary`. Keep it cheap so it can be called on demand by the tray popup or Dashboard. Stub `summary_week()` as a named seam that returns a not-implemented/empty result, so E-06 can include it in the contract without it being live. Everything lives in `summary.rs` in `reposync-core`, stays Tauri-free, and is tested against seeded activity data.

## Steps

1. **Define the `DailySummary` shape (E-11 owns it).** In `summary.rs`, define the result value with concrete fields: `updated_count`, `releases_count`, `attention_count`, `no_change_count`, plus the three item lists `updated` (which repos updated and how), `new_releases` (which releases were detected), and `needs_attention` (which repos need attention). E-11 is the source of truth for this shape; E-06 consumes it to generate the IPC type (circular ownership resolved in E-11's favor). Keep it structured; prose assembly stays in the UI (spec open question). E-06 binds these field names to the transport; do not let E-06 redefine them.
2. **Define "today".** Decide the day boundary (local midnight vs. a rolling 24h) and apply it consistently; local-day is the natural fit for a "what changed today" read-out. Make the boundary injectable for deterministic tests (reuse the injected-clock idea from the scheduler if available, else pass the day window in).
3. **Aggregate the day's activity.** Query `activity_records` for today's rows, grouped by `action_type` / `status` (the enums from `docs/internal/strategy-and-roadmap.md` Section 4.2), to produce the tallies and the per-repo update items. Apply the (status, action_type) -> tally mapping: `status='success'` with `action_type IN ('pull_ff','pull','rebase')` counts as **updated**; `status='success'` with `action_type IN ('check','fetch')` and no new commits, or `status='skipped'`, counts as **no-change**; `status IN ('failed','warning')` contributes to **attention**. Use the `(timestamp DESC)` index. This is read-only (AC2).
4. **Detect new releases today.** Read `repo_remote_meta` for releases whose `latest_release_at` falls in today's window (cross-referenced with activity where useful) to produce the "new releases" list. Read-only.
5. **Compute the current attention count (E-07-free V1 definition).** Read `repo_local_state` for repos currently needing attention using the V1 definition that does NOT depend on E-07's thresholds: count repos with `last_error_code` set OR `is_dirty` set, to fill the attention tally and item list. The richer "behind past threshold" / auto-pause semantics that E-07 owns are deferred (open question); do not re-derive E-07's thresholds here. Read-only.
6. **Assemble and return `DailySummary`.** Combine the tallies and item lists into one value. No git, no network anywhere in the path (AC2).
7. **Stub `summary_week()`.** Add `summary_week()` returning a documented not-implemented/empty `WeeklySummary` (or equivalent), with a doc comment marking it the V1.1 weekly-aggregation extension point (AC3). It must compile and be nameable by E-06 without doing real work.
8. **Verify.** Run the tests below against seeded activity + state data; confirm the daily tallies and item lists are correct and that `summary_week()` is inert.

## Test strategy

- **Daily aggregation correctness.** Seed `activity_records` and the cached state tables for a known day with a mix of updates (`success` + `pull_ff`/`pull`/`rebase`), fetch-no-change (`success` + `check`/`fetch`, or `skipped`), a `failed`/`warning` row, a release, and an attention item (`last_error_code` set or `is_dirty`); call `summary_today()` and assert each tally and item list matches the seed per the (status, action_type) mapping (AC1, AC4, AC5).
- **Day-boundary test.** Seed records straddling the day boundary (just before and just after midnight) and assert only today's are counted, using the injected day window for determinism.
- **Read-only guarantee.** Assert the path issues no git/network calls (structurally - the module has no such dependencies) and only reads the DB; a test that runs `summary_today()` against a read-only connection or asserts no rows are written confirms AC2.
- **Empty-day test.** With no activity today, assert the summary returns zeroed tallies and empty lists (the calm "nothing changed" state), not an error.
- **Weekly stub test.** Assert `summary_week()` is callable and returns the documented not-implemented/empty result without panicking (AC3).
- All run in `cargo test` against a tempdir SQLite migrated by E-02 and seeded as E-09 would write; no Tauri host.

## Files / modules touched

- `crates/reposync-core/src/summary.rs` (the daily aggregation + the weekly stub; replaces the E-01 stub).
- `crates/reposync-core/src/lib.rs` (export the summary API if not already).
- Coordination note for E-06: E-11 OWNS the `DailySummary` field names (`updated_count`, `releases_count`, `attention_count`, `no_change_count`, `updated`, `new_releases`, `needs_attention`) and the `WeeklySummary` seam name; E-06 consumes them to generate the IPC type. No E-06 code is written in this effort.
- Tests under `crates/reposync-core` (seeded-data aggregation tests).

## Risks and mitigations

- **Day-boundary ambiguity.** Local vs UTC midnight changes which records count. Mitigate by fixing local-day semantics and making the window injectable so tests are deterministic and the choice is explicit.
- **"New release today" double-counting.** A release tag cached on `repo_remote_meta` could be counted on multiple days if keyed by fetch time rather than release time. Mitigate by keying the release window on `latest_release_at` (the release's own date), not the fetch time.
- **Drift from the Dashboard's attention semantics.** V1 deliberately uses a simple, E-07-free attention definition (`last_error_code` set OR `is_dirty`), so the summary and a threshold-aware Repos view may diverge for repos that are behind-but-not-errored. This is an accepted V1 tradeoff to avoid a hard E-07 dependency, flagged as an open question; once E-07 lands, decide whether to adopt its threshold definition so the two agree.
- **Scope creep into weekly.** Weekly is explicitly cut; resist implementing it. The stub is the deliverable, not a partial weekly.

## Definition of done

All five acceptance criteria checked, the aggregation tests green in `cargo test` on Windows and in CI, `summary_today()` returns correct tallies/items read-only using the E-07-free V1 attention definition and the (status, action_type) enum mapping, `summary_week()` left as a documented inert stub, the `DailySummary` field names owned here and consumed by E-06, `reposync-core` still has no `tauri` in its tree, and the branch ready for self-merge per `EXECUTION.md`.
