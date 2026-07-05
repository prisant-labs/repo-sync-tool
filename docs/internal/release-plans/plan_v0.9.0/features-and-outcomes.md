# RepoSync v0.9.0 - Features and User Outcomes

- **Date:** 2026-06-30 (expanded 2026-06-30, updated 2026-07-04)
- **Purpose:** The product-facing description of the first release: the features it ships, the functionality behind each, the user problems each one solves, and the screens that carry them. This is the "what the user gets" companion to [feature-inventory.md](feature-inventory.md) (the build-readiness view, by command and effort), [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), and [product-requirements.md](../../product-requirements.md) (the PRD). It does not redefine scope; scope authority stays in [program-roadmap.md](../../program-roadmap.md) and each effort's `spec.md`.
- **Scope:** v0.9.0, RepoSync V1 MUST scope plus Groups (E-16) plus, ratified 2026-07-04, branch/PR intelligence (E-17) and auto-update/distribution (E-18). Windows GA first; macOS ships as an unsigned beta if unblocked by the week-4 descope trigger, otherwise deferred. Ships COMPLETE but on a PRIVATE repository; the public flip is a later, separate milestone (Section 13).
- **Source framing:** users and problems from [PRODUCT.md](../../../../PRODUCT.md); experience commitments from [DESIGN.md](../../../../DESIGN.md); the information architecture and screen detail from [v1-architecture-and-decisions.md](../../v1-architecture-and-decisions.md) Section (UI); the feature spine and functional enumerations from the feature inventory, the strategy doc's V1 functional spec, and the frozen schema.

## Contents

1. What v0.9.0 is
2. The user and the problem
3. What success looks like
4. A day with RepoSync (user journeys)
5. Build and manage the library
6. Keep repos fresh, safely
7. See, audit, and stay aware
8. The repo status taxonomy
9. The screens and surfaces
10. System integration, settings, and quick actions
11. Reference: functional details
12. What is explicitly not in v0.9.0
13. Platform, distribution, and honesty posture
14. The experience the release commits to
15. Build maturity at a glance

## Maturity legend

As of 2026-07-04, most of v0.9.0 is built end to end (backend and GUI); each feature still carries an honest maturity marker because "built" is not the same as "correct" (the 2026-07-04 audit found real defects in shipped work):

- **Core done** - the backend capability is built and tested behind the IPC seam and renders in the built GUI. This is the state of nearly every feature below.
- **Built** - implemented end to end (backend, IPC, and the GUI surface), and may still carry open defects tracked in the backlog and fixed in Phase 1 of [execution-plan.md](execution-plan.md). Applies to Groups and the quick actions.
- **Follow-up** - a small, unbuilt piece with no dedicated effort yet.
- **Not started** - a 2026-07-04 SHOULD addition (E-17, E-18) with a spec but no build yet.
- **Deferred** - explicitly out of this release (listed in Section 12 for contrast).

> The GUI that renders all of this, once the one piece gating the release, is now built: the full shell plus Dashboard, Repos, Activity, and Settings, plus the repo-detail drawer, landed 2026-07-03. What remains before the tag is fixing the audit's open defects, dogfooding, finishing the E-13/E-14/E-15 OS-integration wiring, and building E-17/E-18, sequenced in [execution-plan.md](execution-plan.md). See the feature inventory for per-feature build status.

---

## 1. What v0.9.0 is

RepoSync is a resident desktop tray utility that keeps a personal library of consume-only Git repositories fresh, visible, and safe, with a transparent audit trail. It is local-first (Tauri v2 + a Rust core + a React/TypeScript shell, SQLite-backed), open source (MIT), with no telemetry, no account, and no cloud sync.

v0.9.0 is the first complete release: the full V1 MUST feature set plus Groups, plus (ratified 2026-07-04) branch/PR intelligence and auto-update/distribution, shipping on Windows as the first real GA, with macOS kept honest in CI and released as an unsigned beta if it is unblocked in time. It is deliberately `0.9.0`, not `1.0.0`: the product is feature-complete enough to try and dogfood, ahead of the `1.0.0` stability promise. It ships complete, including the full release ceremony, but on a private repository; the public flip is a separate later milestone (Section 13).

