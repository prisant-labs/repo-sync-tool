# RepoSync v0.9.0 - Product Requirements

- **Version:** v0.9.0 (first release; private until the public flip)
- **Status:** in build, feature-complete target
- **Updated:** 2026-07-04
- **Owner:** jprisant
- **Purpose:** The single requirements view for v0.9.0. It states the problem, the users, the principles, the scope as a table of capabilities mapped to owning efforts, the non-goals, the measurable success criteria, the release posture, and the constraints. Requirements aggregate here; acceptance criteria live in each effort's `spec.md` and are never restated in this document.
- **Sources:** [PRODUCT.md](../../PRODUCT.md) (users, purpose, principles), [DESIGN.md](../../DESIGN.md) (experience commitments), [features-and-outcomes.md](release-plans/plan_v0.9.0/features-and-outcomes.md) (product-facing feature detail), [feature-inventory.md](release-plans/plan_v0.9.0/feature-inventory.md) (build readiness), [program-roadmap.md](program-roadmap.md) (scope ledger, dependency graph, descope triggers), [plan_v0.9.0.md](release-plans/plan_v0.9.0/plan_v0.9.0.md) (release plan).

---

## 1. Problem statement

A cloned-repo library goes stale silently. A developer keeps anywhere from 5 to 100 or more Git repositories on disk that they consume rather than contribute to: self-hosted tools run locally, reference repos read for samples, templates, rarely-touched forks. Nothing signals that a self-hosted tool shipped a new release, that a reference repo moved on, or that an earlier manual pull left a working tree dirty. Staying current means remembering to `git fetch` across many folders, and doing that in bulk risks clobbering local changes.

RepoSync makes that silent staleness visible and keeps the library fresh safely, on a schedule, with receipts for everything it did. It is a resident desktop tray utility (Tauri v2 + a Rust core + a React/TypeScript shell, SQLite-backed, local-first) with a richer main window for management and review. It is read-mostly, predictable, and honest by construction: fast-forward-only by default, dirty repos skipped with a stated reason, every git invocation logged with its raw command and output.

## 2. Users and jobs to be done

**Who it is for.** Technically competent developers who keep a personal library of consume-only Git clones. They are comfortable with Git on the command line but do not want to babysit `git fetch` across dozens of folders. Their context when using RepoSync is ambient, not focused: the app lives in the system tray, runs all day, and is glanced at between other work. This is explicitly the consumer-repo user, not the active-development user. RepoSync is a read-and-refresh tool for repos you are not working in daily, never a Git client for repos you are.

**The job to be done.** Awareness: "what in my library is stale, what changed, what broke," answered at a glance and acted on in one click, without thinking about Git plumbing. Concretely:

1. Keep selected local repositories fresh and visible, at a glance.
2. Understand what changed without reading raw commit hashes.
3. Automate updates safely, read-mostly by default, with per-repo opt-in to anything riskier.
4. Get from awareness to action (open folder, terminal, editor, remote) without leaving the tool.

Full user framing and the day-with-RepoSync journeys live in [PRODUCT.md](../../PRODUCT.md) and [features-and-outcomes.md](release-plans/plan_v0.9.0/features-and-outcomes.md) Section 4.

## 3. Product principles

The product principles are canonical in [PRODUCT.md](../../PRODUCT.md) (Design Principles) and the experience commitments in [DESIGN.md](../../DESIGN.md). They are not restated here; requirements below must satisfy them. In brief, they are: state obvious at a glance; transparency is the trust mechanism (always show the receipt); never hide risk behind vague language; confidence through precision, not decoration; every automation has a manual equivalent and an opt-out; colorblind-safe and WCAG 2.1 AA throughout (status is never encoded by hue alone); quiet in footprint, exact in content.

## 4. v0.9.0 scope

Every capability maps to an owning effort with its own `spec.md` (the contract, including acceptance criteria) and `implementation-plan.md`. Tier is MUST (required for v0.9.0) or SHOULD (in scope, first to be cut under a descope trigger). Efforts E-01 through E-15 are the original functional and integration set; E-16 through E-18 are the additions carried by the v0.9.0 ship plan. Build status is tracked live in [plan_v0.9.0.md](release-plans/plan_v0.9.0/plan_v0.9.0.md); this table states scope and ownership, not status.

**Scope at a glance.** 18 efforts plus the release-gating GUI. Twelve are MUST (E-01 through E-09, E-12, E-13, E-16); six are SHOULD (E-10, E-11, E-14, E-15, E-17, E-18 - the enrichment, summary, notification, autostart, branch-intelligence, and distribution efforts). SHOULD efforts are in scope for v0.9.0 and are the first candidates for a descope trigger; the pre-committed triggers live in [program-roadmap.md](program-roadmap.md).

### 4.1 Platform foundation (infrastructure efforts)

