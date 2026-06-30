# v0.9.0 Feature Inventory (scope at a glance)

- **Date:** 2026-06-23 (last updated 2026-06-29)
- **Purpose:** The user-facing feature/function view of the v0.9.0 scope, by readiness. The efforts ([program-roadmap.md](../../program-roadmap.md)) are implementation units; this is the feature view across them.
- **Companion:** [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), [program-roadmap.md](../../program-roadmap.md) (per-effort spec/plan/issue links). Keep this file's Status column in step with the release plan as efforts land.
- **Status legend:** **Done** = the backend command/function is implemented + tested (no GUI rendering yet); **Specced** = spec + plan exist, not built; **Stub** = a typed stub command exists but is unbuilt; **Gap** = no effort owns it.

## The honest shape

The 12 original efforts deliberately build the **backend behind the IPC seam** (the roadmap is titled "non-GUI functional efforts"). Three **integration efforts** (E-13 tray, E-14 notifications, E-15 autostart) were added 2026-06-23 to close the native-chrome gap. As of 2026-06-29, **14 of 15 efforts are done** (the E-14 notifications and E-15 autostart cores added; their plugin/edge actuation is deferred). The only non-done effort is E-13 (tray), deferred into the edge-wiring effort. Every buildable behind-the-seam core is now done; the one piece still unowned by any effort is the **webview GUI** - the screens that render everything.

## Simplified feature list

| Feature | Function(s) | Tier | Effort | Status |
|---------|-------------|------|--------|--------|
| Add repo (by path) | `repo_add_path` | MUST | E-12/E-02 | **Done** |
| Add repos (scan a folder) | `repo_scan_parent` | MUST | E-02 | **Done** |
| List repos | `repo_list` | MUST | E-02 | **Done** |
| Repo detail | `repo_get` | MUST | E-02 | **Done** |
| Remove repo | `repo_remove` | MUST | E-02 | **Done** |
| Enable/disable per repo | `repo_set_enabled` | MUST | E-02 | **Done** |
| Check now | `repo_check_now` | MUST | E-12/E-03/E-07 | **Done** |
| Scheduled background checks | scheduler | MUST | E-08 | **Done** |
| Update now (ff-only pull) | `repo_update_now` | MUST | E-07 | **Done** |
| Update policy (modes, auto-pause) | `repo_set_policy` | MUST | E-07 | **Done** |
| Quick actions (folder/terminal/editor/remote) | `repo_open_*` | MUST | E-03* | **Stub (unbuilt)** |
| Activity log + retention | `activity_list` | MUST | E-09 | **Done** (writer + retention; `activity_list` read is E-06/UI) |
| Daily summary | `summary_today` | SHOULD | E-11 | **Done** (daily roll-up over activity + state; release-event fidelity = BL-NI-16; weekly = V1.1 seam) |
| GitHub enrichment (unauthenticated) | `repo_refresh_metadata` | SHOULD | E-10 | **Done** (core; release/cache/rate-limit hardening = BL-NI-15 before wiring) |
| Settings | `settings_get/set` | MUST | E-02 | **Done** |
| Error / degraded states | `AppError` | MUST | E-05 | **Done** (taxonomy) |
| Tray + native menu | `tray.rs` | MUST | E-13 | Deferred (folds into the edge-wiring effort - pure Tauri chrome, no unit-testable core) |
| Desktop notifications | `notification:fired` | SHOULD | E-14 | **Done** (core firing decision + coalescing, quiet-hours aware; `tauri-plugin-notification` emit-site deferred edge) |
| Autostart (launch on login) | `settings.autostart` | SHOULD | E-15 | **Done** (core: reconcile drift policy with a non-actuating Unknown OS state + launch-arg detection; `tauri-plugin-autostart` actuation deferred edge) |
| The GUI (all screens) | - | MUST to be usable | none | Gap - mockups only |