---

## 2. The user and the problem

**Who it is for.** Technically competent developers who keep a personal library of 5 to 100 or more cloned Git repositories they *consume* rather than contribute to: self-hosted tools they run locally, reference repos read for samples, templates, and rarely-touched forks. They are comfortable with Git on the command line but do not want to babysit `git fetch` across dozens of folders. Their context is ambient, not focused: the app lives in the tray, runs all day, and is glanced at between other work.

**The job to be done.** Awareness: "what in my library is stale, what changed, what broke," answered at a glance and acted on in one click, without thinking about Git plumbing.

**The problem RepoSync solves.** A cloned-repo library goes stale silently. There is no signal that a self-hosted tool shipped a new release, that a reference repo moved on, or that an earlier manual pull left a working tree dirty. Keeping current means remembering to `git fetch` across many folders, and doing it in bulk risks clobbering local changes. RepoSync makes that silent staleness visible and keeps the library fresh safely, on a schedule, with receipts for everything it did.

**What RepoSync is not.** It is a read-and-refresh tool for repos you are not working in daily, never a Git client for repos you are. Not a CI or deployment tool, not an IDE workspace manager, not a process manager, not multi-user or team-shared in V1.

---

## 3. What success looks like

The release is successful when a user can:

- Add 30 or more repos in one action and immediately see each one's state (clean or dirty, ahead or behind, last fetched, new release, error) without configuring anything.
- Glance at the tray or the Dashboard and know, in seconds, whether anything wants attention, and open exactly that repo in one click.
- Trust that nothing was done to a working tree that was not asked for, because the default path is fast-forward-only and dirty repos are skipped with a stated reason.
- Audit exactly what ran (raw command, exit code, output, timestamp) for any operation, at any time.
- Leave the app running for weeks and have it stay quiet until something genuinely changes, then be told in a way that respects quiet hours.

These are the outcomes every feature below serves. If a feature does not move one of these, it does not belong in V1.

---

## 4. A day with RepoSync (user journeys)

Concrete moments that show the features combining into the value loop, rather than standing alone.

**Onboarding a library.** A user points RepoSync at `~/code/tools` and runs a scan. The bounded walk (depth 5, capped at 10k folders) finds 34 clones and registers them in one action. Within a sweep, every repo shows a real status: 28 in sync, 4 behind, 1 dirty, 1 auth-failed. No per-repo setup was required. *Features: scan a parent folder, list repos, state at a glance, scheduled check.*

**The morning glance.** The user sits down and looks at the tray icon: it carries an attention badge. The Dashboard says "three repos want a look." One is a self-hosted tool 47 commits behind, one has a new release, one failed to fetch on an expired credential. Each is one click into its detail. *Features: tray presence, Dashboard needs-attention, status taxonomy, quick navigation.*

**A repo held back, safely.** A repo the user tinkered in last week has 23 uncommitted changes. RepoSync fetched it, saw 8 incoming commits, and skipped the pull because the policy is "skip if dirty." The repo shows a Dirty status with the exact reason, and offers "Review changes" or "Pull anyway." Nothing touched the working tree. *Features: check now, update policy (dirty handling), ff-only safety, honest status.*

**A new release lands.** A tool the user runs locally publishes `v2.0.5`. On the next enrichment pass RepoSync detects the release, and (outside quiet hours) fires one coalesced notification. The repo's detail shows the release card and, because the local tree is clean, offers a one-click fast-forward. *Features: GitHub enrichment, latest-release detection, notifications, ff-only update.*

**Proving what happened.** A week later the user wonders why a repo is on an odd commit. They open Activity, filter to that repo, and read the exact sequence: scheduled fetch, fast-forward pull with the raw `git` command and its output, exit code, and timestamp. The receipt answers the question with no guessing. *Features: activity log, retention, transparency.*

---

## 5. Build and manage the library