| Capability | Tier | Owning effort (handle) | Spec |
|---|---|---|---|
| Cargo workspace, `reposync-core` + `src-tauri` skeletons, repo hygiene, CI matrix + dependency-hygiene gate | MUST | E-01 (foundation, workspace, CI) | [spec](release-plans/plan_v0.9.0/E-01-foundation/spec.md) |
| SQLite schema as numbered migrations, `sqlx` runner, WAL pool, `paths` seam, migration-failure recovery | MUST | E-02 (persistence and paths) | [spec](release-plans/plan_v0.9.0/E-02-persistence/spec.md) |
| Git engine: CLI network/mutation + `git2` reads behind a `GitEngine` trait, git discovery + version floor, git-not-found state | MUST | E-03 (git engine) | [spec](release-plans/plan_v0.9.0/E-03-git-engine/spec.md) |
| Git fixture test harness: bare + working repo pairs for all 7 states, pinned git in CI, `git2`-vs-CLI cross-check | MUST | E-04 (git fixture harness) | [spec](release-plans/plan_v0.9.0/E-04-git-fixture-harness/spec.md) |
| Error taxonomy: `AppError`, ~30 typed codes with remediation strings, `serde` + `specta::Type` | MUST | E-05 (error taxonomy) | [spec](release-plans/plan_v0.9.0/E-05-error-taxonomy/spec.md) |
| Typed IPC contract: the command + event surface as Rust types with `tauri-specta` TS codegen, version pin + fallback | MUST | E-06 (IPC contract, the typed seam) | [spec](release-plans/plan_v0.9.0/E-06-ipc-contract/spec.md) |
| Tracer bullet + packaging spike: `repo_add_path` + `repo_check_now` end to end on a real Windows build; early MSI from CI | MUST | E-12 (tracer bullet, packaging spike) | [spec](release-plans/plan_v0.9.0/E-12-tracer-bullet/spec.md) |

### 4.2 Build and manage the library

Turning a sprawling folder of clones into a managed, organized, at-a-glance library.

| Capability | Tier | Owning effort (handle) | Spec |
|---|---|---|---|
| Add a repo by path; scan a parent folder (bounded walk, depth 5, capped at 10k folders); list; repo detail; remove; enable or disable per repo; settings | MUST | E-02 (persistence and paths) | [spec](release-plans/plan_v0.9.0/E-02-persistence/spec.md) |
| Groups (repo tags): user-defined, colored, many-to-many labels; organize and filter the library by them | MUST (promoted 2026-06-30) | E-16 (groups, repo tags) | [spec](release-plans/plan_v0.9.0/E-16-groups/spec.md) |

### 4.3 Keep repos fresh, safely

| Capability | Tier | Owning effort (handle) | Spec |
|---|---|---|---|
| Update-policy engine: pure `(repo state, policy) -> action or skip-with-reason`; modes (`check_only` / `fetch_only` / `pull_ff_only`), dirty handling, branch scope, cadence override, three-strikes auto-pause; `repo_update_now` / `repo_set_policy` / `repo_check_now` | MUST | E-07 (update-policy engine) | [spec](release-plans/plan_v0.9.0/E-07-update-policy-engine/spec.md) |
| Scheduled background checks: `tokio` interval, `next_check_at`, jitter, bounded concurrency, quiet hours, per-repo async mutex, injected clock | MUST | E-08 (scheduler) | [spec](release-plans/plan_v0.9.0/E-08-scheduler/spec.md) |

**The safety model is a first-class requirement**, carried across E-07 (update-policy engine) and E-08 (scheduler), not fine print:

- Fast-forward-only by default; the default path cannot rewrite a working tree.
- Dirty repos are skipped and told why, rather than silently pulled or silently ignored.
- Three-strikes auto-pause stops a repeatedly-failing repo from being retried every sweep, and it says so.
- Every automation has a manual equivalent and an opt-out; a repo's settings survive being disabled.
- Risky behavior looks risky; anything that could surprise the working tree is labeled and made harder to reach than the safe path.
- The resident model is stated plainly: the app must be running for scheduled checks to happen.

### 4.4 See, audit, and stay aware

Reading state at a glance, proving what happened with receipts, and being told when something genuinely matters.

