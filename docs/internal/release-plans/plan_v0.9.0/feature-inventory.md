# v0.9.0 Feature Inventory (scope at a glance)

- **Date:** 2026-06-23 (last updated 2026-07-04)
- **Purpose:** The user-facing feature/function view of the v0.9.0 scope, by readiness. The efforts ([program-roadmap.md](../../program-roadmap.md)) are implementation units; this is the feature view across them.
- **Companion:** [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), [execution-plan.md](execution-plan.md) (the phased path to the tag), [program-roadmap.md](../../program-roadmap.md) (per-effort spec/plan/issue links), [features-and-outcomes.md](features-and-outcomes.md) (the product-facing view: features + the user problem each solves). Keep this file's Status column in step with the release plan as efforts land.
- **Status legend:** **Done** = the backend command/function is implemented + tested (no GUI rendering yet); **Built** = implemented end to end, backend and GUI, and may still carry open defects tracked in the backlog; **Specced** = spec + plan exist, not built; **Stub** = a typed stub command exists but is unbuilt; **Gap** = no effort owns it.

## The honest shape

The 12 original efforts deliberately build the **backend behind the IPC seam** (the roadmap is titled "non-GUI functional efforts"). Three **integration efforts** (E-13 tray, E-14 notifications, E-15 autostart) were added 2026-06-23 to close the native-chrome gap, and three more efforts were added 2026-07-04: **E-16** (Groups, retroactive spec for a build that already shipped), **E-17** (branch and PR intelligence), and **E-18** (auto-update and distribution). As of 2026-07-04, of the 18 efforts in scope: 15 are done (E-01 through E-12, E-14, E-15, and E-16), one is partial (E-13 tray: Show/Quit shipped, four menu items plus close-to-tray remain), and two have not started (E-17, E-18). The **webview GUI** itself, once the one piece owned by no effort, is now built: the full shell plus the Dashboard, Repos, Activity, and Settings screens, plus the repo-detail drawer, landed 2026-07-03. It carries open defects from the 2026-07-04 audit (`_LOCAL/audit/2026-07-04_18-21_fable-audit.md`), fixed in Phase 1 of [execution-plan.md](execution-plan.md), not open scope.

## Simplified feature list