Turning a sprawling folder of clones into a managed, organized, at-a-glance library.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| Add a repo by path | Register a single local clone by its folder path (`repo_add_path`). | Start watching a specific repo without ceremony. | Core done |
| Scan a parent folder | Walk one folder and add every clone under it (`repo_scan_parent`), with a bounded walk (default depth 5, capped at 10k folders) so a huge tree cannot stall the app. | Onboard a whole library at once instead of one repo at a time. | Core done |
| List repos | The at-a-glance list of every tracked repo with its current state (`repo_list`). | See the whole library and its health in one place. | Core done |
| Repo detail | The full view of one repo: local vs remote, latest release, recent commits, policy, and where it lives (`repo_get`). | Drill from "this repo needs me" into exactly why. | Core done |
| Remove a repo | Stop tracking a repo (`repo_remove`). Clears the registry row and its history; it never touches the working tree on disk. | Curate the watch list without any risk to the actual clone. | Core done |
| Enable or disable per repo | Turn scheduled checks on or off for a single repo (`repo_set_enabled`); its settings survive being disabled. | Park a repo you do not care about right now without losing its configuration. | Core done |
| Groups (repo tags) | Associate a repo with one or more user-defined, colored groups, and organize or filter the library by them, via a Groups nav, create/assign/filter flows, and per-row chips. The `groups` and `repo_groups` tables (a many-to-many association, name plus color) are frozen into the schema, so "one repo, many groups" is stored today. | Impose a personal taxonomy on a large library ("self-hosted apps," "reference," "templates," "forks") instead of one flat list. | Built |

> **Groups/tags scope note.** Groups were promoted to MUST-tier scope as E-16 on 2026-06-30 and were fully built 2026-07-03: the store and IPC layer (commit a85e0fc) and the GUI, a Groups sidebar nav, create/assign/filter, and per-row chips (commit 51daaa7). The feature spec was written retroactively, after the build, as the as-built contract in [E-16-groups/spec.md](E-16-groups/spec.md), rather than designed ahead of the build as originally planned. Settled: a single taxonomy, reusing the `groups`/`repo_groups` schema, with repos assignable to more than one. Known defects: BL-NI-22 (the per-repo group-membership query is O(N), one IPC round-trip per visible row) and the group filter false-empties during load or on a fan-out failure; both are Phase 1 fixes in [execution-plan.md](execution-plan.md).

---

## 6. Keep repos fresh, safely

Staying current without babysitting, and without risk to uncommitted work. This is the heart of the product, and the safety model below is a first-class feature, not fine print.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| Check now | On demand, fetch and recompute one repo's state (`repo_check_now`). Reads only; it never mutates the working tree. | Get an immediate, safe answer to "is this current?" without a manual `git fetch`. | Core done |
| Scheduled background checks | A resident scheduler fetches due repos on an interval (default cadence every 6 hours), with bounded concurrency, a per-repo lock, an injected clock, and startup jitter to avoid a thundering herd on metered networks. | Keep the whole library's state current automatically, all day, with no user action. | Core done |
| Update now, fast-forward-only | Pull a repo, but only when the pull is a clean fast-forward (`repo_update_now`). Anything that would merge, rebase, or otherwise rewrite the working tree is refused. | Advance a repo to the latest upstream with zero chance of a surprising change to local state. | Core done |
| Update policy | Per-repo control over update mode (`check_only` / `fetch_only` / `pull_ff_only`), what to do when the tree is dirty (skip, with the reason stated), branch scope (default branch only), cadence override, and auto-pause after repeated failures (`repo_set_policy`). | Tune how aggressive RepoSync is per repo, and stop it from hammering a repo that keeps failing. | Core done |

**The safety model (a feature in its own right).** RepoSync is read-mostly, predictable, and honest by construction:

- **Fast-forward-only by default.** The default path cannot rewrite your working tree.
- **Dirty repos are skipped, with a stated reason.** A repo with uncommitted changes is left untouched and told you why, rather than being silently pulled or silently ignored.
- **Three-strikes auto-pause.** A repo that fails repeatedly is automatically paused (tracked by `consecutive_failures` and `auto_paused` in the schema) so it stops being retried on every sweep, and it says so.
- **Every automation has a manual equivalent and an opt-out.** Anything the scheduler does, you can trigger, pause, or disable by hand.
- **Risky behavior looks risky.** Safe defaults are presented plainly; anything that could surprise the working tree is labeled clearly and made harder to reach than the safe path.
- **Resident model, stated plainly.** The app must be running for scheduled checks to happen. There is no OS-level scheduler in V1. This is documented behavior, not a bug.

