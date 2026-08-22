# Changelog

All notable changes to RepoSync are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project aims
to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

This file is the user-facing NOTES layer: the body of each GitHub Release is
derived from the matching section here. Internal release governance (effort
specs, plans, hygiene gates) lives in `docs/internal/release-plans/`.

## [Unreleased]

### Changed
- **The repository is now public** (2026-07-17), at
  `github.com/prisant-labs/repo-sync-tool` under the MIT license. The v0.9.0
  notes below describe a private build and are kept as the historical record;
  see the note at the end of that section for what has since changed.
- Group colors can be edited after creation, not only at create time. A
  rejected duplicate name now leaves both the name and the color unchanged.
- The close (X) button is configurable: **Settings -> System -> "Close button
  minimizes to tray"**, on by default. Off makes the close button quit the app.
  Existing installs keep the previous behavior on upgrade. Tray **Quit** always
  exits either way.

### Added
- **Activity can be filtered by action and outcome.** Two rows of chips above the
  list narrow it to checks or updates, and to successes or failures, so finding
  the one failed update for one repository no longer means scrolling. The filter
  runs in the database, not over the rows already on screen, so "show me failures"
  searches the whole log rather than the most recent page of it. The list also now
  says when it is showing only the newest matching entries, instead of leaving a
  full page looking like the complete history.
- **Activity entries open a full receipt.** Selecting a row in Activity shows
  what RepoSync actually ran, what git printed back on both streams, the exit
  code, and how long it took. RepoSync has always recorded this; nothing
  displayed it, so answering "why did that repository not update" meant opening
  the database by hand. Captured output has already had credentials stripped
  from it, so the receipt shows exactly what is stored.
- **A Diagnostics panel in Settings, with an "Open logs" button.** It shows
  where your data, database, and log folders are; whether logging is actually
  running and under what retention; which `git` executable was found and its
  version; and how many scheduled cycles have run since launch. It flags the
  three conditions worth acting on - logging that failed to start, a data folder
  inside a OneDrive-synced tree, and a database that had to be recovered at
  startup - and a "Copy details" button produces a plain-text summary for a bug
  report. The log folder was previously reachable only by knowing its path.
- **Scheduled check outcomes that fail to save are now counted where you can see
  them.** When a background check finishes but its result cannot be written, the
  repository simply stays due and retries, which is the right behavior and
  completely silent. The Diagnostics panel now carries the count, so a database
  problem that would otherwise present as "checks seem to run twice" is visible.
- A published [security model](docs/security-model.md) describing trust
  boundaries, the controls in place, and the known weaknesses that remain.
- A real [README](README.md), including an honest platform and signing status
  table.
- **RepoSync now keeps a diagnostic log.** Background failures that a tray app
  cannot show you - a scheduled check that could not save its result, an
  activity entry that failed to write, git going missing, an update check that
  could not reach the server - are written to a rotating file under the app data
  directory (`logs/reposync.<date>.log`) instead of vanishing. Previously they
  went to a console that a release build does not even have, so a problem that
  happened while you were not looking left no trace at all. Fourteen days are
  kept by default, capped at 32 MiB total, oldest first. Three environment
  variables adjust it: `REPOSYNC_LOG=debug` for more detail while reproducing a
  problem, and `REPOSYNC_LOG_DAYS` / `REPOSYNC_LOG_MAX_MB` if you need a longer
  history or a smaller footprint.

### Fixed
- **A problem at GitHub no longer reports itself as a problem with your internet.**
  When GitHub answered a metadata refresh with an error, RepoSync classified
  every case it did not have a specific name for as a lost connection and told
  you "no network connection" - about a request that had plainly gone through,
  since a round trip had to complete for that error to come back at all. You
  would go and check your wifi while the actual problem was a GitHub outage or a
  refused request. It now reports which error GitHub returned, and how it is
  treated follows from that: a server error (5xx) is temporary and gets retried,
  while a refusal (4xx) is not and stops, where previously every case was retried
  forever under the wrong explanation. Requests that genuinely never reach GitHub
  still report as a lost connection, which is what that message was always for.
- **A repository whose upstream branch was deleted no longer reads "In sync".**
  RepoSync has always known the difference between a repository tracking a live
  upstream, one with no upstream at all, and one whose upstream was deleted from
  the remote, and it correctly skips checking the last of those instead of
  reporting a failure. Nothing carried that verdict out to the views, so they
  fell through to the most reassuring answer available. One repository recorded
  this outcome 39 times over ten days, green every time, while showing "In sync"
  for a branch that structurally could not sync. It now has its own status,
  **No upstream**. Two limits worth knowing: it ranks below "dirty" and "failed"
  and stays out of the "Needs attention" count, because it describes a fact
  about the repository rather than something to act on today; and a repository
  last checked before this release keeps its previous badge until its next
  check, then changes over.
