# Changelog

All notable changes to RepoSync are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This file is the user-facing NOTES layer: the body of each GitHub Release is
derived from the matching section here. Internal release governance (effort
specs, plans, hygiene gates) lives in `docs/internal/release-plans/`.

## [Unreleased]

### Added
- Foundation: Cargo workspace (Tauri-free `reposync-core` + `src-tauri` shell), React/TypeScript frontend, dual-OS CI matrix with the dependency-hygiene gate (E-01).
- The full 30-variant `AppError` taxonomy with stable codes and remediation (E-05).
- The frozen IPC contract: commands, events, and the payload surface, with `tauri-specta` TypeScript codegen and a stale-bindings CI gate (E-06).
- The update-policy engine: per-repo modes, dirty/branch/failure handling, and 3-strikes auto-pause (E-07).
- The scheduler: interval checks with jitter, quiet hours, bounded concurrency, and per-repo locking, with a global cadence setting that new repos inherit by default (E-08).
- The activity log: every git operation recorded with full context, with retention (E-09).
- The GitHub metadata client: unauthenticated release and repo enrichment with ETag caching and rate-limit backoff (E-10).
- The daily summary: an aggregated view of what happened across all repos today (E-11).
- The full desktop GUI: Dashboard, Repos, Activity, and Settings screens, a repo detail drawer, add/scan flows, and editable settings.
- Groups: user-defined, colored labels for repos, with filtering by group.
- Branch and PR intelligence: each GitHub repo shows its open pull-request count (and how many target the default branch), its latest release, and how stale the local checkout's HEAD is. Counts appear as a signal badge in the repo row, as context on the dashboard's "Needs attention" items, and in a dedicated block in the repo detail drawer. Fetched unauthenticated with a hard request budget (at most 30 GitHub requests per rolling hour) that spreads a large library's first sync over several hours rather than hitting the rate limit; a private or unreachable repo keeps its last-known counts with an "as of" timestamp and is never shown as having zero pull requests (E-17).
- Per-repo check cadence: override the global cadence for a single repo, or inherit it, from the repo detail drawer; the change takes effect immediately without waiting out the old schedule (E-08 / P3-D).
- A database-recovery notice: if a startup migration fails and the database is reset, the app shows a dismissible banner naming where the previous database was preserved (E-02 AC7).
- Open-in actions: open a repo's folder, terminal, editor, or GitHub remote from the app (known defects tracked in `docs/backlog.md`; see Notes).
- A system tray icon with the full native menu - Show RepoSync, Check All Now (checks every enabled repo), Pause all / Resume all (suspends and resumes scheduled checks), Open recent (a submenu of recently-active repos), Settings, and Quit - plus left-click-to-show and close-to-tray (the close button hides to the tray; only Quit exits). On an autostart launch the window starts hidden in the tray (E-13).
- Auto-update: RepoSync can check for a new version on launch (default on, a real toggle in Settings) and via a "Check for updates" button, then install a signed update after you confirm - it never updates silently. Every update is verified against a committed signing key before it is applied; a bad signature aborts and keeps your current version. Delivered over GitHub Releases with a winget package manifest prepared. Auto-update ships DARK in the private build (the update server is not reachable while the repo is private, and the production signing key is a human-only step); it activates at the public flip (E-18).
- Release scaffolding: version-scoped release plans under `docs/internal/release-plans/`, the cut-tag runbook, and this changelog.

### Fixed
- The activity retention sweep now runs on a daily cadence while the app is resident, not only at startup, so a long-running tray session prunes old activity rows as configured (E-09 / P3-D).

### Notes
- Pre-release; this repo is private. See `docs/internal/program-roadmap.md` for the effort breakdown and `docs/internal/release-plans/plan_v0.9.0/plan_v0.9.0.md` for the release plan and readiness checks.
- Desktop notifications and autostart have their core logic built but are not yet wired to the OS (`tauri-plugin-notification` / `tauri-plugin-autostart`); their Settings toggles do not yet have a runtime effect.
- The tray menu is code-complete for V1; its launch-only behavior (menu actions, close-to-tray, autostart-hidden) is verified in the dogfood pass, not by automated tests.
- Open-in has known defects on Windows (path handling, remote URL validation, and more); see `docs/backlog.md`.

<!--
Template for a cut release section (move [Unreleased] items here at G2):

## [0.9.0] - YYYY-MM-DD

### Added
### Changed
### Fixed
### Removed
-->