| Capability | Tier | Owning effort (handle) | Spec |
|---|---|---|---|
| Activity log writer + retention: every git op recorded with raw command, stdout, stderr, exit code, timestamp; filterable timeline; retention default 90 days with automatic sweep; `activity_list` | MUST | E-09 (activity log writer, retention) | [spec](release-plans/plan_v0.9.0/E-09-activity-log/spec.md) |
| GitHub enrichment (unauthenticated): description, default branch, latest release, topics; ETag conditional requests; rate-limit backoff capturing the reset time; `repo_refresh_metadata` | SHOULD | E-10 (GitHub metadata client) | [spec](release-plans/plan_v0.9.0/E-10-github-client/spec.md) |
| Daily summary: read-only, once-a-day roll-up over activity and state; `summary_today` (weekly left as a V1.1 seam) | SHOULD | E-11 (summary engine, daily) | [spec](release-plans/plan_v0.9.0/E-11-summary-engine/spec.md) |
| Branch and PR intelligence: surface branch state and pull-request context for tracked repos | SHOULD (ratified 2026-07-04) | E-17 (branch and PR intelligence) | [spec](release-plans/plan_v0.9.0/E-17-branch-intel/spec.md) |

**State-at-a-glance is the core requirement**, so the state vocabulary is stated precisely. Every state is encoded as color plus icon plus word, never hue alone, so it survives grayscale and color blindness. The sync-state taxonomy comprises six states: in sync, behind (`↓N`), ahead (`↑N`), dirty (skipped, reason stated), failed (with the specific error code and a remediation hint), and paused (a first-class pill, not a greyed-out row). Delta counts are shown separately as `↑N` (ahead) and `↓N` (behind). Release and branch intelligence (new release, open PRs) are separate signals rendered in the signal register (magenta, status-release color), never as status states, honoring the Status-Owns-Saturation rule (DESIGN.md). The full taxonomy with encodings is in [features-and-outcomes.md](release-plans/plan_v0.9.0/features-and-outcomes.md) Section 8; the visual tokens are in [DESIGN.md](../../DESIGN.md).

#### Degradation requirements

Intelligence features degrade gracefully rather than erroring. A non-GitHub remote shows local intelligence only (ahead/behind, dirty, recency); the GitHub-only signals (pull requests, releases) are reported as unavailable, never as an error. Offline or rate-limited states retain the last-known values and show them with an as-of timestamp, rather than blanking the row or spamming a failure. A missing upstream is shown as an explicit unknown state, never a fabricated zero. This applies across the intelligence-bearing capabilities (E-10 GitHub enrichment and E-17 branch and PR intelligence), whose per-capability acceptance criteria enforce it in their own specs.

### 4.5 System integration and distribution

| Capability | Tier | Owning effort (handle) | Spec |
|---|---|---|---|
| Tray native menu: right-click menu (Show / Check All Now / Pause-Resume / Open recent / Settings / Quit) wired to commands; close-to-tray | MUST | E-13 (tray native menu) | [spec](release-plans/plan_v0.9.0/E-13-tray-menu/spec.md) |
| Desktop notifications: OS toasts for new release / failure, coalesced per check cycle, quiet-hours aware, via `tauri-plugin-notification` | SHOULD | E-14 (desktop notifications) | [spec](release-plans/plan_v0.9.0/E-14-notifications/spec.md) |
| Autostart (launch on login): opt-in launch keyed off the `autostart` setting via `tauri-plugin-autostart`, with startup reconciliation | SHOULD | E-15 (autostart, launch on login) | [spec](release-plans/plan_v0.9.0/E-15-autostart/spec.md) |
| Auto-update and distribution: in-app updater plus the winget manifest and updater infrastructure, built and verified now (submission and endpoint activation wait for the public flip) | SHOULD (ratified 2026-07-04) | E-18 (auto-update and distribution) | [spec](release-plans/plan_v0.9.0/E-18-auto-update/spec.md) |

### 4.6 The GUI (gates the release, not an effort)

The webview GUI that renders every capability above (Dashboard, Repos list and detail, Activity timeline, Summaries, Settings, add/scan flow) is the single release-gating item that is not itself an effort. The design language is the Graphite direction in [DESIGN.md](../../DESIGN.md). At minimum the Dashboard and Repos list must ship. The GUI's own quick actions (`repo_open_folder` / `terminal` / `editor` / `remote`) fold into the GUI work.

## 5. Non-goals for v0.9.0

These are deliberate exclusions, not gaps. The product anti-positioning (not a Git client for active development, not a CI or deployment tool, not an IDE workspace manager, not a process manager, not multi-user or team-shared) holds permanently. The following are cut to V1.1 or later:

- **Tray popover window.** The native right-click tray menu ships (E-13, tray native menu); the frameless left-click popover window is V1.1 (backlog `BL-V11-01`), because its anchored geometry is OS-specific and unverifiable on Windows-only hardware.
- **Keyring PAT.** v0.9.0 is unauthenticated GitHub access with aggressive caching. The optional Personal Access Token for higher rate limits is stubbed behind a seam in E-10 (GitHub metadata client) for V1.1.
- **Weekly summary.** The daily summary ships (E-11, summary engine); weekly aggregation is a V1.1 seam.
- **Saved filters and named views** over the repo list, and search across repos (name, path, description, remote URL, tag).
- **Custom per-repo command recipes** ("after update, run X").
- **Power-aware scheduling** (battery and lock awareness). v0.9.0 ships a fixed cadence plus a global pause.
- **macOS signed GA.** Signing and notarization are gated on Mac hardware and Apple credentials.