- **Quiet hours no longer discards the notifications it withholds, and the
  setting now describes both halves of what it does.** The hint mentioned only
  the paused checks, so turning it on to stop overnight git activity also muted
  failure alerts without saying so. Worse, a cycle that began before the window
  opened and finished inside it had every notification it produced destroyed
  rather than delayed: a fetch that failed at 21:59 with quiet hours starting at
  22:00 was never reported at all, and the next cycle had nothing left to report
  with. Withheld notifications are now held and delivered on the first cycle
  after the window ends, folded in with that cycle's own. Held alerts live in
  memory and do not survive a restart, which can cost you the alert but never
  the record: the failure stays on the repository, in the Activity log, and in
  the "Needs attention" count.
- **The Dashboard's "Needs attention" hint promised a rule the count does not
  implement.** It read "dirty, failed, behind"; the number has only ever counted
  repositories that are dirty or have a recorded failure. It now reads "dirty or
  failed". Behind-ness was deliberately left out rather than folded in: the
  default update mode fetches without touching your working tree, so a watched
  library drifting behind is the intended steady state rather than an anomaly,
  and counting it would make the number permanently non-zero and stop it meaning
  "act on this". Behind-ness keeps its own badge on the Repos screen.
- **The Settings row for a GitHub token described a keychain RepoSync has never
  written to.** It read "Stored in the OS keychain, never on disk. Managed
  outside this screen." None of that is true in this version: there is no token
  provider, nothing writes to a keychain, and there is no other screen. The row
  now says what actually applies, that RepoSync reads GitHub without signing in
  and GitHub allows 60 requests an hour that way, shared across all your
  repositories. Its value reads "not supported yet" rather than "not set",
  because "not set" invited you to go and set it and there was nowhere to do
  that. The row stayed rather than being hidden because that 60-per-hour ceiling
  is a real constraint on a large library.
- **A repository whose checks are failing now actually appears in "Needs
  attention", and shows as failed in the Repos list.** It previously appeared
  only if it also happened to have uncommitted changes. The error code every one
  of those views reads was declared in the database and read in three places, and
  nothing ever wrote it, so it was empty forever: a repository that had been
  unable to reach its remote for a week looked healthy everywhere except the
  activity log. The failure reason is now recorded whenever a check or update
  fails, and cleared again when the repository recovers.
- **A failed check now reports itself instead of disappearing.** Checking a
  repository whose fetch fails used to raise an error that stopped the completion
  from being announced at all, so other open views never learned the check had
  finished and the tray's "Check All Now" did not count it. A failed check is now
  a completed check that reports what went wrong, carrying the reason with it, and
  the repository detail view says which of the three causes it was (credentials,
  network, or something else) and points at the Activity receipt for the exact
  git output. A run of "Check All Now" in which some repositories failed reports
  once for the whole run rather than once per repository, and names the most
  actionable problem it saw: a credential failure is reported ahead of a network
  failure, because the first will not fix itself.
- **A manual update that recovers a repository now lifts everything together.**
  Clearing the error, the failure streak, and the automatic pause used to be two
  separate writes, so a failure between them could leave a repository paused out
  of scheduled checking while looking healthy in every view that would have told
  you about it.
- **Adding a repository is now atomic.** Previously the registry row and its
  local-state row were written separately, so a failure between them left a
  repository that would list forever, never record a check, never report an
  error, and refuse to be re-added because the incomplete row still held the
  unique-path constraint.
- **A check now records its outcome and its receipt together.** Previously a
  failure between the two could advance a repository's "last checked" time with
  no matching entry in the activity log, which made the activity log quietly
  incomplete rather than visibly wrong.
- **A momentarily busy database is no longer reported as a permanent failure.**
  SQLite lock contention is now classified as retryable, so it surfaces as "the
  database is busy, retry" instead of a hard error with no useful next step.

### Security
- **Captured git output is now bounded.** Each stored command, stdout, and
  stderr stream is capped at 16 KiB with an explicit truncation marker. Git's
  output is controlled by the remote, and a check is recorded per repository per
  cycle indefinitely, so this was an unbounded write amplifier pointed at your
  disk.
