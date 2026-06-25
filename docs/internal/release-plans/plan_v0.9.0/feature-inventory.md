# v0.9.0 Feature Inventory (scope at a glance)

- **Date:** 2026-06-23 (last updated 2026-06-25)
- **Purpose:** The user-facing feature/function view of the v0.9.0 scope, by readiness. The efforts ([program-roadmap.md](../../program-roadmap.md)) are implementation units; this is the feature view across them.
- **Companion:** [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), [program-roadmap.md](../../program-roadmap.md) (per-effort spec/plan/issue links). Keep this file's Status column in step with the release plan as efforts land.
- **Status legend:** **Done** = the backend command/function is implemented + tested (no GUI rendering yet); **Specced** = spec + plan exist, not built; **Stub** = a typed stub command exists but is unbuilt; **Gap** = no effort owns it.

## The honest shape

The 12 original efforts deliberately build the **backend behind the IPC seam** (the roadmap is titled "non-GUI functional efforts"). Three **integration efforts** (E-13 tray, E-14 notifications, E-15 autostart) were added 2026-06-23 to close the native-chrome gap. As of 2026-06-25, **9 of 15 efforts are done** and 11 of the 18 V1 commands are real. The one piece still unowned by any effort is the **webview GUI** - the screens that render everything.

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
| Activity log + retention | `activity_list` | MUST | E-09 | Specced |
| Daily summary | `summary_today` | SHOULD | E-11 | Specced |
| GitHub enrichment (unauthenticated) | `repo_refresh_metadata` | SHOULD | E-10 | Specced |
| Settings | `settings_get/set` | MUST | E-02 | **Done** |
| Error / degraded states | `AppError` | MUST | E-05 | **Done** (taxonomy) |
| Tray + native menu | `tray.rs` | MUST | E-13 | Specced |
| Desktop notifications | `notification:fired` | SHOULD | E-14 | Specced |
| Autostart (launch on login) | `settings.autostart` | SHOULD | E-15 | Specced |
| The GUI (all screens) | - | MUST to be usable | none | Gap - mockups only |

> **\*Quick actions are a loose end.** `repo_open_folder/terminal/editor/remote` are tagged E-03 but the E-03 effort delivered the git engine, not these OS shell-out commands - they remain typed stubs. They are small and UI-adjacent (triggered from the repo-detail screen), so they fold naturally into the GUI work or a tiny follow-up; not currently owned by a live effort.

## Readiness categories

**A. Done (built + reviewed)** - **9 efforts:** E-01 (foundation + CI), E-02 (persistence + the list/get/scan/remove/enable/settings commands), E-03 (git engine), E-04 (fixture harness), E-05 (error taxonomy), E-06 (frozen IPC contract), E-07 (policy engine + update-now/set-policy + check-now promotion), E-08 (scheduler: tokio interval, bounded concurrency, per-repo mutex, injected clock, auto-pause persistence), E-12 (tracer + Windows MSI spike). All built test-first, adversarially reviewed, findings fixed.

**B. Specced, not built** - E-09 (activity writer + retention), E-10 (GitHub client, unauthenticated), E-11 (daily summary), E-13 (tray menu), E-14 (notifications), E-15 (autostart). Plus the `repo_open_*` quick-action stubs (no live owner).

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

1. **Backend + integration (UI-independent, all specced):** the foundation (E-01..E-08, E-12) is done. Remaining: **E-09 activity writer** (recommended next), then E-10/E-11, plus E-13/E-14/E-15 and the small `repo_open_*` follow-up. None needs a UI decision; all headlessly testable.
2. **The GUI (needs design):** the webview screens. Category C. The Draft 2 mockups exist; needs a spec/effort before building; renders against the frozen `bindings.ts` and the now-real commands.
