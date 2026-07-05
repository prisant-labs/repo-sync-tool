---
effort: E-17
title: Branch and PR Intelligence
status: ready
tier: SHOULD
scope: V1 (v0.9.0)
depends_on: [E-10, E-06, E-02, E-08, E-11]
tracking-issue: "#19"
source: docs/internal/product-requirements.md Section 4 capability table, row: Branch and PR intelligence; DESIGN.md Section 2 (status taxonomy); docs/internal/release-plans/plan_v0.9.0/E-10-github-client/spec.md
---

# E-17 - Branch and PR Intelligence

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started. New SHOULD-tier feature, jp-ratified 2026-07-04, scoped into v0.9.0 (context pack `_LOCAL/plans/2026-07-04_18-25_ship-plan-context-pack.md`). No tracking issue yet (the repo stays private through v0.9.0; the release plan is the tracker).
- **Next:** land the core-first work in `reposync-core` behind the E-10 (GitHub metadata client) `Transport` seam - the pull-request-count fetch, its own ETag cache, and the `last_local_commit_at` population - test-first with fixtures and a fake transport, then the additive E-06 (IPC contract) amendment and `bindings.ts` regen, then the UI (row badge, dashboard attention view, drawer section).
- **Blockers / preconditions:** E-10 core done (built, `refresh_one` present); **BL-NI-15b (E-10 release-ETag 304 short-circuit) must be resolved before the remote wiring lands** - the release dimension is not trustworthy to wire until the release endpoint gets its own ETag and durable staleness state. The `repo_refresh_metadata` command shell (E-06 / src-tauri) and the scheduler refresh cadence (E-08) are the wiring this effort rides on; audit findings #10 (dashboard attention rows all render failed-red) and #11 (drawer goes stale on background completion) are Phase 1 fixes this effort builds on top of, not around.

## Context

E-17 gives the user repo situational awareness beyond freshness. Today RepoSync answers "is this checkout behind / dirty / failing?"; E-17 adds "what is going on with this repo" - how open the upstream is (open pull requests, pull requests aimed at the default branch), whether a new release shipped, and how stale the local checkout's own HEAD is. For a consume-only library of tools you track but do not actively develop, that is the difference between "my copy is current" and "the project is active and here is what changed."

The feature has two halves that already have most of their machinery in place, so E-17 is mostly assembly and one new fetch, not new architecture.

**Local intelligence is largely already built.** The git engine (E-03) already inspects and the check path (E-02/E-03, `crates/reposync-core/src/repo.rs`) already persists ahead/behind vs upstream (`repo_local_state.ahead_count` / `behind_count`, computed by `compute_ahead_behind` via a local rev-list, no network) and dirty state (`is_dirty`). Both are already exposed on `RepoSummary` and `RepoDetail`. What is NOT yet populated is **last-local-commit recency**: the column `repo_local_state.last_local_commit_at` exists (migration 0001) but nothing writes it - the inspect path never reads the HEAD commit time. E-17 fills that gap (read HEAD committer time via git2, persist it) and surfaces it. So local intelligence for E-17 is: expose ahead/behind + dirty that already exist, and add recency population + surfacing.

**Remote intelligence extends the E-10 client.** E-10 (GitHub metadata client, `crates/reposync-core/src/github.rs`) already fetches description, default branch, latest release (tag / date / URL), topics, and the archived flag, unauthenticated, with ETag conditional requests, a ~24h refresh clock, and rate-limit backoff - all behind a mockable `Transport` seam and a `TokenProvider` seam whose V1 impl returns `None`. E-17 adds one thing the client does not do: **pull-request counts** (open PRs, and PRs targeting the default branch), fetched behind the same seam, unauthenticated, honoring ETag and the rate-limit backoff. The latest release E-10 already caches is surfaced by E-17's UI rather than re-fetched. The PAT / authenticated path stays a V1.1 seam (BL-V11-02, keyring PAT); E-17 runs the unauthenticated 60/hour path only.