| Feature | Function(s) | Tier | Effort | Status |
|---------|-------------|------|--------|--------|
| Add repo (by path) | `repo_add_path` | MUST | E-12/E-02 | **Done** |
| Add repos (scan a folder) | `repo_scan_parent` | MUST | E-02 | **Done** |
| List repos | `repo_list` | MUST | E-02 | **Done** |
| Repo detail | `repo_get` | MUST | E-02 | **Done** |
| Remove repo | `repo_remove` | MUST | E-02 | **Done** |
| Enable/disable per repo | `repo_set_enabled` | MUST | E-02 | **Done** |
| Groups (repo tags) | `groups` / `repo_groups` (store + IPC + GUI) | MUST (promoted 2026-06-30) | [E-16](E-16-groups/spec.md) | **Built** (a85e0fc backend, 51daaa7 frontend, both 2026-07-03; spec written retroactively at E-16; all six Known defects fixed 2026-07-05 - the group-filter false-empty state and five others in commit 4ab54bf, and BL-NI-22 (the O(N) per-repo group query) resolved via a single bulk `repo_group_memberships` IPC call, verified in the working tree, not yet committed) |
| Check now | `repo_check_now` | MUST | E-12/E-03/E-07 | **Done** |
| Scheduled background checks | scheduler | MUST | E-08 | **Done** |
| Update now (ff-only pull) | `repo_update_now` | MUST | E-07 | **Done** |
| Update policy (modes, auto-pause) | `repo_set_policy` | MUST | E-07 | **Done** |
| Quick actions (folder/terminal/editor/remote) | `repo_open_*` | MUST | E-03* | **Built, hardened** (implemented 2026-07-03, commit 8fc806c; the Windows canonicalized-path defect, the unvalidated-remote-URL security defect, the `cmd /C` metacharacter injection, the silent-editor-failure defect, and the `wt.exe` full-path detection gap - audit findings 1-2, 8-9 plus one low - were all fixed 2026-07-05 in commit 187ed2a) |
| Activity log + retention | `activity_list` | MUST | E-09 | **Done + wired** (writer + retention; the `activity_list` read + IPC command wired in the edge-wiring effort) |
| Daily summary | `summary_today` | SHOULD | E-11 | **Done + wired** (daily roll-up over activity + state; command wired with the edge local-day window; release-event fidelity = BL-NI-16; weekly = V1.1 seam) |
| GitHub enrichment (unauthenticated) | `repo_refresh_metadata` | SHOULD | E-10 | **Done + wired** (command wired over the unauthenticated client; `X-RateLimit-Reset` now captured for an honest rate-limit error; release data-loss + backoff = BL-NI-15a/c; release-ETag/cadence = BL-NI-15b) |
| Settings | `settings_get/set` | MUST | E-02 | **Done** |
| Error / degraded states | `AppError` | MUST | E-05 | **Done** (taxonomy) |
| Tray + native menu | `tray.rs`, `windows/mod.rs` | MUST | E-13 | **Code-complete** (P3-C, 2026-07-05; dogfood-pending): full six-item menu (Show, Check All Now, Pause all/Resume all, Open recent submenu, Settings, Quit) + left-click-show + close-to-tray + launch-aware visibility, per [E-13-tray-menu/spec.md](E-13-tray-menu/spec.md). Backed by an additive `repo_check_all` command, a `navigate:requested` event, and a `GlobalPause` the scheduler honors; the dead `repo:state-changed`/`check-started`/`error:raised` events now emit (BL-NI-31). Launch verification is dogfood-only |
| Desktop notifications | `notification:fired` | SHOULD | E-14 | **Done** (core firing decision + coalescing, quiet-hours aware; `tauri-plugin-notification` emit-site deferred edge, Phase 3) |
| Autostart (launch on login) | `settings.autostart` | SHOULD | E-15 | **Done** (core: reconcile drift policy with a non-actuating Unknown OS state + launch-arg detection; `tauri-plugin-autostart` actuation deferred edge, Phase 3) |
| The GUI (all screens) | - | MUST to be usable | none | **Built** (full shell plus Dashboard, Repos, Activity, and Settings, plus the repo-detail drawer, landed 2026-07-03; design language settled in DESIGN.md; open defects in the audit fixed in Phase 1) |
| Branch and PR intelligence | `repo_refresh_metadata` (enriched); additive `RepoSummary`/`RepoDetail` fields | SHOULD (jp-ratified 2026-07-04) | [E-17](E-17-branch-intel/spec.md) | **Shipped** (Phase 4, 2026-07-05: open-PR + default-branch-PR counts behind the E-10 `Transport` seam with their own ETag; `last_local_commit_at` populated; the rate budgeter caps aggregate GitHub usage at 30/rolling-hour with oldest-first backfill; BL-NI-15b resolved via the decoupled `release_etag`; row signal badge + dashboard attention context + drawer intel block; migration 0005) |
| Auto-update and distribution | (not yet designed) | SHOULD (jp-ratified 2026-07-04) | [E-18](E-18-auto-update/spec.md) | **Not started** (updater + winget manifest prepared and verified; winget submission deferred to the public flip) |

> **\*Quick actions were a loose end, now built.** `repo_open_folder/terminal/editor/remote` are tagged E-03 but the E-03 effort delivered the git engine, not these OS shell-out commands; they were implemented 2026-07-03 (commit 8fc806c) as part of the edge-wiring build, still with no live effort owner, shipped with the defects noted above, and hardened 2026-07-05 in commit 187ed2a.

> **Groups (repo tags)** were promoted from V1.1 into v0.9.0 on 2026-06-30 and are now built (2026-07-03): the store, IPC layer, and GUI are all shipped on top of the `groups` + `repo_groups` schema frozen into migration 0002 (a many-to-many association with name + color). The feature spec was written retroactively as E-16, after the build, rather than before it. Ledger amendment: [program-roadmap.md](../../program-roadmap.md) (2026-06-30); backlog BL-V11-04; product framing in [features-and-outcomes.md](features-and-outcomes.md) Section 3; as-built contract in [E-16-groups/spec.md](E-16-groups/spec.md).

## Readiness categories