---

## 7. See, audit, and stay aware

The awareness half of the product: reading state at a glance, proving what happened, and getting told when something matters.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| State at a glance | Each repo's status is legible in its row: clean or dirty, ahead or behind (with counts), last checked, new release, error. Every state is color plus icon plus word, so it survives grayscale and color blindness. | Answer "what in my library needs me?" in a single glance, no drilling in. | Core done |
| Dashboard | The landing view: repos needing attention, recently updated, new releases, failures, and the daily summary card. | Open the app and immediately see the state of everything. | Core done (renders with the GUI) |
| Activity log and retention | Every git operation is recorded with its raw command, stdout, stderr, exit code, and timestamp, on a filterable global timeline, with configurable retention (default 90 days) and an automatic sweep (`activity_list`). | Trust through receipts: audit exactly what ran, and confirm nothing was done to a working tree that was not asked for. | Core done |
| GitHub enrichment | Unauthenticated metadata for GitHub repos: description, default branch, latest release (tag, date, URL), and topics, with aggressive caching and honest rate-limit handling that captures the reset time (`repo_refresh_metadata`). | Know when a tool you run has shipped a new release, and see repo context, without leaving the app or logging in. | Core done |
| Daily summary | A read-only, once-a-day roll-up over activity and state: what needed attention, what updated, what shipped a release, over the local-day window (`summary_today`). | Get a digest instead of watching the app all day. | Core done |
| Desktop notifications | Fire a notification on a new release or a failure, coalesced per check cycle so one sweep does not spam, and aware of quiet hours. | Ambient awareness without nagging, and without interrupting focus time. | Core done (the `tauri-plugin-notification` emit-site is Phase 3 of [execution-plan.md](execution-plan.md)) |

---

## 8. The repo status taxonomy

"State at a glance" is the core value, so the state vocabulary is worth stating precisely. Every sync state is encoded as **color plus icon plus word** (never hue alone), so it survives grayscale and color blindness. The delta counts (`Δ`) are shown separately as `↑N` (ahead) and `↓N` (behind).

**The six sync states** describe the repo's synchronization status with its remote:

| State | Meaning | Encoding | Notes |
|-------|---------|----------|-------|
| In sync | Local matches the tracked remote; nothing to do. | Green, check icon, "in sync" | The quiet, common case. |
| Behind | The remote has commits the local clone does not (`↓N`). | Indigo (deliberately not the blue interaction accent), down-arrow, "behind" | A clean behind can be fast-forwarded; a dirty behind is held back. |
| Ahead | The local clone has commits the remote does not (`↑N`). | Green delta on the row | Shown as a delta; RepoSync never pushes. |
| Dirty | The working tree has uncommitted changes; the pull was skipped. | Amber, warning icon, "dirty" | Untouched by design; the reason is stated, with "Review changes" / "Pull anyway". |
| Failed | An operation errored: auth failure, missing path, or deleted upstream. | Red, alert icon, "failed" | Carries the specific error code and a remediation hint. |
| Paused | Scheduled checks are off for this repo, either by the user or by three-strikes auto-pause. | First-class "paused" pill (`check_only` mode) | Distinct status, not a greyed-out row; its config survives. |

**Signals: branch and release intelligence** are separate from the sync-state taxonomy and are rendered in the signal register (magenta, status-release color) as distinct indicators layered on top of the status:

| Signal | Meaning | Encoding | Notes |
|--------|---------|----------|-------|
| New release | A newer GitHub release than the local checkout was detected. | Magenta package icon, release tag | Surfaced as a chip on the row, the Dashboard, and (outside quiet hours) as a notification; never as a status color. |
| Pull requests | Open pull requests on the repo (GitHub only). | Magenta PR icon, open count | Surfaced as a chip on the row; unavailable/unknown for non-GitHub repos or when unauthenticated access cannot reach the repo. |