Two load-bearing lessons from the E-10 hardening carry directly into E-17. First, BL-NI-15a (E-10 release/cache tri-state): a 404 on a sub-resource fetched under the unauthenticated context is **ambiguous** - it can mean "genuinely none" or "private / inaccessible" - so it must be `Unknown` (preserve the cached value), never a destructive "0". E-17's PR fetch reuses that discipline exactly: a 404/403 on the pulls endpoint never overwrites a cached PR count with zero. Second, BL-NI-15b (E-10 release-ETag 304 short-circuit): the repo-resource ETag must not gate a sub-resource's freshness, or a repo-304 hides a change in the sub-resource for the whole 24h window. E-17's PR data therefore gets its **own** ETag and last-checked tracking, decoupled from the repo-resource ETag.

## In scope

### (a) Local intelligence (git2 reads)

- **Confirm and surface existing state.** Ahead/behind vs upstream (`ahead_count` / `behind_count`) and dirty state (`is_dirty`) are already computed and persisted by the check path and already on `RepoSummary` / `RepoDetail`. E-17 does not recompute them; it surfaces them as first-class situational-awareness signals in the row and drawer.
- **Add last-local-commit recency.** Read the HEAD commit's committer time via git2 during inspect and persist it to the existing `repo_local_state.last_local_commit_at` column (currently never written). Expose it additively on `RepoSummary` (it is already declared on `RepoDetail`). This is "how stale is my local checkout's HEAD", distinct from `last_checked_at` ("when did RepoSync last look").

### (b) Remote intelligence (E-10 unauthenticated client)

- **Open pull-request count** for GitHub repos, fetched behind the E-10 `Transport` seam, unauthenticated, honoring ETag conditional requests and the rate-limit backoff.
- **Pull requests targeting the default branch** (a subset count), using the default branch E-10 already caches (`repos.default_branch`).
- **Latest release surfacing.** The release E-10 already caches (`latest_release_tag` / `latest_release_at` / `latest_release_url`) is rendered by E-17's UI; E-17 does not re-fetch it.
- **Own cache tracking for PR data.** The PR fetch carries its own ETag and last-checked timestamp (new `repo_remote_meta` columns), decoupled from the repo-resource ETag, so a repo-304 never hides a new PR (the BL-NI-15b lesson).
- **Rate-limit and backoff discipline.** The PR fetch counts against the same unauthenticated 60/hour budget; it surfaces the observed `RateLimit` so the refresh-pass orchestrator can call the existing `should_backoff`, and a rate-limited outcome carries an honest `reset_at`.
- **Request budget (the unauthenticated 60/hour ceiling).** A hard request budgeter in `reposync-core` caps GitHub API usage at 30 requests per rolling hour, leaving headroom for E-10 (GitHub metadata client) enrichment traffic, which shares the same unauthenticated budget. PR metadata costs at most one request per repo per pass. Refresh is spread oldest-metadata-first, round-robin, driven by the E-08 (scheduler) tick, so a cold 100-repo backfill completes over several hours BY DESIGN rather than bursting. On budget exhaustion or a 403 rate-limit response, repos retain their last-known values with a staleness timestamp and the UI renders "as of <time>", never an error state. The V1.1 PAT path (already stubbed in E-10) lifts the ceiling to 5000/hour and is the documented escape hatch. This is what makes a 100+-repo library viable on the unauthenticated path, rather than dead-ending near 20 repos.

### (c) UI surfaces

- **Repo-row badge** (Repos list): a compact signal for open PRs and latest release, rendered in the DESIGN.md **signal register** (release = magenta `package`), never in the status-taxonomy colors, so PR/release info never masquerades as sync status. Badge data rides in the existing `repo_list` query (additive `RepoSummary` fields), not a per-repo fan-out.
- **Dashboard "Needs attention" view**: extends `src/screens/dashboard.tsx` so each attention item renders with its TRUE per-repo status per the taxonomy (behind = violet arrow-down, dirty = amber triangle, failed = red x-circle), building on the Phase 1 fix of audit finding #10 (attention rows all render failed-red). Branch/PR context (for example "14 behind", "3 PRs to main") is added as the item detail.
- **Drawer detail section** (`src/components/repo-detail.tsx`): a "branch and PR intelligence" block showing ahead/behind, open PRs (and PRs to the default branch), latest release, and last-commit recency; it refreshes on background check/refresh completion by subscribing to backend events, building on the Phase 1 fix of audit finding #11 (drawer goes stale on background completion).