> **\*Quick actions are a loose end.** `repo_open_folder/terminal/editor/remote` are tagged E-03 but the E-03 effort delivered the git engine, not these OS shell-out commands - they remain typed stubs. They are small and UI-adjacent (triggered from the repo-detail screen), so they fold naturally into the GUI work or a tiny follow-up; not currently owned by a live effort.

## Readiness categories

**A. Done (built + reviewed)** - **14 efforts:** E-01 (foundation + CI), E-02 (persistence + the list/get/scan/remove/enable/settings commands), E-03 (git engine), E-04 (fixture harness), E-05 (error taxonomy), E-06 (frozen IPC contract), E-07 (policy engine + update-now/set-policy + check-now promotion), E-08 (scheduler: tokio interval, bounded concurrency, per-repo mutex, injected clock, auto-pause persistence), E-09 (activity writer + retention sweep), E-10 (GitHub metadata client core - fetch/map/cache + parser hardened; the release/cache/rate-limit rework is backlogged as BL-NI-15 to land before wiring), E-11 (daily summary engine: read-only roll-up over activity + state; attention and no-change disjoint; new-release detection via the latest-release snapshot, with release-event fidelity backlogged as BL-NI-16; weekly is a V1.1 seam), E-12 (tracer + Windows MSI spike), E-14 (desktop-notifications core: the pure firing decision + cycle coalescing with per-kind identity preserved, quiet-hours aware via the scheduler's predicate; the `tauri-plugin-notification` emit-site is deferred edge; auth-toggle policy = BL-NI-17), E-15 (autostart core: the reconcile drift-correction policy with a tri-state OS read that refuses to actuate on an unknown/failed query, plus launch-arg detection; the `tauri-plugin-autostart` actuation is deferred edge; the "setting wins vs adopt the OS change" policy = BL-NI-18). All built test-first, adversarially reviewed, findings fixed or filed.

**B. Specced, not built** - none remain as pure spec; every buildable behind-the-seam core is done. E-13 (tray menu) is DEFERRED into the edge-wiring effort (pure Tauri chrome that adds no product logic, blocked on the scheduler control surface + a window, so there is nothing to unit-test now). Plus the `repo_open_*` quick-action stubs (no live owner).

**C. Identified but no effort owns it** - the **webview GUI**: dashboard, repo list + detail, activity timeline, settings screen, add/scan flow. Mockups exist (Draft 2: dashboard, onboarding, settings, repo-detail, Windows parity); no effort, no spec.

**D. Not planned, worth a conscious call for a first release**
- First-run / onboarding / empty state (zero repos).
- **Unsigned-binary friction**: Windows SmartScreen "unknown publisher" + macOS Gatekeeper block (signing deferred / human-only).
- App self-update: the auto-updater is CUT to V1.1, so `0.9.0` updates are manual re-download - decide if a "check for updates" link is wanted.
- Brand/name + app icon finalization (RepoSync is a working title).
- About screen (version/license/links); logs location for support; uninstall data cleanup; basic accessibility.
- OneDrive relocation when `%LOCALAPPDATA%` is itself synced (BL-NI-12).
- Decided, not gaps: no telemetry, no crash reporting (ratified OSS defaults).

## The two tracks to v0.9.0

1. **Backend + integration (UI-independent, all specced):** the foundation (E-01..E-12) plus the E-14 notifications and E-15 autostart cores is done (E-10 / E-14 / E-15 core-only, their plugin/edge wiring deferred; E-11 done with the BL-NI-16 caveat). All behind-the-seam cores are complete. Remaining: the **edge-wiring effort** - spawn the scheduler at launch, wire the manual commands to shared locks, build the tray (E-13) + the E-14 `tauri-plugin-notification` emit-site + the E-15 autostart registration/OS-query, and resolve BL-NI-15 / BL-NI-16 / BL-NI-18. Plus the small `repo_open_*` follow-up. The core logic is all headlessly tested; the edge chrome needs a real Windows launch.
2. **The GUI (needs design):** the webview screens. Category C. The Draft 2 mockups exist; needs a spec/effort before building; renders against the frozen `bindings.ts` and the now-real commands.
