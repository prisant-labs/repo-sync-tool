# RepoSync v0.9.0 - Features and User Outcomes

- **Date:** 2026-06-30
- **Purpose:** The product-facing description of the first public release: the features it ships, the functionality behind each, and the user problem each one solves. This is the "what the user gets" companion to [feature-inventory.md](feature-inventory.md) (the build-readiness view, by command and effort) and [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan). It does not redefine scope; scope authority stays in [program-roadmap.md](../../program-roadmap.md) and each effort's `spec.md`.
- **Scope:** v0.9.0, RepoSync V1 MUST scope, Windows GA first. macOS ships as an unsigned beta if unblocked by the week-4 descope trigger, otherwise deferred.
- **Source framing:** users and problems from [PRODUCT.md](../../../../PRODUCT.md); experience commitments from [DESIGN.md](../../../../DESIGN.md); feature spine from the feature inventory and the strategy doc's V1 functional spec.

## Maturity legend

Because v0.9.0 is mid-build, each feature carries an honest maturity marker:

- **Core done** - the backend capability is built and tested behind the IPC seam; it will render once the GUI lands. This is the state of nearly every feature below.
- **Planned, schema ready** - the data model exists in the frozen schema, but the full spec is intentionally deferred. Applies to Groups/tags.
- **Follow-up** - a small, unbuilt piece with no dedicated effort yet, expected to fold into the GUI work.
- **Deferred** - explicitly out of this release (listed in Section 7 for contrast).

> The one piece that gates the release and is not itself a feature is the **webview GUI** that renders all of this. Every capability below is built behind the typed command seam (`src/lib/bindings.ts`); the screens that surface them are the remaining work before the tag. See the feature inventory for per-feature build status.

---

## 1. What v0.9.0 is

RepoSync is a resident desktop tray utility that keeps a personal library of consume-only Git repositories fresh, visible, and safe, with a transparent audit trail. It is local-first (Tauri v2 + a Rust core + a React/TypeScript shell, SQLite-backed), open source (MIT), with no telemetry, no account, and no cloud sync.

v0.9.0 is the first public build: the full V1 MUST feature set, shipping on Windows as the first real GA, with macOS kept honest in CI and released as an unsigned beta if it is unblocked in time. It is deliberately `0.9.0`, not `1.0.0`: the product is feature-complete enough to try and dogfood, ahead of the `1.0.0` stability promise.

---

## 2. The user and the problem

**Who it is for.** Technically competent developers who keep a personal library of 5 to 100 or more cloned Git repositories they *consume* rather than contribute to: self-hosted tools they run locally, reference repos read for samples, templates, and rarely-touched forks. They are comfortable with Git on the command line but do not want to babysit `git fetch` across dozens of folders. Their context is ambient, not focused: the app lives in the tray, runs all day, and is glanced at between other work.

**The job to be done.** Awareness: "what in my library is stale, what changed, what broke," answered at a glance and acted on in one click, without thinking about Git plumbing.

**The problem RepoSync solves.** A cloned-repo library goes stale silently. There is no signal that a self-hosted tool shipped a new release, that a reference repo moved on, or that an earlier manual pull left a working tree dirty. Keeping current means remembering to `git fetch` across many folders, and doing it in bulk risks clobbering local changes. RepoSync makes that silent staleness visible and keeps the library fresh safely, on a schedule, with receipts for everything it did.

**What RepoSync is not.** It is a read-and-refresh tool for repos you are not working in daily, never a Git client for repos you are. Not a CI or deployment tool, not an IDE workspace manager, not a process manager, not multi-user or team-shared in V1.

---

## 3. Build and manage the library

Turning a sprawling folder of clones into a managed, organized, at-a-glance library.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| Add a repo by path | Register a single local clone by its folder path (`repo_add_path`). | Start watching a specific repo without ceremony. | Core done |
| Scan a parent folder | Walk one folder and add every clone under it (`repo_scan_parent`), with a bounded walk (default depth 5, capped at 10k folders) so a huge tree cannot stall the app. | Onboard a whole library at once instead of one repo at a time. | Core done |
| List repos | The at-a-glance list of every tracked repo with its current state (`repo_list`). | See the whole library and its health in one place. | Core done |
| Repo detail | The full view of one repo: local vs remote, latest release, recent commits, policy, and where it lives (`repo_get`). | Drill from "this repo needs me" into exactly why. | Core done |
| Remove a repo | Stop tracking a repo (`repo_remove`). Clears the registry row and its history; it never touches the working tree on disk. | Curate the watch list without any risk to the actual clone. | Core done |
| Enable or disable per repo | Turn scheduled checks on or off for a single repo (`repo_set_enabled`); its settings survive being disabled. | Park a repo you do not care about right now without losing its configuration. | Core done |
| Groups (repo tags) | Associate a repo with one or more user-defined, colored groups, and organize or filter the library by them. The `groups` and `repo_groups` tables (a many-to-many association, name plus color) are already frozen into the schema, so "one repo, many groups" is storable today. | Impose a personal taxonomy on a large library ("self-hosted apps," "reference," "templates," "forks") instead of one flat list. | Planned, schema ready |