### (d) Graceful degradation

- **Non-GitHub remotes:** local intel (ahead/behind, dirty, recency) still computed for any git remote; remote intel (PRs, release) reports "unavailable" and is skipped, never an error (`parse_github_coords` returns `None` -> `RefreshOutcome::Skipped`).
- **Offline / network-lost:** the cached PR/release counts are preserved and shown with a staleness marker; no row is wiped, no error is spammed (`FetchOutcome::NetworkLost` leaves the row intact).
- **Rate-limited:** the last-known counts are shown; the client backs off and surfaces `AppError::RateLimited { reset_at }` honestly rather than continuing to spend the budget.
- **Private repo, unauthenticated:** a 404/403 on the PR fetch is `Unknown` (preserve the cached count), never "0 PRs" - the BL-NI-15a discipline.

## Out of scope

- **The authenticated PAT path** (keyring-backed token, the 5000/hour ceiling): stays the V1.1 seam (BL-V11-02, keyring PAT). E-17 runs `NoToken` unauthenticated only.
- **PR bodies, reviews, checks, merge actions, or any write to GitHub.** E-17 reads counts and the default-branch subset; it never opens, comments on, or merges a PR. RepoSync is not a Git client (DESIGN.md anti-positioning; PRODUCT.md).
- **Per-PR detail lists** (author, title, status per PR). V1 ships counts; a full PR list is a V1.1 surface behind the same seam.
- **A durable release-detected / PR-detected event history** (an immutable per-(repo, tag/PR, first-seen) source). That is BL-NI-16 (E-11 daily-release fidelity needs a release-event source) territory, coupled to the E-10 wiring effort; E-17 ships the mutable snapshot approximation, consistent with E-11's V1 posture.
- **The `repo_remote_meta` schema itself.** E-02 (persistence) owns the table and the migration; E-17 specifies the additive columns it needs (see the amendment section) and coordinates the migration, it does not own the DDL.
- **Non-GitHub hosts** (GitLab, self-hosted): the parse seam already skips them cleanly; extending remote intel to other hosts is a V1.1 seam extension.

## Confirm: what already exists vs what is new

| Signal | Status | Where |
|---|---|---|
| Ahead / behind vs upstream | EXISTS (computed + persisted + on Summary/Detail) | `repo.rs::compute_ahead_behind`, `repo_local_state.ahead_count/behind_count`, `ipc::RepoSummary`/`RepoDetail` |
| Dirty state | EXISTS (persisted + surfaced) | `repo_local_state.is_dirty`, `ipc::RepoSummary`/`RepoDetail` |
| Last-local-commit recency | NEW population (column exists, never written) | `repo_local_state.last_local_commit_at` (migration 0001), git2 inspect must read HEAD committer time |
| Latest release (tag/date/url) | EXISTS (fetched + cached by E-10) | `github.rs::refresh_one`, `repo_remote_meta.latest_release_*` |
| Open PR count | NEW fetch | E-17 extends the E-10 `Transport` fetch |
| PRs targeting default branch | NEW fetch | E-17, using cached `repos.default_branch` |
| PR ETag / last-checked cache | NEW columns | `repo_remote_meta` migration 0005 (additive) |

## IPC contract amendment (E-06)

