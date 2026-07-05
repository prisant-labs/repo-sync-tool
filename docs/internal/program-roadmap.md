# RepoSync V1 - Program Roadmap (non-GUI functional efforts)

This is the program-level plan for everything in RepoSync V1 that can be built, tested, and frozen **without a single final UI/UX decision**. It decomposes the backend, data, contract, and integration work into numbered efforts, each of which carries its own `spec.md` (the contract) and `implementation-plan.md` (the implementation steps) in its folder.

The per-release operational plan (phases, workstreams, gates) lives in `release-plans/plan_v0.9.0/execution-plan.md` (the v0.9.0 ship execution plan); this file is the cross-release roadmap it draws its scope from.

It operationalizes `docs/internal/v1-architecture-and-decisions.md` (the architecture and decisions brief). Read that brief for the full rationale; read this for the work breakdown.

## Ratified decisions this plan assumes

| Decision | Ratified direction | Date |
|---|---|---|
| Platform | True dual-platform, **Windows-first**, maximally common architecture; macOS degrades to "compiles + bundles in CI" until real Mac access | 2026-05-31 |
| Autonomy boundary | **Visibility-tiered merge** (agent self-merges green PRs while private; human-reviewed once public) layered on a 7-item human-only allowlist (both adopted: the allowlist sits on top of the tiered merges). See `EXECUTION.md` | 2026-06-19 |
| V1 scope line | **MUST / SHOULD / CUT** tiering ratified (below), with pre-committed descope triggers | 2026-06-19 |
| Gitignore (repo hygiene) | `docs/internal/` is **TRACKED**; `_local/` is gitignored. This **OVERRIDES** the brief Section 6 repo-hygiene wording about quarantining `docs/internal/`. | 2026-06-19 |
| Command naming | Normalized to singular `repo_list` (the brief mixed `repos_list` with the `repo_*` family). All command names use the singular `repo_*` form. | 2026-06-19 |
| Schema additions | `repo_local_state` gains `consecutive_failures` and `auto_paused` columns (for the 3-strikes auto-pause); `repo_remote_meta` gains `etag`. All land in the **initial migration**. | 2026-06-19 |
| Groups/tags promotion | Grouping/tags (BL-V11-04) promoted from **CUT to V1.1** into the **v0.9.0** initial release. The `groups` / `repo_groups` schema is frozen into migration 0002. **Updated 2026-07-04:** the store/IPC layer and the GUI are BUILT (commit a85e0fc for the backend, commit 51daaa7 for the frontend); the feature now has a numbered effort, E-16 (groups, repo tags - retroactive spec), with its spec written retroactively at [release-plans/plan_v0.9.0/E-16-groups/spec.md](release-plans/plan_v0.9.0/E-16-groups/spec.md). | 2026-06-30 (amended 2026-07-04) |
| v0.9.0 ship scope and visibility | Ship v0.9.0 COMPLETE (all planned work, including the two new efforts below) but the repo STAYS PRIVATE at tag time. The full release ceremony (merge PR #2, tag v0.9.0, GitHub Release with Windows installer artifacts) happens on the private repo; public launch is a separate later milestone, the "public flip" (BL-DEC-03 (go-public timing)). E-17 (branch and PR intelligence) and E-18 (auto-update and distribution) are promoted into v0.9.0 as new SHOULD-tier efforts; winget SUBMISSION under E-18 waits for the public flip since it needs public artifact URLs. | 2026-07-04 |

## Scope ledger

| Tier | Items | In these efforts |
|---|---|---|
| **MUST** | Add/scan repos, list + detail, manual + scheduled fetch (ff-only), activity log, error states, enable/disable, settings | E-02, E-03, E-04, E-05, E-06, E-07, E-08, E-09, E-12 |
| **SHOULD (keep)** | Unauthenticated GitHub enrichment + ETag caching, daily summary | E-10 (unauthenticated path), E-11 (daily only) |
| **Integration (native chrome / OS)** | Tray native menu, desktop notifications, autostart | E-13 (MUST), E-14 (SHOULD), E-15 (SHOULD) |
| **CUT to V1.1** | Tray popup window (keep native menu), keyring PAT, weekly summary, saved filters, recipes | Stubbed behind seams: PAT path in E-10, weekly aggregation in E-11; the rest are UI-surface and out of these efforts entirely |

> **Amended 2026-06-30, updated 2026-07-04:** grouping/tags moved out of **CUT to V1.1** and into the **v0.9.0** release (feature committed, schema ready). See the ratified-decisions amendment above, [features-and-outcomes.md](release-plans/plan_v0.9.0/features-and-outcomes.md) Section 3, and backlog BL-V11-04. As of 2026-07-04 the store/IPC layer and the GUI are BUILT (commit a85e0fc backend, commit 51daaa7 frontend), and the feature has a numbered effort, [E-16 (groups, repo tags)](release-plans/plan_v0.9.0/E-16-groups/spec.md), whose spec was written retroactively as the as-built contract plus a known-defects section.
>
> **Amended 2026-07-04:** auto-updater (BL-V11-07 (auto-updater)) moved out of **CUT to V1.1** and into the **v0.9.0** release as [E-18 (auto-update and distribution)](release-plans/plan_v0.9.0/E-18-auto-update/spec.md). The updater and manifest infrastructure are built and verified for the private v0.9.0 release; winget SUBMISSION is deferred to the public flip (BL-DEC-03 (go-public timing)), since winget requires public artifact URLs. See E-18's spec and backlog BL-V11-07.

## The seam principle (why this is all buildable now)

The entire backend lives on one side of a single seam: the **typed IPC contract** (E-06). UI/UX decisions govern *rendering*, never *what data exists or how git, the DB, and the scheduler behave*. Freeze the contract early and both halves proceed independently and indefinitely. Every effort below depends on the frozen contract at most, never on a finished screen. `reposync-core` never imports `tauri`; that is what keeps all of this headlessly unit-testable and makes the macOS port a thin edge.

## Effort index

| Effort | Title | Delivers | Depends on | Brief section |
|---|---|---|---|---|
| **E-01** | Foundation, workspace, CI | Cargo workspace, `reposync-core` + `src-tauri` skeletons, repo hygiene, CI matrix + dependency-hygiene gate | - | 4.3, 6 |
| **E-02** | Persistence & paths | SQLite schema as numbered migrations (incl. `scoped_bookmark_blob`), `sqlx::migrate!` runner, WAL pool, `paths` seam, migration-failure recovery | E-01 | 4.5, 4.10b/c |
| **E-03** | Git engine | `cli.rs` (network/mutation) + `inspect.rs` (git2 reads) behind `GitEngine` trait; git discovery + 2.30 floor; git-not-found state | E-01 | 4.6, 4.10d |
| **E-04** | Git fixture harness | Programmatic bare + working repo pairs for all 7 states; pinned git in CI; git2-vs-CLI cross-check | E-03 | 6 |
| **E-05** | Error taxonomy | `AppError` (~30 `thiserror` codes + remediation), `serde` + `specta::Type` | E-01 | 6, 4.10 |
| **E-06** | IPC contract | Command + event surface as Rust types in `reposync-core::ipc`; `tauri-specta` TS codegen; version pin + fallback | E-05 | 4.4 |
| **E-07** | Update-policy engine | Pure `(repo state, policy) -> action or skip-with-reason`; modes, dirty/branch/failure handling, 3-strikes auto-pause | E-04 | 4.6, 5 |
| **E-08** | Scheduler | `tokio` interval, `next_check_at`, jitter, bounded semaphore, quiet hours, **per-repo async mutex**, injected clock | E-04, E-02 | 4.7 |
| **E-09** | Activity log writer + retention | Append every git op with full context; retention sweep honoring `activity_retention_d` (default 90) | E-02 | 4.5, 6 |
| **E-10** | GitHub metadata client | `octocrab`/`reqwest-rustls`, ETag conditional requests, rate-limit backoff; **unauthenticated** V1 path, PAT path stubbed behind a seam for V1.1 | E-02 | 4.4, 6 |
| **E-11** | Summary engine | Daily summary aggregation; weekly left as a V1.1 extension point | E-09 | Section 3 (SHOULD tier) + descope trigger |
| **E-12** | Tracer bullet + packaging spike | `repo_add_path` + `repo_check_now` end to end (real git to SQLite to emitted event) on a real Windows build behind a throwaway UI; early Windows MSI from CI | E-02, E-03, E-06 (thin-slice-sufficient on E-06: needs only the `repo_add_path` / `repo_check_now` / `repo:check-completed` contract slice, not full E-06) | 6 |
| **E-13** | Tray native menu | Native right-click menu (Show / Check All / Pause-Resume / Open recent / Settings / Quit) wired to commands; close-to-tray; native chrome in `tray.rs` | E-01, E-08, E-02 | 8 |
| **E-14** | Desktop notifications | OS toasts for new release / failure / auth via `tauri-plugin-notification`; pure firing-decision + coalescing; quiet-hours aware | E-08, E-10, E-09, E-02 | 11 |
| **E-15** | Autostart | Launch-on-login via `tauri-plugin-autostart` keyed off the `autostart` setting; startup reconciliation; start minimized | E-02, E-01 | platform seam |
| **E-16** | Groups (repo tags), retroactive spec | Many-to-many colored labels for repos, filterable in the GUI; store + IPC + GUI (spec written retroactively for already-built code) | E-02 | n/a (post-brief, 2026-07-04) |
| **E-17** | Branch and PR intelligence | Surface each repo's current branch, ahead/behind counts, and open PR status alongside fetch freshness | E-10, E-06, E-02, E-08, E-11 | n/a (post-brief, 2026-07-04) |
| **E-18** | Auto-update and distribution | In-app updater (`tauri-plugin-updater`) and signed release artifact pipeline; winget manifest built and verified, submission deferred to the public flip | E-12, E-06, E-02, E-01 | n/a (post-brief, 2026-07-04) |

> **Note on E-13 to E-15 (integration efforts, added 2026-06-23):** these close a category-C gap surfaced in the 2026-06-22 audit: the tray menu, notifications, and autostart were named in the brief but owned by no effort. They are **native chrome / OS integration, not webview screens**, so they are UI-independent and buildable now. Tiers (E-13 MUST, E-14/E-15 SHOULD) are provisional, flagged in each spec's open questions.

> **Note on E-16 to E-18 (added 2026-07-04):** E-16 (groups) is a retroactive spec for a feature already built on 2026-07-03 (commit a85e0fc backend, commit 51daaa7 frontend); its tier is MUST because the feature already shipped. E-17 and E-18 are new SHOULD-tier efforts, jp-ratified 2026-07-04, for the v0.9.0 private release (see the ratified-decisions table above). None of the three trace to a brief section since they postdate `v1-architecture-and-decisions.md`.

> **Note on E-11 (summary engine):** the brief has no Section 6 summary workstream. E-11 is a plan-level expansion of the SHOULD-tier "daily summary" scope item (brief Section 3), and is governed by its own pre-committed descope trigger (cut to V1.1 if not green by end of week 5).

> **Dependency-edge semantics:** an edge is either *thin-slice-sufficient* (the downstream effort needs only a named contract or interface slice of its upstream, not the upstream's full completion) or *full-completion-required* (the downstream effort cannot start until the upstream is finished and frozen). E-12's edge to E-06 is thin-slice-sufficient (the `repo_add_path` / `repo_check_now` / `repo:check-completed` slice only), and week-1 uses only *minimal* slices of E-02 (schema + runner) and E-03 (one git2 read + CLI fetch); the remaining edges in the graph below are full-completion-required.

## Per-effort docs and tracking

Each effort's contract (`spec.md`) and execution (`implementation-plan.md`) live in the current release folder; the per-effort build status is aggregated in [plan_v0.9.0.md](release-plans/plan_v0.9.0/plan_v0.9.0.md). The **Issue** column is the live tracking layer (one GitHub issue per effort, milestone = release); it is filled when issues are created.

| Effort | Spec | Impl plan | Build status | Issue |
|--------|------|-----------|--------------|-------|
| E-01 | [spec](release-plans/plan_v0.9.0/E-01-foundation/spec.md) | [plan](release-plans/plan_v0.9.0/E-01-foundation/implementation-plan.md) | Done | #3 |
| E-02 | [spec](release-plans/plan_v0.9.0/E-02-persistence/spec.md) | [plan](release-plans/plan_v0.9.0/E-02-persistence/implementation-plan.md) | Done | #4 |
| E-03 | [spec](release-plans/plan_v0.9.0/E-03-git-engine/spec.md) | [plan](release-plans/plan_v0.9.0/E-03-git-engine/implementation-plan.md) | Done | #5 |
| E-04 | [spec](release-plans/plan_v0.9.0/E-04-git-fixture-harness/spec.md) | [plan](release-plans/plan_v0.9.0/E-04-git-fixture-harness/implementation-plan.md) | Done | #6 |
| E-05 | [spec](release-plans/plan_v0.9.0/E-05-error-taxonomy/spec.md) | [plan](release-plans/plan_v0.9.0/E-05-error-taxonomy/implementation-plan.md) | Done | #7 |
| E-06 | [spec](release-plans/plan_v0.9.0/E-06-ipc-contract/spec.md) | [plan](release-plans/plan_v0.9.0/E-06-ipc-contract/implementation-plan.md) | Done | #8 |
| E-07 | [spec](release-plans/plan_v0.9.0/E-07-update-policy-engine/spec.md) | [plan](release-plans/plan_v0.9.0/E-07-update-policy-engine/implementation-plan.md) | Done | #9 |
| E-08 | [spec](release-plans/plan_v0.9.0/E-08-scheduler/spec.md) | [plan](release-plans/plan_v0.9.0/E-08-scheduler/implementation-plan.md) | Done | #10 |
| E-09 | [spec](release-plans/plan_v0.9.0/E-09-activity-log/spec.md) | [plan](release-plans/plan_v0.9.0/E-09-activity-log/implementation-plan.md) | Done | #11 |
| E-10 | [spec](release-plans/plan_v0.9.0/E-10-github-client/spec.md) | [plan](release-plans/plan_v0.9.0/E-10-github-client/implementation-plan.md) | Done (core; BL-NI-15a/c done, BL-NI-15b before wiring) | #12 |
| E-11 | [spec](release-plans/plan_v0.9.0/E-11-summary-engine/spec.md) | [plan](release-plans/plan_v0.9.0/E-11-summary-engine/implementation-plan.md) | Done | #13 |
| E-12 | [spec](release-plans/plan_v0.9.0/E-12-tracer-bullet/spec.md) | [plan](release-plans/plan_v0.9.0/E-12-tracer-bullet/implementation-plan.md) | Done | #14 |
| E-13 | [spec](release-plans/plan_v0.9.0/E-13-tray-menu/spec.md) | [plan](release-plans/plan_v0.9.0/E-13-tray-menu/implementation-plan.md) | Partial (Show/Quit shipped; completion planned) | #15 |
| E-14 | [spec](release-plans/plan_v0.9.0/E-14-notifications/spec.md) | [plan](release-plans/plan_v0.9.0/E-14-notifications/implementation-plan.md) | Done (core; plugin wiring deferred) | #16 |
| E-15 | [spec](release-plans/plan_v0.9.0/E-15-autostart/spec.md) | [plan](release-plans/plan_v0.9.0/E-15-autostart/implementation-plan.md) | Done (core; plugin wiring deferred) | #17 |
| E-16 | [spec](release-plans/plan_v0.9.0/E-16-groups/spec.md) | [plan](release-plans/plan_v0.9.0/E-16-groups/implementation-plan.md) | Built 2026-07-03 (a85e0fc backend, 51daaa7 frontend); spec retroactive | #18 |
| E-17 | [spec](release-plans/plan_v0.9.0/E-17-branch-intel/spec.md) | [plan](release-plans/plan_v0.9.0/E-17-branch-intel/implementation-plan.md) | Not started | #19 |
| E-18 | [spec](release-plans/plan_v0.9.0/E-18-auto-update/spec.md) | [plan](release-plans/plan_v0.9.0/E-18-auto-update/implementation-plan.md) | Not started | #20 |

## Dependency graph

```mermaid
flowchart TB
    E01[E-01 Foundation + CI]
    E02[E-02 Persistence + paths]
    E03[E-03 Git engine]
    E05[E-05 Error taxonomy]
    E04[E-04 Fixture harness]
    E06[E-06 IPC contract]
    E07[E-07 Policy engine]
    E08[E-08 Scheduler]
    E09[E-09 Activity writer]
    E10[E-10 GitHub client]
    E11[E-11 Summary engine]
    E12[E-12 Tracer + packaging]

    E01 --> E02
    E01 --> E03
    E01 --> E05
    E03 --> E04
    E05 --> E06
    E04 --> E07
    E04 --> E08
    E02 --> E08
    E02 --> E09
    E02 --> E10
    E09 --> E11
    E02 --> E12
    E03 --> E12
    E06 --> E12
```

## Sequencing (UI-independent, ~3 weeks then breadth)

The tracer bullet (E-12) is deliberately pulled forward into week 1, built on *thin-slice-sufficient* slices of E-01/E-02/E-03/E-06 (see the edge-semantics note above), so the whole architecture is pierced once before any breadth is built. E-12 needs only the `repo_add_path` / `repo_check_now` / `repo:check-completed` contract slice of E-06, not full E-06, and only *minimal* slices of E-02 and E-03; it does not wait on those efforts completing. The riskiest cross-platform unknowns surface while the codebase is tiny.

- **Week 1 - prove the spine, lay the floor.** E-01 (workspace + hygiene + CI), minimal E-02 (schema + runner) and minimal E-03 (one git2 read + CLI fetch), then the E-12 tracer end to end on a real Windows build with the macOS bundle green in CI.
- **Week 2 - make the engine trustworthy and freeze the seam.** E-04 (fixture harness, the biggest testability multiplier), E-03 parsers + git2/CLI cross-check hardened, E-05 (`AppError`), E-06 (full IPC contract + `tauri-specta` codegen). After week 2 the seam is frozen and the frontend can stub against real types.
- **Week 3 - layer logic, background machinery, packaging.** E-07 (policy), E-08 (scheduler), E-09 (activity writer), E-10 (GitHub client), E-11 (daily summary), and a real packaging spike producing a signed-or-documented Windows artifact.

## Descope triggers (pre-committed)

- If the tray popup window (a V1.1 item already) is ever pulled into V1 and is not stable on Windows by end of week 4, it stays cut and V1 ships with the native menu only.
- If macOS signing/notarization is not unblocked (Mac access + credentials) by end of week 4, drop macOS from the V1 GA bar and ship Windows-only GA, macOS as a staged later GA. Watch this first; it is the most likely buffer-eater.
- If GitHub enrichment (E-10) is not green by end of week 5, ship V1 with local git state only and add enrichment in a fast-follow.
- If the daily summary (E-11) is not done by end of week 5, cut it to V1.1 (it is SHOULD, not MUST).

## Conventions and structure

- Each effort lives in a per-effort folder `E-NN-slug/` with `spec.md` and `implementation-plan.md`, under `docs/internal/release-plans/` - either in `_unassigned/` (not yet committed to a release) or inside the release it is promoted into (`plan_vX.Y.Z/E-NN-slug/`). All 18 v0.9.0 efforts (E-01 through E-18) currently sit in `plan_v0.9.0/`. See `docs/internal/release-plans/README.md` for the structure and the promote/gate flow.
- `spec.md` is the contract: frontmatter, a Task Summary block agents keep current, scope, the interface/contract, acceptance criteria with source citations into the brief, dependencies, and V1.1 extension points.
- `implementation-plan.md` is the how: ordered steps, test strategy, files touched, risks, and a definition of done.
- Hard rules: no em-dashes or en-dashes anywhere; `reposync-core` never imports `tauri`; tests are written with the logic (test-first for the pure engines E-07/E-08).
- This file is the cross-release roadmap (dependency graph, sequencing, scope ledger). A single release's aggregation + gates + checklist live in that release's `plan_vX.Y.Z.md`.

## Source

`docs/internal/v1-architecture-and-decisions.md` (architecture + decisions brief) and `docs/internal/strategy-and-roadmap.md` (the original plan it extends). Governance: `EXECUTION.md` at the repo root.