> **Groups/tags scope note.** Groups are committed to the initial release, and the schema scaffolding for them already exists. The detailed feature spec (exact UX, create/assign/filter flows, whether the surface reads as a sidebar of groups or as chips on each repo, and the final "Groups" vs "Tags" label) is **intentionally deferred until the GUI is finalized**, because the feature is primarily a UI surface and should be designed as one coherent screen alongside the rest of the interface. What is settled: a single taxonomy, reusing the existing schema, with repos assignable to more than one.

---

## 4. Keep repos fresh, safely

Staying current without babysitting, and without risk to uncommitted work. This is the heart of the product, and the safety model below is a first-class feature, not fine print.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| Check now | On demand, fetch and recompute one repo's state (`repo_check_now`). Reads only; it never mutates the working tree. | Get an immediate, safe answer to "is this current?" without a manual `git fetch`. | Core done |
| Scheduled background checks | A resident scheduler fetches due repos on an interval (default cadence every 6 hours), with bounded concurrency, a per-repo lock, an injected clock, and startup jitter to avoid a thundering herd on metered networks. | Keep the whole library's state current automatically, all day, with no user action. | Core done |
| Update now, fast-forward-only | Pull a repo, but only when the pull is a clean fast-forward (`repo_update_now`). Anything that would merge, rebase, or otherwise rewrite the working tree is refused. | Advance a repo to the latest upstream with zero chance of a surprising change to local state. | Core done |
| Update policy | Per-repo control over update mode (fast-forward pull or fetch-only), what to do when the tree is dirty (skip, with the reason stated), branch scope (default branch only), cadence override, and auto-pause after repeated failures (`repo_set_policy`). | Tune how aggressive RepoSync is per repo, and stop it from hammering a repo that keeps failing. | Core done |

**The safety model (a feature in its own right).** RepoSync is read-mostly, predictable, and honest by construction:

- **Fast-forward-only by default.** The default path cannot rewrite your working tree.
- **Dirty repos are skipped, with a stated reason.** A repo with uncommitted changes is left untouched and told you why, rather than being silently pulled or silently ignored.
- **Every automation has a manual equivalent and an opt-out.** Anything the scheduler does, you can trigger, pause, or disable by hand.
- **Risky behavior looks risky.** Safe defaults are presented plainly; anything that could surprise the working tree is labeled clearly and made harder to reach than the safe path.
- **Resident model, stated plainly.** The app must be running for scheduled checks to happen. There is no OS-level scheduler in V1. This is documented behavior, not a bug.

---

## 5. See, audit, and stay aware

The awareness half of the product: reading state at a glance, proving what happened, and getting told when something matters.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| State at a glance | Each repo's status is legible in its row: clean or dirty, ahead or behind (with counts), last checked, new release, error. Every state is color plus icon plus word, so it survives grayscale and color blindness. | Answer "what in my library needs me?" in a single glance, no drilling in. | Core done |
| Dashboard | The landing view: repos needing attention, recently updated, new releases, failures, and the daily summary card. | Open the app and immediately see the state of everything. | Core done (renders with the GUI) |
| Activity log and retention | Every git operation is recorded with its raw command, stdout, stderr, exit code, and timestamp, on a filterable global timeline, with configurable retention (default 90 days) and an automatic sweep (`activity_list`). | Trust through receipts: audit exactly what ran, and confirm nothing was done to a working tree that was not asked for. | Core done |
| GitHub enrichment | Unauthenticated metadata for GitHub repos: description, default branch, latest release (tag, date, URL), and topics, with aggressive caching and honest rate-limit handling that captures the reset time (`repo_refresh_metadata`). | Know when a tool you run has shipped a new release, and see repo context, without leaving the app or logging in. | Core done |
| Daily summary | A read-only, once-a-day roll-up over activity and state: what needed attention, what updated, what shipped a release, over the local-day window (`summary_today`). | Get a digest instead of watching the app all day. | Core done |
| Desktop notifications | Fire a notification on a new release or a failure, coalesced per check cycle so one sweep does not spam, and aware of quiet hours. | Ambient awareness without nagging, and without interrupting focus time. | Core done (emit-site pending edge-wiring) |

---

## 6. System integration, settings, and quick actions

The utility layer: living in the tray, starting with your session, bending to your setup, and getting you from awareness to action.