The IPC contract is frozen; additions follow the documented additive path. **Precedent:** the E-16 (groups) feature amended the contract additively - it added `GroupSummary` to `crates/reposync-core/src/ipc.rs`, added `group_list` / `group_create` / `group_rename` / `group_delete` / `group_assign` / `group_unassign` / `groups_for_repo` as `#[tauri::command] #[specta::specta]` functions in `src-tauri/src/commands/mod.rs`, registered them in `collect_commands![...]` in `src-tauri/src/lib.rs`, and regenerated `src/lib/bindings.ts`. The E-06 spec blesses this: "Adding a command, an event, or a payload field is an **additive** contract revision ... regenerating `bindings.ts`. Removing or renaming one is a deliberate breaking change that the stale-`bindings.ts` CI check makes loud" (E-06 spec, V1.1 extension points). E-17 follows that path exactly.

**E-17's additions (all additive; no rename, no removal):**

1. **Payload fields (`crates/reposync-core/src/ipc.rs`):**
   - `RepoSummary`: add `open_pr_count: Option<i64>` and `last_local_commit_at: Option<i64>` (both `Option`, so an un-refreshed or non-GitHub repo is a clean `None`). `latest_release_tag` is already present.
   - `RepoDetail`: add `open_pr_count: Option<i64>` and `default_branch_pr_count: Option<i64>`. `last_local_commit_at`, `latest_release_at`, and `latest_release_url` are already present.