---

## 9. The screens and surfaces

The GUI is five primary areas in a persistent left sidebar (a `Workspace` group), plus a `Groups` section for the taxonomy, plus two tray surfaces. The sidebar footer always shows a live "Watching N repos / next sweep in Xh Ym" status. The canonical design language is the Graphite direction in `DESIGN.md`; the earlier draft mockups were archived to `_local/gui/archived-mockups/`.

| Surface | What it shows | What you do here | Mockup |
|---------|---------------|------------------|--------|
| **Dashboard** | The at-a-glance home: hero stats (repos under watch, need attention, updated today, new releases), a "Needs attention" card grid, a recently-updated list, and the prose daily summary ("Today's read"). | Answer "does anything want me right now?"; jump into the repo, activity, or summary behind any item. | `01-dashboard-mac-light`, `08-dashboard-windows-mica` |
| **Repos** | The full library as a sortable, filterable table (filter chips: All, In sync, Behind, Dirty, Failed, Paused, New release) with a `460px` detail drawer. | Filter and sort the library; select a repo to see local-vs-remote, latest release, recent commits, and where it lives; run quick actions. | `02-repos-mac-dark`, `07-repo-detail-mac-light` |
| **Activity** | The chronological log of everything RepoSync did: fetches, fast-forwards, releases detected, skips, failures, scheduled sweeps, each with raw command output. | Filter by repo, action type, or status; read the exact receipt for any operation. | `04-activity-windows-dark` |
| **Summaries** | The archive of auto-generated daily read-outs (the prose digests surfaced on the Dashboard), exportable to Markdown. | Read the narrative over time; export a digest. | (nav present; no dedicated mockup yet) |
| **Settings** | DB-backed configuration: cadence, per-repo mode, dirty policy, quiet hours, notify toggles, git executable, editor/terminal commands, autostart, retention, groups, theme. | Make the tool fit your machine and preferences. | `06-settings-mac-dark` |
| **Onboarding / empty state** | The first-run experience for zero repos: add or scan to begin. | Get from install to a watched library. | `05-onboarding-windows-light` |
| **Tray icon** | The always-resident presence; communicates aggregate state with a colored badge dot when something needs attention. | Read state without opening anything. | (in tray-popup mockup) |
| **Tray menu / popup** | A compact surface with attention items, new releases, and recent updates, plus "Check all now" and "Open RepoSync." | The fast path that keeps the full window optional. | `03-tray-popup` |

> **Tray surface scope.** V1 ships the **native right-click tray menu**. The frameless left-click **popup window** shown in `03-tray-popup` is cut to V1.1 (backlog `BL-V11-01`) because its anchored geometry is OS-specific and unverifiable on Windows-only hardware; the native menu covers the essential interaction. **Summaries** and **Settings** full pages exist in nav but have no dedicated mockup yet and need layouts as part of the GUI effort.

---

## 10. System integration, settings, and quick actions

The utility layer: living in the tray, starting with your session, bending to your setup, and getting you from awareness to action.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| Tray presence and menu | A resident system-tray icon with a native right-click menu (needs attention, recently updated, new releases) and quick controls. | A background utility that is just there and glanceable, never occupying a window you have to manage. | Built, PARTIAL (Show, Quit, and left-click-show shipped 2026-07-03, commit bb353f9; Check All Now, Pause/Resume, Open recent, the Settings menu item, and close-to-tray remain, Phase 3 of [execution-plan.md](execution-plan.md)) |
| Autostart (launch on login) | Opt-in launch when you log in, reconciling drift between the setting and the actual OS state, and refusing to actuate when the OS state cannot be read. | Have the watcher running from the moment you start work, without remembering to open it. | Core done (OS registration is Phase 3 of [execution-plan.md](execution-plan.md)) |
| Settings | Global cadence, quiet hours, notify-on-release and notify-on-failure toggles, git executable path, editor and terminal commands, autostart, and activity retention (`settings_get`/`settings_set`). | Make the tool fit your machine and your preferences. | Core done |
| Honest error and degraded states | A typed error taxonomy surfaces specific, truthful failures (auth failure, missing path, deleted upstream, auto-paused) rather than vague messages (`AppError`). | Know precisely what is wrong and why, so you can fix it. | Core done |
| Quick actions | Open a repo's folder, terminal, editor, or remote in one click (`repo_open_folder` / `terminal` / `editor` / `remote`). | Jump from "this repo needs me" straight into acting on it in your own tools. | Built, with open defects (implemented 2026-07-03, commit 8fc806c; broken on Windows, including a security defect in `repo_open_remote`, per the audit findings 1-2 and 8-9; fixed in Phase 1 of [execution-plan.md](execution-plan.md)) |