| Feature | What it does | Problem it solves | Maturity |
|---------|--------------|-------------------|----------|
| Tray presence and menu | A resident system-tray icon with a compact popup (needs attention, recently updated, new releases) and quick controls. | A background utility that is just there and glanceable, never occupying a window you have to manage. | Edge-wiring (pure Tauri chrome, no headless core; builds at launch) |
| Autostart (launch on login) | Opt-in launch when you log in, reconciling drift between the setting and the actual OS state, and refusing to actuate when the OS state cannot be read. | Have the watcher running from the moment you start work, without remembering to open it. | Core done (OS registration pending edge-wiring) |
| Settings | Global cadence, quiet hours, notify-on-release and notify-on-failure toggles, git executable path, editor and terminal commands, autostart, and activity retention (`settings_get`/`settings_set`). | Make the tool fit your machine and your preferences. | Core done |
| Honest error and degraded states | A typed error taxonomy surfaces specific, truthful failures (auth failure, missing path, deleted upstream, auto-paused) rather than vague messages (`AppError`). | Know precisely what is wrong and why, so you can fix it. | Core done |
| Quick actions | Open a repo's folder, terminal, editor, or remote in one click (`repo_open_folder` / `terminal` / `editor` / `remote`). | Jump from "this repo needs me" straight into acting on it in your own tools. | Follow-up (typed stubs; fold into the GUI) |

---

## 7. What is explicitly not in v0.9.0

Naming the boundary is part of the scope. These are deliberate exclusions, not gaps:

**Product anti-positioning (never in scope).** Not a Git client for active development (no commit-graph DAGs, no branch/tag/stash trees, no multi-verb git toolbar), not a CI or deployment tool, not an IDE workspace manager, not a process manager, not multi-user or team-shared.

**Deferred to V1.1 or later.**

- Weekly summary (the daily summary ships; weekly is a seam left for V1.1).
- Saved filters and named views over the repo list.
- Search across repos (name, path, description, remote URL, tag).
- Custom per-repo command recipes ("after update, run X").
- App auto-updater. v0.9.x updates are a manual re-download; a "check for updates" link is an open call.
- Power-aware scheduling (battery and lock awareness). V1 ships a fixed cadence plus a global pause.
- Optional Personal Access Token for higher GitHub rate limits. V1 is unauthenticated with aggressive caching; a token flow is a later upgrade.
- macOS signed GA (signing and notarization are gated on Mac hardware and Apple credentials).

**Deferred design, committed feature.** The Groups/tags **spec** is deferred until the GUI is finalized, but the feature itself is committed to this release and its schema already exists (see Section 3).

---

## 8. Platform, distribution, and honesty posture

- **Windows GA first.** Windows is the first real GA target. macOS is kept compiling and bundling in CI and ships as an unsigned beta if the week-4 descope trigger clears, otherwise it is deferred to a staged later release.
- **Unsigned-binary friction, stated up front.** Windows SmartScreen "unknown publisher" and macOS Gatekeeper will warn on first run, because code signing is deferred. This is documented in the release notes, not hidden.
- **Manual updates in 0.9.x.** With the auto-updater cut to V1.1, updating means re-downloading the latest artifact.
- **Open source and private by default.** MIT licensed, no telemetry, no crash reporting, no account, no cloud sync. All state is local, in a SQLite database.

---

## 9. The experience the release commits to

From the product and design principles, the qualities every screen must hold:

1. **State obvious at a glance.** A repo's clean/dirty, ahead/behind, last checked, and error state is legible in the row, not one click away.
2. **Transparency is the trust mechanism.** The product shows exactly what it did (raw command, exit code, stdout/stderr, timestamp) and never implies an action without the receipt available.
3. **Never hide risk behind vague language.** Risky Git behavior looks risky; safe defaults read plainly.
4. **Confidence through precision, not decoration.** Density and accuracy over flourish. No gradient, hero metric, or decorative card stands in for substance.
5. **Every automation has a manual equivalent and an opt-out.** Scheduled behavior is a convenience, not a cage.
6. **Colorblind-safe and AA throughout.** Every state is color plus icon plus word; all text, secondary included, meets WCAG 2.1 AA contrast. Status is never encoded by hue alone.
7. **Quiet in footprint, exact in content.** It idles in the tray and does not nag; when you look, it tells you everything that matters, precisely.

---

## 10. Build maturity at a glance

Every behind-the-seam core in this document is built, adversarially reviewed, and tested. What remains before the `v0.9.0` tag:

- The **webview GUI** that renders these features (dashboard, repos list and detail, activity timeline, summaries, settings, add/scan flow). This is the one release-gating item that is not itself a feature.
- The **edge-wiring** effort: spawn the scheduler at launch, wire the manual commands to shared locks, build the tray, and wire the notification and autostart plugin emit-sites.
- The **Groups/tags spec and UI**, designed once the GUI is finalized (feature committed, schema ready).
- The small **quick-actions follow-up** (`repo_open_*`).

For per-feature build status and effort ownership, see [feature-inventory.md](feature-inventory.md).