**A. Done (built + reviewed)** - **15 efforts:** E-01 (foundation + CI), E-02 (persistence + the list/get/scan/remove/enable/settings commands), E-03 (git engine), E-04 (fixture harness), E-05 (error taxonomy), E-06 (frozen IPC contract), E-07 (policy engine + update-now/set-policy + check-now promotion), E-08 (scheduler: tokio interval, bounded concurrency, per-repo mutex, injected clock, auto-pause persistence, spawned resident 2026-07-03 in commit 81c96af), E-09 (activity writer + retention sweep), E-10 (GitHub metadata client core - fetch/map/cache + parser hardened; the release data-loss fix + rate-limit backoff surface landed test-first as BL-NI-15a/c after 3 adversarial-review cycles - atomic write, tri-state `ReleaseState`, same-auth release request; the release-ETag/separate-cadence rework is BL-NI-15b, deferred to wiring), E-11 (daily summary engine: read-only roll-up over activity + state; attention and no-change disjoint; new-release detection via the latest-release snapshot, with release-event fidelity backlogged as BL-NI-16; weekly is a V1.1 seam), E-12 (tracer + Windows MSI spike), E-14 (desktop-notifications core: the pure firing decision + cycle coalescing with per-kind identity preserved, quiet-hours aware via the scheduler's predicate; the `tauri-plugin-notification` emit-site is deferred edge; auth-toggle policy = BL-NI-17), E-15 (autostart core: the reconcile drift-correction policy with a tri-state OS read that refuses to actuate on an unknown/failed query, plus launch-arg detection; the `tauri-plugin-autostart` actuation is deferred edge; the "setting wins vs adopt the OS change" policy = BL-NI-18), E-16 (Groups: store, IPC, and GUI built 2026-07-03, spec written retroactively). All built test-first, adversarially reviewed, findings fixed or filed (E-16's build predates its spec; see the E-16 note above).

**B. Partial or specced, not built** - E-13 (tray menu) is now CODE-COMPLETE (P3-C, 2026-07-05: full six-item menu + close-to-tray + launch-aware visibility; dogfood verification pending, per [E-13-tray-menu/spec.md](E-13-tray-menu/spec.md)). E-17 (branch and PR intelligence) SHIPPED 2026-07-05 (Phase 4: PR intelligence behind the E-10 seam with the rate budgeter, BL-NI-15b resolved, the signal-register row badge + drawer intel block; dogfood pending). E-18 (auto-update and distribution) is specced (2026-07-04) but not started; Phase 4 of the execution plan builds it.

**C. Built this pass, formerly the open gap** - the **webview GUI** (dashboard, repo list + detail, activity timeline, settings screen, add/scan flow) and the `repo_open_*` **quick actions** were both the "no effort owns it" gap as of 2026-06-30; both landed 2026-07-03 (commits 8aeebd7 and 8fc806c). Neither has a numbered effort of its own, but neither is a gap anymore, only a source of open defects (see the audit) tracked through Phase 1 of the execution plan.

**D. Not planned, worth a conscious call for a first release**
- First-run / onboarding / empty state (zero repos).
- **Unsigned-binary friction**: Windows SmartScreen "unknown publisher" + macOS Gatekeeper block (signing deferred / human-only).
- Brand/name + app icon finalization (RepoSync is a working title).
- About screen (version/license/links); logs location for support; uninstall data cleanup; basic accessibility.
- OneDrive relocation when `%LOCALAPPDATA%` is itself synced (BL-NI-12).
- Decided, not gaps: no telemetry, no crash reporting (ratified OSS defaults).

> App self-update was previously listed here as an open call ("the auto-updater is CUT to V1.1"); that is now superseded. E-18 (auto-update and distribution) brings the updater into v0.9.0 scope, ratified 2026-07-04. See [E-18-auto-update/spec.md](E-18-auto-update/spec.md).

## The three tracks from here to the tag

As of 2026-07-04, the first two tracks (backend/integration, GUI) are both built; what remains on them is correctness and completion, not new construction. The third track (new features) has not started. All three are sequenced in [execution-plan.md](execution-plan.md).

1. **Backend + integration:** the foundation (E-01..E-12) plus the E-14 notifications and E-15 autostart cores is done (E-10 / E-14 / E-15 core-only, their plugin/edge wiring deferred; E-11 done with the BL-NI-16 caveat). The scheduler is spawned resident at launch (81c96af) and the manual commands share its per-repo locks. E-13's tray is PARTIAL (Show/Quit only, bb353f9); the `repo_open_*` quick actions are built and hardened (8fc806c, then 187ed2a on 2026-07-05). Remaining edge-wiring work: finish the tray's four remaining menu items + close-to-tray, wire the E-14 `tauri-plugin-notification` emit-site, wire the E-15 autostart registration/OS-query, and resolve BL-NI-15b / BL-NI-16 / BL-NI-18. This is Phase 3 of the execution plan.
2. **The GUI:** the webview screens. Formerly Category C, now built. The full shell plus Dashboard/Repos/Activity/Settings plus the detail drawer plus add/scan landed 2026-07-03 on `build/e-01-foundation` against the frozen `bindings.ts`; the design language is the Graphite direction in `DESIGN.md` (draft mockups archived to `_local/gui/archived-mockups/`). Remaining work is fixing the audit's open defects (Phase 1) and dogfooding (Phase 2), not building new screens.
3. **New features:** E-17 (branch and PR intelligence) SHIPPED 2026-07-05; E-18 (auto-update and distribution), ratified 2026-07-04, not started. This is Phase 4 of the execution plan.