---

## 11. Reference: functional details

The precise enumerations behind the features above, drawn from the frozen schema and the effort specs. Useful when building the screens against the real command surface.

**Update modes (per repo).**

- `check_only` - fetch and report state, never pull. The safest mode; also what a paused repo effectively behaves like.
- `fetch_only` - fetch and update remote-tracking refs, but do not touch the working tree.
- `pull_ff_only` - fetch, and if the working tree is clean and the update is a clean fast-forward, pull. The default.
- **Dirty policy:** skip (the default) leaves a dirty tree untouched with a stated reason.
- **Branch scope:** default branch only in V1.
- **Cadence:** a per-repo override of the global interval (default 6 hours).
- **Auto-pause:** three consecutive failures pause the repo (`consecutive_failures`, `auto_paused`).

**Activity records.** Every operation writes an audit row. Action types: `check`, `fetch`, `pull_ff`, `pull`, `rebase`, `open`, `enable`, `disable`, `manual_retry`. Statuses: `success`, `skipped`, `warning`, `failed`. Each row keeps the raw command, stdout, stderr, exit code, duration, and (where relevant) the commit range. Retention defaults to 90 days and is swept automatically.

**Error taxonomy.** `AppError` is a typed enum of about thirty variants grouped by domain (git, filesystem, network/GitHub, database, config), each with a stable machine code and a remediation string. This is what lets a Failed status say *which* failure it was (for example: expired credential vs. missing path vs. deleted upstream) rather than a generic error.

**Settings inventory (the singleton row).** `global_check_minutes` (default 360), `quiet_hours_start` / `quiet_hours_end` (minutes since midnight), `notify_on_release` (default on), `notify_on_failure` (default on), `git_executable_path`, `editor_command`, `terminal_command`, `autostart` (default off), `activity_retention_d` (default 90), `github_token_present` (V1 always false; the keyring PAT is V1.1).

**GitHub enrichment (unauthenticated).** Description, default branch, latest release (tag, date, URL), and topics, cached with ETag conditional requests and rate-limit backoff that records the reset time. The authenticated PAT path is stubbed behind a seam for V1.1.

---

## 12. What is explicitly not in v0.9.0

Naming the boundary is part of the scope. These are deliberate exclusions, not gaps:

**Product anti-positioning (never in scope).** Not a Git client for active development (no commit-graph DAGs, no branch/tag/stash trees, no multi-verb git toolbar), not a CI or deployment tool, not an IDE workspace manager, not a process manager, not multi-user or team-shared.

**Deferred to V1.1 or later.**

- Weekly summary (the daily summary ships; weekly is a seam left for V1.1).
- Saved filters and named views over the repo list.
- Search across repos (name, path, description, remote URL, tag).
- Custom per-repo command recipes ("after update, run X").
- The frameless tray popup window (the native tray menu ships; the popup is V1.1, `BL-V11-01`).
- Power-aware scheduling (battery and lock awareness). V1 ships a fixed cadence plus a global pause.
- Optional Personal Access Token for higher GitHub rate limits. V1 is unauthenticated with aggressive caching; a token flow is a later upgrade.
- macOS signed GA (signing and notarization are gated on Mac hardware and Apple credentials).
- winget **submission** (the manifest is prepared and verified as part of E-18, but publishing it to the winget-pkgs repo waits for the public flip, since winget requires public artifact URLs).