2. **No new command required for V1.** The existing `repo_refresh_metadata(id) -> RepoDetail` (E-06, currently the UNWIRED E-10 wiring seam per BL-NI-15) is the refresh entry point; E-17 folds the PR fetch into its core (`refresh_one`) so the same command returns the enriched `RepoDetail`. If a distinct on-demand PR refresh is ever wanted, `repo_refresh_pr_intel(id)` would follow the same additive command path; V1 does not add it.
3. **No new event required for V1.** The row and drawer refetch on the existing `repo:check-completed` event and on manual refresh. A `repo:metadata-refreshed` event (to push a background remote refresh to the UI) is an additive option flagged for V1.1, not built here.
4. **Schema (E-02-owned, additive migration 0005):** `repo_remote_meta` gains `open_pr_count INTEGER`, `default_branch_pr_count INTEGER`, `pr_etag TEXT`, `pr_last_checked_at INTEGER`, following the additive-migration pattern of 0002 (activity/settings) and 0003 (cadence inherit). The additive-migration sequence is coordinated across three efforts: 0004 (P1-C, the Phase 1 fix for migration 0001's stale `check_frequency_min` default, BL-NI-34), 0005 (E-17, this branch and PR intelligence effort), and 0006 (E-18, auto-update and distribution). If BL-NI-15b lands its `release_etag` / `release_last_checked_at` columns in the same remote-wiring push, migration 0005 may co-locate them; E-17 treats BL-NI-15b as an upstream precondition regardless.

The stale-`bindings.ts` CI gate (E-06 AC6) proves the regen happened; every added field is `Option`, so no existing consumer breaks.

## Acceptance criteria

- [ ] AC1: Ahead/behind vs upstream and dirty state (already persisted in `repo_local_state.ahead_count` / `behind_count` / `is_dirty` and already on `RepoSummary` / `RepoDetail`) are surfaced as first-class branch-intelligence signals in the repo row and drawer; E-17 does not recompute them. Source: product-requirements.md Section 4 capability table, row: Branch and PR intelligence; `repo.rs` check path; `ipc.rs`.
- [ ] AC2: The HEAD commit's committer time is read via git2 during inspect and persisted to `repo_local_state.last_local_commit_at` (currently never written), and exposed additively on `RepoSummary`; last-commit recency is shown distinct from last-checked. Source: product-requirements.md; migration 0001 (`last_local_commit_at` column); `git/inspect.rs`.
- [ ] AC3: For GitHub repos, the client fetches an open pull-request count and a count of pull requests targeting the default branch, unauthenticated, behind the E-10 `Transport` seam, sending the stored PR ETag as `If-None-Match` and honoring the rate-limit backoff. Source: product-requirements.md; E-10 spec (AC3 ETag, AC4 rate-limit, `Transport` seam).
- [ ] AC4: The PR fetch has its OWN ETag and last-checked tracking (`repo_remote_meta.pr_etag` / `pr_last_checked_at`), decoupled from the repo-resource ETag, so a repo-resource 304 never suppresses a new pull request for the 24h window. Source: BL-NI-15b (release-ETag 304 short-circuit); E-10 spec.
- [ ] AC5: A 404 or 403 on the PR fetch under the unauthenticated context is treated as Unknown and PRESERVES the cached PR counts; it is never written as "0 PRs" - a private or inaccessible repo must not be reported as having zero pull requests. Source: BL-NI-15a (E-10 `ReleaseState::Unknown` discipline); `github.rs`.
- [ ] AC6: The latest release already cached by E-10 (`latest_release_tag` / `latest_release_at` / `latest_release_url`) is surfaced in the repo row and drawer; E-17 does not re-fetch it. Source: E-10 spec AC1; features-and-outcomes.md (GitHub enrichment).
- [ ] AC7: Non-GitHub remotes still get local intelligence (ahead/behind, dirty, recency); remote intelligence (PRs, release) reports "unavailable" and is skipped as `RefreshOutcome::Skipped`, never an error. Source: `github.rs::parse_github_coords`; product-requirements.md (Degradation requirements).
- [ ] AC8: Offline (`NetworkLost`) and rate-limited outcomes preserve the last-known cached counts and surface them with a staleness marker; a rate-limited outcome carries an honest `AppError::RateLimited { reset_at }` and backs off rather than spending the budget. Source: E-10 `github.rs` (`FetchOutcome::NetworkLost` / `RateLimited`, `should_backoff`); product-requirements.md.
- [ ] AC9: The repo-row PR/release badge renders in the DESIGN.md signal register (release = magenta `package` icon/token), NOT the status-taxonomy colors, honoring the Status-Owns-Saturation rule so PR/release info never reads as sync status. Source: DESIGN.md Section 2 (status taxonomy; "release is a signal color ... not a repo state"; the Status-Owns-Saturation Rule).
- [ ] AC10: The Dashboard "Needs attention" view renders each item with its true per-repo status per the taxonomy (behind = violet arrow-down, dirty = amber triangle, failed = red x-circle), not a single failed-red icon, building on the Phase 1 fix of audit finding #10. Source: DESIGN.md Section 2; audit `_LOCAL/audit/2026-07-04_18-21_fable-audit.md` finding #10 (dashboard attention blanket-red).
- [ ] AC11: The drawer branch-and-PR-intelligence section refreshes on background check/refresh completion (subscribes to backend events), not only on its own actions, building on the Phase 1 fix of audit finding #11. Source: DESIGN.md Section 5 (Detail drawer); audit finding #11 (drawer staleness).
- [ ] AC12: The row/list badge data rides in the existing single `repo_list` query via additive `RepoSummary` fields (folded into the join), NOT a per-repo fan-out, reusing the BL-NI-22 lesson. Source: BL-NI-22 (O(N) group filter); `src/screens/repos.tsx`.
- [ ] AC13: The PR-fetch logic, its JSON-to-count mapping, and its cache/backoff decisions are pure and testable behind the `Transport` seam with a fake transport and fixtures - no live GitHub calls in tests; `reposync-core` carries no `tauri` import (the E-01 hygiene gate stays green). Source: E-01 dependency hygiene; E-10 spec (mockable `Transport` seam).
- [ ] AC14: Every IPC addition follows the documented E-06 additive amendment path (new `Option` fields, regenerated `bindings.ts` proven by the stale-check gate); no field is renamed or removed. Source: E-06 spec (additive contract revision); E-16 groups amendment precedent.
- [ ] AC15: The authenticated PAT path stays a V1.1 seam (`NoToken` returns `None`); the PR fetch runs unauthenticated and counts against the 60/hour ceiling. Source: E-10 spec AC5; BL-V11-02 (keyring PAT).
- [ ] AC16: Against a mock GitHub server with an injected clock, a 100-repo library reaches full PR-intelligence coverage without ever exceeding 60 requests in any rolling hour and without a single rate-limit error surfacing to the UI; stale values render with their as-of timestamp. Source: E-10 spec AC5 (unauthenticated 60/hour ceiling); E-08 (scheduler refresh cadence); the request-budget design (In scope (b)).

## Dependencies

- **Upstream:**
  - E-10 (GitHub metadata client) - the `Transport` and `TokenProvider` seams, `refresh_one`, ETag caching, rate-limit backoff. Done-core.
  - **BL-NI-15b (E-10 release-ETag 304 short-circuit) - must be resolved before the remote wiring lands** (the release dimension is not trustworthy to wire until the release endpoint has its own ETag and durable staleness state). Precondition, not owned here.
  - E-06 (IPC contract) - the additive amendment path and the `repo_refresh_metadata` command shell E-17 rides on.
  - E-02 (persistence) - owns the `repo_remote_meta` migration 0005 (additive PR columns) and the `last_local_commit_at` write path.
  - E-08 (scheduler) - the refresh cadence that decides WHEN remote intel is refreshed (E-17 honors the ~24h clock; it does not own the tick).
  - E-11 (summary engine) - owns `DailySummary` / `SummaryItem` field authority; the dashboard attention view consumes it (E-17 derives per-item status by joining against the already-fetched `repo_list`, so no `SummaryItem` schema change is needed).
- **Downstream:** none in V1; a per-PR list and a `repo:metadata-refreshed` push event are V1.1 surfaces on the same seams.

## V1.1 extension points

- **Authenticated PR intel via the keyring PAT** (BL-V11-02): a second `TokenProvider` impl lifts the ceiling to 5000/hour and makes private-repo PR intel reliable (the 404/403-as-Unknown ambiguity disappears under an authenticated 404); the fetch/cache/backoff logic is untouched.
- **Per-PR detail list** (author, title, target branch, draft/ready) behind the same seam and an additive `RepoDetail` field or a new `repo_pull_requests(id)` command.
- **A durable release-/PR-detected event source** (BL-NI-16) so a faithful daily/weekly history survives snapshot overwrites and same-day multiples.
- **A `repo:metadata-refreshed` push event** so a background remote refresh updates open windows without a manual refetch.
- **Non-GitHub host intel** (GitLab merge requests, self-hosted) behind the same parse/fetch seam once `host_type` grows.

## Open questions

- **Cheap open-PR count vs precision.** The repo-resource JSON E-10 already fetches carries `open_issues_count`, which GitHub defines as issues PLUS pull requests - a zero-extra-call approximation but conflated. A precise open-PR count needs a dedicated `GET /repos/{owner}/{name}/pulls?state=open` read (the standard trick is `per_page=1` and reading the `Link` header's last-page number as the count). Default: the dedicated pulls read for precision, with its own ETag; flag the extra-call cost against the 60/hour budget at scaffold time. `open_issues_count` is the documented zero-call fallback if the budget bites.
- **PR-fetch cadence.** Whether PR intel refreshes on the same ~24h clock as the repo resource or a slower one (PR counts churn faster than release/description). Default: the same 24h clock in V1 (simplest, and the ETag makes the common case a cheap 304); lift to a separate cadence only if it proves too stale. Flag at wiring.
- **Per-repo request cost.** With the release sub-fetch (E-10) plus the PR fetch, a full refresh is up to three requests per repo. For a large library on the unauthenticated 60/hour ceiling, that tightens the budget; the ratified request budgeter (In scope (b)) caps aggregate GitHub usage at 30 requests per rolling hour and spreads refresh oldest-metadata-first, so the ceiling is respected by construction, with the ETag 304s and the 24h window keeping steady-state volume low and `should_backoff` as the hard backstop. Confirm the real per-pass cost against a realistic library size during dogfood (Phase 2). This is the BL-NI-22 fan-out lesson applied to network calls: batch/skip aggressively, never one avoidable round-trip per repo per surface render.
- **Recency source of truth.** `last_local_commit_at` reads the HEAD commit's committer time (stable, matches "when the checkout's HEAD was authored"); confirm committer vs author time at implementation. Default: committer time.
