---
effort: E-17
plan_for: spec.md
title: Branch and PR Intelligence - implementation plan
status: ready
---

# E-17 - Branch and PR Intelligence - Implementation Plan

## Approach

Core-first, in three ordered layers, so the risky and testable work lands before any UI and before the frozen contract is touched.

1. **Pure core in `reposync-core`, behind the existing E-10 (GitHub metadata client) seams, test-first.** The pull-request-count fetch is one more branch in the `Transport` boundary; its JSON-to-count mapping, its own-ETag cache decision, and its 404/403-as-Unknown discipline are pure functions over a fake transport and fixtures - no live GitHub, no HTTP-mock dependency. Local recency (`last_local_commit_at`) is a git2 read plus a persist, testable against a real fixture repo. This layer changes no wire types and touches no Tauri code, so it can land and be reviewed in isolation with the E-01 hygiene gate green.
2. **The additive E-06 (IPC contract) amendment + `bindings.ts` regen.** Only after the core produces the data does the contract gain the `Option` fields to carry it, following the E-16 (groups) precedent exactly. This is a mechanical, gated step (the stale-`bindings.ts` CI check proves it).
3. **The UI**, last, against the now-live typed surface: the repo-row badge, the dashboard attention view, and the drawer section - each building on the Phase 1 fixes of audit findings #10 (dashboard attention blanket-red) and #11 (drawer staleness), never around them.

The durable properties to protect: `reposync-core` stays Tauri-free; the unauthenticated path stays structurally the only path (`NoToken`); the PR sub-resource never destructively overwrites a cached count on an ambiguous 404 (the BL-NI-15a lesson); the row/list badge is O(1) per the existing `repo_list` query, never a per-repo fan-out (the BL-NI-22 lesson); and aggregate unauthenticated GitHub usage stays under the 60/hour ceiling by construction, governed by the hard request budgeter.

## Preconditions (verify before starting the remote layer)

