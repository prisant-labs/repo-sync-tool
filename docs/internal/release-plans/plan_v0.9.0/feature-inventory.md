# v0.9.0 Feature Inventory (scope at a glance)

- **Date:** 2026-06-23
- **Purpose:** The user-facing feature/function view of the v0.9.0 scope, by readiness. The efforts ([program-roadmap.md](../../program-roadmap.md)) are implementation units; this is the feature view across them.
- **Companion:** [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), [program-roadmap.md](../../program-roadmap.md) (per-effort spec/plan/issue links).

## The honest shape

The 12 original efforts deliberately build the **backend behind the IPC seam** (the roadmap is titled "non-GUI functional efforts"). On 2026-06-23, three **integration efforts** (E-13 tray menu, E-14 notifications, E-15 autostart) were added to close the native-chrome gap. The one piece still unowned by any effort is the **webview GUI** - the screens that render everything.

## Simplified feature list

| Feature | Function(s) | Tier | Effort | Status |
|---------|-------------|------|--------|--------|
| Add repo (by path) | `repo_add_path` | MUST | E-12/E-02 | Done |
| Add repos (scan a folder) | `repo_scan_parent` | MUST | E-02/E-03 | Specced |
| List repos | `repo_list` | MUST | E-02 | Specced |
| Repo detail | `repo_get` | MUST | E-02 | Specced |
| Remove repo | `repo_remove` | MUST | E-02 | Specced |
| Enable/disable per repo | `repo_set_enabled` | MUST | E-02 | Specced |
| Check now | `repo_check_now` | MUST | E-12/E-03 | Done |
| Scheduled background checks | scheduler | MUST | E-08 | Specced |
| Update now (ff-only pull) | `repo_update_now` | MUST | E-07 | Specced |
| Update policy (modes, auto-pause) | `repo_set_policy` | MUST | E-07 | Specced |
| Quick actions (folder/terminal/editor/remote) | `repo_open_*` | MUST | E-03 | Specced |
| Activity log + retention | `activity_list` | MUST | E-09 | Specced |
| Daily summary | `summary_today` | SHOULD | E-11 | Specced |
| GitHub enrichment (unauthenticated) | `repo_refresh_metadata` | SHOULD | E-10 | Specced |
| Settings | `settings_get/set` | MUST | E-02 | Specced |
| Error / degraded states | `AppError` | MUST | E-05 | Done (taxonomy) |
| Tray + native menu | `tray.rs` | MUST | E-13 | Specced (new) |
| Desktop notifications | `notification:fired` | SHOULD | E-14 | Specced (new) |
| Autostart (launch on login) | `settings.autostart` | SHOULD | E-15 | Specced (new) |
| The GUI (all screens) | - | MUST to be usable | none | Gap - no effort, mockups only |

## Readiness categories

**A. Done (built + verified)** - add repo by path, check-now (fetch + ahead/behind + event), error taxonomy, the frozen IPC contract (all commands typed, 2 real), foundation + CI + the Windows MSI packaging spike.

**B. Specced + implementation-ready (spec + plan exist, not built)** - E-02 (persistence/settings/list/detail/remove/enable/scan), E-03 (git engine + quick actions + git-not-found), E-04 (fixtures), E-07 (policy), E-08 (scheduler), E-09 (activity + retention), E-10 (GitHub, unauthenticated), E-11 (daily summary), and now **E-13 (tray menu), E-14 (notifications), E-15 (autostart)**.

**C. Identified but no effort owns it** - the **webview GUI**: dashboard, repo list + detail, activity timeline, settings screen, add/scan flow. Mockups are Draft 1; no effort, no spec. (The integration items that used to sit here - tray, notifications, autostart - are now E-13/14/15.)

**D. Not planned, worth a conscious call for a first release**
- First-run / onboarding / empty state (zero repos).
- **Unsigned-binary friction**: Windows SmartScreen "unknown publisher" + macOS Gatekeeper block (signing deferred / human-only).
- App self-update: the auto-updater is CUT to V1.1, so `0.9.0` updates are manual re-download - decide if a "check for updates" link is wanted.
- Brand/name + app icon finalization (RepoSync is a working title).
- About screen (version/license/links); logs location for support; uninstall data cleanup; basic accessibility.
- Decided, not gaps: no telemetry, no crash reporting (ratified OSS defaults).

## The two tracks to v0.9.0

1. **Backend + integration (UI-independent, all specced):** finish E-02/E-03, then E-04, then E-07/E-08, then E-09/E-10/E-11, plus E-13/E-14/E-15. None needs a UI decision; all are headlessly testable. Recommended next build: **E-04** (unblocks the test-first E-07/E-08).
2. **The GUI (needs design + specs):** the webview screens. Category C. Needs specs before it can be built; renders against the frozen `bindings.ts`.