- **Credentials are stripped from captured git output before it is stored,
  shown, or logged.** Any credentials embedded in a URL are removed outright,
  and well-known GitHub and GitLab token formats plus `Authorization:` headers
  are removed on a best-effort basis. This runs at the moment output is
  captured, so the database, the error messages on screen, and the diagnostic
  log all get the filtered version. Note the honest limit: arbitrary secrets in
  unfamiliar formats are not guaranteed to be caught, and paths and repository
  names are deliberately left intact because they are the diagnostic value.
- **CI now runs a dependency advisory gate** on every pull request, covering
  both the Rust and the production npm dependency graphs. Any accepted advisory
  is recorded explicitly in `.cargo/audit.toml` with a reason, so a known
  vulnerability cannot pass silently.

## [0.9.0] - 2026-07-05

First tagged release. Private build: the repository stays private through
v0.9.0; the public flip (live update endpoint, winget submission, signed
production artifacts) is a later milestone.

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
- Open-in actions: open a repo's folder, terminal, editor, or GitHub remote from the app. The Windows path-handling and remote-URL-validation defects found in the audit are fixed; folder-open and remote-open are dogfood-verified, and editor/terminal resolution is unit-tested (see Notes).
- A system tray icon with the full native menu - Show RepoSync, Check All Now (checks every enabled repo), Pause all / Resume all (suspends and resumes scheduled checks), Open recent (a submenu of recently-active repos), Settings, and Quit - plus left-click-to-show and close-to-tray (the close button hides to the tray; only Quit exits). On an autostart launch the window starts hidden in the tray (E-13).
- Auto-update: RepoSync can check for a new version on launch (default on, a real toggle in Settings) and via a "Check for updates" button, then install a signed update after you confirm - it never updates silently. Every update is verified against a committed signing key before it is applied; a bad signature aborts and keeps your current version. Delivered over GitHub Releases with a winget package manifest prepared. Auto-update ships DARK in the private build (the update server is not reachable while the repo is private, and the production signing key is a human-only step); it activates at the public flip (E-18).
- Release scaffolding: version-scoped release plans under `docs/internal/release-plans/`, the cut-tag runbook, and this changelog.

### Fixed
- The activity retention sweep now runs on a daily cadence while the app is resident, not only at startup, so a long-running tray session prunes old activity rows as configured (E-09 / P3-D).
- Windows open-in defects from the audit: repository paths that used the `\\?\` extended-length prefix broke folder-open, and `repo_open_remote` executed the stored origin URL without validation. Both are fixed - paths are normalized before opening and only well-formed http/https/ssh GitHub origins are opened (P1-A).
- The status taxonomy on the dashboard "Needs attention" list now reflects each repo's true state rather than rendering every attention row as a failure (BL-NI-27), and an open detail drawer refreshes when a background check completes (BL-NI-28).

### Notes
- Private build; this repo stays private through v0.9.0. See `docs/internal/program-roadmap.md` for the effort breakdown and `docs/internal/release-plans/plan_v0.9.0/plan_v0.9.0.md` for the release plan and readiness checks.

> **Superseded 2026-07-30.** The notes above were accurate when v0.9.0 was cut
> and are preserved as the historical record. Two of them have since changed:
> the repository **went public on 2026-07-17**, and the auto-updater is
> therefore no longer blocked by repository visibility. It remains **dark** for
> the other reason given above: the production signing key is still a
> human-gated step and the shipped config carries a placeholder public key. The
> installers are still unsigned. Current status is in the
> [README](README.md#status-public-beta-and-honest-about-it).
- Desktop notifications, launch-on-login, and the system tray are wired to the OS in this release (`tauri-plugin-notification` / `tauri-plugin-autostart` / the native tray). Their Settings toggles take effect at runtime.
- The tray menu and the OS-integration surface (menu actions, close-to-tray, autostart-hidden launch, live toasts, quiet-hours suppression) are verified in the dogfood pass, not by automated tests, because they live outside the webview and the packaged shell.
- Auto-update ships DARK: the updater is wired but disabled until the maintainer generates the production signing key and the update endpoint is reachable (both are public-flip steps). See `docs/backlog.md` for the remaining deferred items.
- The Windows installers (NSIS and MSI) are unsigned: no Windows Authenticode code-signing certificate is in place yet. Expect a SmartScreen "unknown publisher" warning on install. This is separate from the auto-update dark state above; both are public-flip prerequisites.

<!--
Template for a cut release section (move [Unreleased] items here at G2).
Replace X.Y.Z with the version being cut; do not leave a real version number
here, or heading-scanning tools read it as a duplicate release section:

## [X.Y.Z] - YYYY-MM-DD

### Added
### Changed
### Fixed
### Removed
-->