**Auto-update is no longer a non-goal.** It was previously cut to V1.1; the v0.9.0 ship plan moves it into scope as E-18 (auto-update and distribution). The updater and winget manifest are built and verified in this release. Only the winget submission and the updater endpoint activation are deferred to the public flip, because both require public artifact URLs.

## 6. Success criteria

v0.9.0 is ready to tag (privately) when all of the following hold, verified by hand and by the gate suite:

1. **The installer installs and runs on Windows.** A packaged build installs from its artifact and launches to a working app on a clean Windows machine.
2. **Scheduled checks fire.** The scheduler spawns at launch, due repos are fetched on their cadence, and a global cadence change reschedules already-scheduled repos.
3. **Notifications reach the OS.** A new release or a failure produces a real desktop toast, coalesced per cycle and suppressed during quiet hours.
4. **All gates are green.** `cargo clippy --workspace --all-targets -D warnings`, the full `cargo test --workspace` suite (completing within the tiered-test budget), `pnpm typecheck` / `lint` / `build`, and all PR #2 (build RepoSync V1) CI checks pass on both `windows-latest` and `macos-latest`.
5. **The dogfood walkthrough is clean.** A human runs the real app (`pnpm tauri dev` and a packaged build) through every flow (add, scan, check, update, group, open, audit, settings) with no defect that blocks the value loop, recorded in a dogfood report.

Per-capability acceptance criteria are defined in each effort's `spec.md`; the criteria above are the release-level bar and do not restate them.

## 7. Release posture

v0.9.0 ships **complete but private**. The full release ceremony happens on the private repo: merge PR #2 (build RepoSync V1), version bump to 0.9.0, `CHANGELOG` and README updates, an installer build with a smoke test, a final full gate sweep, and a private GitHub Release with the Windows installer artifacts attached and an annotated `v0.9.0` tag.

The **public flip** is a separate, later milestone. It carries the two deferred distribution steps: the winget submission (which requires public artifact URLs) and activation of the updater endpoint. Everything those steps depend on (the manifest, the updater client, the release artifacts) is built and verified in v0.9.0; the flip is the act of publishing, not new construction.

Unsigned-binary friction (Windows SmartScreen "unknown publisher") is documented in the release notes, not hidden, because code signing is deferred.

## 8. Constraints

- **Open source, community contribution.** MIT licensed. RepoSync is an OSS community contribution, not a commercial product, which sets the license, telemetry, and distribution defaults.
- **No telemetry.** No telemetry, no crash reporting, no account, no cloud sync. All state is local, in a SQLite database. This is a ratified OSS default, not a V1 shortcut.
- **Dual-platform, Windows-first.** True dual-platform on a maximally common architecture, per the ratified platform decision (2026-05-31). Windows is the first real GA target; macOS is kept compiling and bundling in CI and ships as an unsigned beta only if the week-4 descope trigger clears, otherwise it is deferred to a staged later release.
- **Local-first, resident model.** The app must be running for scheduled checks to happen; there is no OS-level scheduler in v0.9.0. This is documented behavior, stated plainly, not a bug.
- **`reposync-core` never imports `tauri`.** The typed IPC seam (E-06, IPC contract) keeps the core headlessly testable and makes the macOS port a thin edge.

## 9. Requirements traceability and change control

- **Requirements aggregate here; acceptance criteria live in specs.** This document states what v0.9.0 must do and which effort owns each capability. The testable acceptance criteria for each capability live only in that effort's `spec.md`, with citations back to the source brief. This document never restates or invents acceptance criteria.
- **Scope authority is the roadmap.** The tier assignments and the scope ledger are authoritative in [program-roadmap.md](program-roadmap.md). This PRD reflects that ledger; it does not redefine scope. A scope change is a ratified decision recorded in the roadmap's ratified-decisions table first, then reflected here (the E-16 groups promotion of 2026-06-30 and the E-17 / E-18 additions of 2026-07-04 are the current examples).
- **SHOULD efforts and descope triggers.** Each SHOULD effort carries a pre-committed descope trigger in the roadmap. If a trigger fires, the effort is cut to a fast-follow and this document is updated to move it into Section 5 (non-goals). MUST efforts do not have descope triggers; they gate the release.
- **Build status lives in the plan, not here.** Per-effort build and readiness status is tracked in [plan_v0.9.0.md](release-plans/plan_v0.9.0/plan_v0.9.0.md) and [feature-inventory.md](release-plans/plan_v0.9.0/feature-inventory.md). This PRD is stable across the build; it changes only when scope or requirements change.