**No longer deferred, ratified into scope 2026-07-04.** App auto-updater: previously cut to V1.1 with manual re-download as the plan, this is now E-18 (auto-update and distribution), built in Phase 4 of [execution-plan.md](execution-plan.md). Its update-check endpoint is private-repo-only until the public flip.

**Deferred design, committed feature, now built.** Groups/tags was originally planned with its spec deferred until the GUI was finalized; instead the feature was built ahead of its spec (2026-07-03), and the spec was written retroactively (E-16, see Section 5).

---

## 13. Platform, distribution, and honesty posture

- **Windows GA first.** Windows is the first real GA target. macOS is kept compiling and bundling in CI and ships as an unsigned beta if the week-4 descope trigger clears, otherwise it is deferred to a staged later release.
- **Unsigned-binary friction, stated up front.** Windows SmartScreen "unknown publisher" and macOS Gatekeeper will warn on first run, because code signing is deferred. This is documented in the release notes, not hidden.
- **Private-repo posture for this release (ratified 2026-07-04).** v0.9.0 ships COMPLETE, including the full release ceremony (tag, GitHub Release, installer artifacts), but on a private repository. The updater's `latest.json` manifest is hosted and verified privately; winget submission is prepared but withheld, since winget requires public artifact URLs. Both wait for a later, separate "public flip" milestone.
- **In-app updates via E-18.** The auto-updater, previously cut to V1.1, is now in scope (Section 12); it checks the private update endpoint above.
- **Open source and private by default (product posture, distinct from the private-repo posture above).** MIT licensed, no telemetry, no crash reporting, no account, no cloud sync. All state is local, in a SQLite database.

---

## 14. The experience the release commits to

From the product and design principles, the qualities every screen must hold:

1. **State obvious at a glance.** A repo's clean/dirty, ahead/behind, last checked, and error state is legible in the row, not one click away.
2. **Transparency is the trust mechanism.** The product shows exactly what it did (raw command, exit code, stdout/stderr, timestamp) and never implies an action without the receipt available.
3. **Never hide risk behind vague language.** Risky Git behavior looks risky; safe defaults read plainly.
4. **Confidence through precision, not decoration.** Density and accuracy over flourish. No gradient, hero metric, or decorative card stands in for substance.
5. **Every automation has a manual equivalent and an opt-out.** Scheduled behavior is a convenience, not a cage.
6. **Colorblind-safe and AA throughout.** Every state is color plus icon plus word; all text, secondary included, meets WCAG 2.1 AA contrast. Status is never encoded by hue alone.
7. **Quiet in footprint, exact in content.** It idles in the tray and does not nag; when you look, it tells you everything that matters, precisely.

---

## 15. Build maturity at a glance

As of 2026-07-04: every behind-the-seam core is built, adversarially reviewed, and tested; the **webview GUI** (dashboard, repos list and detail, activity timeline, settings, add/scan flow), **Groups** (E-16, with its spec written retroactively), and the **quick actions** (`repo_open_*`) are all built too. What remains before the `v0.9.0` tag, sequenced in [execution-plan.md](execution-plan.md):

- **Correctness.** Fix the 2026-07-04 audit's findings: the opener defects (broken open-in on Windows, the unvalidated-remote-URL security defect, `cmd /C` injection), the scheduler cadence gaps, and the frontend defects (group filter false-empty, the Dashboard attention-row taxonomy, drawer staleness, an accessibility batch). Also repair PR #2's CI (red on all four checks) and the test suite (does not complete in a reasonable time).
- **Dogfood.** Run the real, packaged app and exercise every flow; fix what falls out.
- **OS-integration completion.** Finish E-13 (tray): Check All Now, Pause/Resume, Open recent, the Settings menu item, close-to-tray. Wire E-14 (notifications) and E-15 (autostart) to their OS plugins.
- **New features.** E-17 (branch and PR intelligence) and E-18 (auto-update and distribution), ratified 2026-07-04, not started.
- **Packaging and the private release ceremony.** Build and smoke-test the Windows installer; merge PR #2; tag `v0.9.0`; cut a private GitHub Release.

For per-feature build status and effort ownership, see [feature-inventory.md](feature-inventory.md).