- E-10 core is built (`refresh_one`, `Transport`, `TokenProvider`, ETag/backoff present). Confirmed in `crates/reposync-core/src/github.rs`.
- **BL-NI-15b (E-10 release-ETag 304 short-circuit) is resolved** - the release endpoint has its own ETag and durable staleness state. Until then, wiring the remote refresh is blocked (the release dimension is untrustworthy to wire, per BL-NI-15's own note). The local-intelligence layer (recency) and the pure PR-fetch logic can proceed against the fake transport regardless; only the live wiring waits.
- Audit findings #10 and #11 are fixed in Phase 1 (the corrected dashboard attention taxonomy and the event-subscribed drawer). E-17's UI extends the corrected surfaces.

## Ordered steps

1. **Local recency population (core, test-first).** In the git2 inspect path (`crates/reposync-core/src/git/inspect.rs` and the check/update persist in `crates/reposync-core/src/repo.rs`), read the HEAD commit's committer time and persist it to `repo_local_state.last_local_commit_at` (the column exists since migration 0001 but is never written). Write the fixture test first: a repo whose HEAD commit has a known time yields that `last_local_commit_at` after a check. No schema change, no wire change yet. (AC2.)
2. **Migration 0005 (E-02-owned, additive).** Add `open_pr_count INTEGER`, `default_branch_pr_count INTEGER`, `pr_etag TEXT`, `pr_last_checked_at INTEGER` to `repo_remote_meta`, following the additive pattern of 0002/0003. Coordinate with E-02 (persistence) as the schema owner. The additive-migration sequence is coordinated across P1-C (0004, the Phase 1 fix for migration 0001's stale `check_frequency_min` default, BL-NI-34), E-17 (0005, this effort), and E-18 (0006, auto-update and distribution). If BL-NI-15b lands its `release_etag` / `release_last_checked_at` columns in the same push, co-locate them in 0005. (Supports AC3/AC4.)
3. **PR-count fetch behind the `Transport` seam (core, test-first).** Extend the fetch so a 200 repo refresh also issues the pulls read for the count and the default-branch subset count, sending the stored `pr_etag` as `If-None-Match` for that endpoint, under the SAME auth context as the repo request (the BL-NI-15a private-repo lesson). Model the result as a tri-state mirroring `ReleaseState`: `Found(counts)` / `KnownZero` / `Unknown`, where a 404/403 under the unauthenticated context is `Unknown` (preserve), never zero. Map `Link`-header last-page to the count (see the spec open question on cheap vs precise). Write the fake-transport tests first: 200-with-PRs writes counts; 304 on the pulls endpoint bumps only `pr_last_checked_at`; a 404/403 preserves the cached count; a network error preserves the row. (AC3, AC4, AC5.)
4. **Own-ETag cache + backoff for the PR endpoint (core).** The PR read reads/writes `pr_etag` and `pr_last_checked_at`, decoupled from `repo_remote_meta.etag`, so a repo-304 does not gate PR freshness (AC4). Surface the observed `RateLimit` from the PR read into the existing `RefreshReport` so the refresh-pass orchestrator can call `should_backoff`; a rate-limited PR read carries `reset_at` (AC8). Reuse `should_backoff` and the `RATE_LIMIT_BACKOFF_PERCENT` constant; add no second backoff policy.
5. **Request budgeter (core, test-first, injected clock).** A hard request budgeter in `reposync-core` caps aggregate unauthenticated GitHub usage at 30 requests per rolling hour, shared across E-10 (GitHub metadata client) enrichment and the E-17 PR fetch, leaving headroom under the real 60/hour ceiling. It is pure core logic over an injected clock (the E-08 (scheduler) clock seam), so a rolling-hour window is deterministically testable without wall-clock waits. The E-08 refresh pass consults the budgeter and refreshes oldest-metadata-first, round-robin, so a cold 100-repo backfill spreads over several hours by design rather than bursting; on exhaustion the pass stops issuing requests and repos keep their last-known values with a staleness timestamp (never an error state). Write the injected-clock tests first: a 100-repo backfill reaches full coverage yet never exceeds 60 requests in any rolling hour and surfaces no rate-limit error. (AC16, AC8.)
6. **Wire the remote refresh (src-tauri, rides the E-10 wiring seam).** The `repo_refresh_metadata(id) -> RepoDetail` command shell (E-06) and the E-08 scheduler refresh cadence call the enriched `refresh_one`, so a refreshed `RepoDetail` now carries PR counts too. This is the wiring BL-NI-15 defers to "the E-10 wiring effort"; E-17 is that effort for the branch/PR dimension. Gated on the BL-NI-15b precondition. Keep the write path transactional (the BL-NI-15a atomicity property: freshness markers cannot advance unless the PR write is durable).
7. **E-06 additive amendment + regen (steps for the contract).** Add the `Option` fields to `RepoSummary` (`open_pr_count`, `last_local_commit_at`) and `RepoDetail` (`open_pr_count`, `default_branch_pr_count`) in `crates/reposync-core/src/ipc.rs`. Fold `open_pr_count` and `last_local_commit_at` into the `repo_list` store query's join (O(1), the BL-NI-22 lesson), and into the `repo_get` join. Regenerate `src/lib/bindings.ts`; the stale-check CI gate proves it. No new command, no new event in V1. (AC12, AC14.)
8. **Repo-row badge (frontend).** In `src/screens/repos.tsx` (`RepoRow`), add a PR/release badge in the signal register - the release token/`package` icon and a PR count chip - using the existing `STATUS_STYLE`/signal tokens, never the status-taxonomy colors (AC9). Data comes from the now-additive `RepoSummary` fields; no new per-repo call.
9. **Dashboard attention view (frontend).** In `src/screens/dashboard.tsx`, render each `attention` item with its true status by joining the item's `repoId` against the already-fetched `useRepoList(ALL_FILTER)` data and calling `deriveStatus` / `STATUS_STYLE` (the same taxonomy the Repos view uses), replacing the single hardcoded `AlertTriangle text-status-failed` icon. Add branch/PR context to the item detail. This reuses the corrected Phase 1 surface (finding #10) and needs no `SummaryItem` schema change. (AC10.)
10. **Drawer section (frontend).** In `src/components/repo-detail.tsx`, add a "branch and PR intelligence" block (ahead/behind, open PRs, PRs to default, latest release, recency) that reads from `repo_get`'s `RepoDetail` and refreshes on backend events (the Phase 1 finding-#11 event subscription), not only on its own actions. (AC6, AC11.)
11. **Degradation pass (frontend + core).** Verify each degraded path renders correctly: non-GitHub repo shows local intel only and "unavailable" for remote; offline/rate-limited shows last-known counts with a staleness marker; private-unauthenticated shows Unknown (not zero). (AC7, AC8, AC5.)
12. **Verify.** Full core tests green on Windows and CI; `cargo tree -p reposync-core | grep -i tauri` stays empty; scoped `tsc` + `pnpm build` green; regenerated `bindings.ts` in sync (stale-check gate); a dogfood pass (Phase 2) confirms the real per-pass request cost against a realistic library.

## Test strategy

- **Local recency (fixture).** A git2 fixture repo with a known HEAD commit time -> assert `last_local_commit_at` is persisted after a check, and is `None`/untouched where inspect fails. Real git fixtures, in the slow-tier bucket (the CI plan tiers fast unit vs slow git-fixture tests).
- **PR-fetch mapping + tri-state (fake transport, pure).** Feed canned pulls responses through the mapper: assert the open count and the default-branch subset count map correctly; assert `Found` / `KnownZero` / `Unknown` classification; assert an `Unknown` (404/403) preserves the cached counts while a 200 rewrites them - mirroring the E-10 `refresh_200_release_unknown_preserves_cached_release` and `_known_none_clears` tests. Pure, fast tier, no network.
- **Own-ETag / 304 (fake transport).** Assert the stored `pr_etag` is sent as `If-None-Match` on the pulls endpoint, that a PR-endpoint 304 bumps only `pr_last_checked_at` and leaves counts intact, and that a repo-resource 304 does NOT suppress a PR refresh (the BL-NI-15b decoupling) - the regression test that proves AC4.
- **Rate-limit surfacing.** Drive a near-exhausted budget on the PR read; assert the observed `RateLimit` reaches the caller via `RefreshReport` so `should_backoff` fires, and a rate-limited outcome carries `reset_at`. Mirrors `refresh_surfaces_rate_limit_budget_for_backoff`.
- **Request budget (injected clock, 100-repo).** Drive a 100-repo library against a mock GitHub server with an injected clock: assert the budgeter never lets aggregate usage exceed 60 requests in any rolling hour, that full PR-intelligence coverage is still reached (spread over multiple passes), and that no rate-limit error reaches the UI while stale repos render with their as-of timestamp. Proves AC16. Pure and deterministic via the injected clock, fast tier, no live network.
- **Contract round-trip.** Extend the `ipc.rs` `payloads_round_trip_losslessly` test to include the new `RepoSummary` / `RepoDetail` fields, guarding the camelCase wire shape.
- **Degradation.** Unit-cover non-GitHub skip (`Skipped`, zero transport calls), network-lost preserve, and the private-repo Unknown path; a frontend smoke (manual or via the webapp-testing harness) for the "unavailable" / staleness-marker rendering.
- **UI taxonomy.** Manual/visual check (Phase 2 dogfood) that the dashboard attention items render behind/dirty/failed in the correct taxonomy colors and the row badge uses the signal register, not status colors.

## Files / modules touched

- `crates/reposync-core/src/github.rs` - the PR-count fetch behind `Transport`, the PR tri-state, the own-ETag cache decision, the `RefreshReport` extension; the production `ReqwestTransport` gains the pulls read (unauthenticated, same auth context). Not exercised by unit tests (the seam is faked). The request budgeter (rolling-hour cap over an injected clock, shared with E-10 enrichment traffic) also lives in core here (or a small sibling module); it is pure and unit-tested.
- `crates/reposync-core/src/git/inspect.rs` + `crates/reposync-core/src/repo.rs` - read + persist HEAD committer time into `last_local_commit_at`.
- `crates/reposync-core/src/store.rs` - fold `open_pr_count` + `last_local_commit_at` into the `repo_list` and `repo_get` joins (O(1), no fan-out).
- `crates/reposync-core/src/ipc.rs` - additive `Option` fields on `RepoSummary` / `RepoDetail`; extend the round-trip test.
- `crates/reposync-core/migrations/0005_branch_intel.sql` - additive `repo_remote_meta` PR columns (E-02-owned; coordinate; 0004 is P1-C, 0006 is E-18).
- `src-tauri/src/commands/mod.rs` - the `repo_refresh_metadata` shell calls the enriched core (no new command); the E-08 scheduler refresh path wires the cadence.
- `src/lib/bindings.ts` - regenerated (gated by the stale-check CI).
- `src/screens/repos.tsx`, `src/screens/dashboard.tsx`, `src/components/repo-detail.tsx` - the row badge, the attention-view taxonomy join, the drawer section.
- `src/lib/status.ts` - reused (no change) for the attention-view `deriveStatus` join; a signal-badge helper may be added here.

## Risks and mitigations

- **Unauthenticated 60/hour ceiling under a large library.** Up to three requests per repo per pass (repo + release + PRs), which dead-ends near 20 repos if left ungoverned. Mitigate primarily with the hard request budgeter (step 5): a 30-requests-per-rolling-hour aggregate cap plus oldest-metadata-first round-robin refresh, so a 100+-repo library backfills over several hours by design and never bursts past the ceiling. The 24h refresh clock, per-endpoint ETag 304s, and `should_backoff` keep steady-state volume low as further backstops; confirm the real per-pass cost during Phase 2 dogfood. This is the BL-NI-22 fan-out lesson at the network layer: never an avoidable round-trip per repo per surface.
- **Private repo, unauthenticated, reads as "0 PRs".** The central correctness trap. A 404/403 is Unknown (preserve), never zero - the BL-NI-15a discipline, enforced by the tri-state type so an untrusted count is unrepresentable, and by sending the PR request the same auth context as the repo request.
- **Repo-ETag gating PR freshness (a repeat of BL-NI-15b).** Avoided by giving the PR endpoint its own `pr_etag` / `pr_last_checked_at`; the decoupling is proven by a dedicated regression test.
- **Wiring the remote refresh before BL-NI-15b lands.** Blocked by design: the remote-wiring step (step 5) is gated on the BL-NI-15b precondition. The pure PR logic and the local recency work do not wait.
- **Per-repo fan-out for badges.** Avoided by folding the badge fields into the existing `repo_list` join; no `groups_for_repo`-style per-row call (the BL-NI-22 anti-pattern).
- **Badge color bleeding into the status taxonomy.** The PR/release badge uses only the signal register (release token / `package`); a review check (and the DESIGN.md Status-Owns-Saturation rule) guards it.
- **`open_issues_count` conflation** (if the cheap fallback is used): it counts issues plus PRs, so it is an approximation, not a PR count. Default to the dedicated pulls read; document clearly if the fallback is chosen.

## Definition of done

- All sixteen acceptance criteria met.
- Ahead/behind and dirty surfaced (not recomputed); `last_local_commit_at` populated and shown.
- Open-PR count and default-branch PR count fetched unauthenticated behind the `Transport` seam, with their own ETag, honoring backoff; a 404/403 preserves (never zeroes) the cached counts.
- The request budgeter caps aggregate unauthenticated GitHub usage under the 60/hour ceiling; a 100-repo library reaches full PR-intelligence coverage over several passes with no rate-limit error surfaced and stale repos shown with an as-of timestamp, proven under an injected clock (AC16).
- The pure PR logic is fixture-/fake-transport tested with no live GitHub calls; `reposync-core` still has no `tauri` in its dependency tree.
- The E-06 amendment is additive-only, `bindings.ts` regenerated and in sync (stale-check gate green), following the E-16 groups precedent.
- The row badge (signal register), the dashboard attention view (true taxonomy, building on finding #10), and the event-subscribed drawer section (building on finding #11) ship and degrade gracefully offline / rate-limited / non-GitHub / private-unauthenticated.
- BL-NI-15b confirmed resolved before the remote wiring landed.
- Full local gate green (scoped `cargo -p reposync-core`, scoped `tsc`/`pnpm build`, and a full sweep), a Phase 2 dogfood pass on a real library confirms the per-pass request cost, and the branch is ready per `EXECUTION.md`.
