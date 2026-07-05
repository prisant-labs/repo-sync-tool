# Execution Plan: v0.9.0 ship (private)

- **Date:** 2026-07-04
- **Orchestrator:** Claude Fable 5 (full autonomy after jp says "go")
- **Status:** ready to execute (this doc is the operational script; it encodes the phase skeleton from the 2026-07-04 context pack in full detail)
- **Purpose:** The step-by-step build plan that takes the repo from its audited 2026-07-03 state to a tagged, privately-released **v0.9.0**. It is the EXECUTE layer for the [release plan](plan_v0.9.0.md) (the PLAN layer) and it consumes the [Fable audit](../../../../_LOCAL/audit/2026-07-04_18-21_fable-audit.md) (23 confirmed findings + notable lows), the [program roadmap](../../program-roadmap.md), the [edge-wiring plan](edge-wiring-plan.md), and the [backlog](../../../backlog.md).
- **Companions:** [ci-plan.md](ci-plan.md) (CI diagnosis + redesign, referenced by Phase 0), the per-effort specs for E-16 / E-17 / E-18, and the [cut-tag runbook](../runbook_cut-tag-release.md) (Phase 5 ceremony).

## What "ship v0.9.0 (private)" means here

Ship v0.9.0 COMPLETE with all documentation, but the repo STAYS PRIVATE. The full release ceremony runs on the private repo: merge PR #2 (build RepoSync V1), tag `v0.9.0`, cut a private GitHub Release with Windows installer artifacts. The public flip (BL-DEC-03, go-public timing) is a separate later milestone. Two constraints follow from privacy and appear wherever release mechanics are described:

- **Auto-update endpoints** are built and verified against a local or private fixture, not pointed at a public release URL. A private repo has no public artifact URL for the updater to poll.
- **Winget submission** is deferred to the public flip (winget requires a public artifact URL). E-18 (auto-update and distribution) builds and verifies the manifest and updater infrastructure now; it does not submit.

## Ratified decisions this plan executes

| Decision | Direction |
|---|---|
| End state | v0.9.0 complete + documented; repo stays private; private release ceremony (merge PR #2, tag, private GitHub Release with Windows artifacts). |
| New features in scope | E-17 (branch and PR intelligence) and E-18 (auto-update and distribution). Winget submission deferred to the public flip; updater infra built and verified now. |
| Authority | Full autonomy after jp says "go": build, fix, dogfood, commit, push, merge PR #2, tag, release (private). |
| Descope | E-17 and E-18 are SHOULD; either can slip to v0.9.1 without blocking the tag (see Descope levers). |

---

## 1. Model tiers and how work is assigned

| Tier | Role | Gets |
|---|---|---|
| **Fable** | Orchestration, integration review, crux decisions, dogfood driving, the release ceremony. | Cross-workstream sequencing, merge/tag calls, disposition of review findings, descope calls. |
| **Opus** | The load-bearing correctness work. | Scheduler / locking / migrations, opener / security, CI redesign, test-suite tiering, E-17 and E-18 cores, command-layer integration tests. |
| **Sonnet** | Standard feature and surface work. | Frontend fixes and a11y, per-repo cadence drawer UI, E-17 frontend, docs prose, additive IPC command wiring. |
| **Haiku** | Mechanical, verifiable-by-rule work. | Full-suite gate sweeps, mechanical doc syncs (Task Summaries, feature-inventory rows, CHANGELOG lines), link checks, stale-comment cleanups. |

Model tier is named per workstream in each phase table. When a workstream spans tiers (a core built by Opus, a frontend consumed by Sonnet), the table names the tier that owns the compile-risky part; the lighter part is a follow-on sub-task in the same workstream.

## 2. Serialization model (the rule that orders every phase)

One rule governs parallelism throughout: **anything that compiles the `reposync` shell crate (`src-tauri`) is a strict serial chain; `reposync-core`-only work and frontend-only (`src/`) work parallelize freely.**

- **Chokepoint = the shell crate.** `src-tauri` is a single compilation unit with shared managed state, one `lib.rs` setup path, one command registry, and generated `bindings.ts`. Two agents editing it at once collide on the same files and on the same `cargo build`/bindings regen. So every workstream that edits `src-tauri` (or regenerates bindings, or adds a Tauri plugin, or adds/changes a command) takes the chokepoint **one at a time, in the order the phase DAG gives**.
- **`reposync-core` is Tauri-free and headlessly testable.** Multiple core-only workstreams (pure scheduler logic, path-resolution helpers, E-17 core, summary changes) run in parallel because they touch disjoint modules and never share the shell's build.
- **Frontend-only (`src/**`, no bindings change) runs in parallel** with everything, because it only consumes `bindings.ts` and never compiles Rust. The one coupling: a frontend workstream that needs a NEW command or a newly-emitted event must wait for the shell-crate workstream that adds it to land (and regen bindings) first.

Each phase below gives its serialization DAG explicitly: the shell-crate chain (serial), the core-only set (parallel), the frontend-only set (parallel), and the cross-edges where a frontend task waits on a command/event from the chain.

## 3. Gates protocol

Two gate levels: scoped gates run continuously during work; a full-suite sweep runs once per phase, executed by a Haiku gate-runner.

### 3.1 Scoped gates (every change, run by the owning agent)

Run the narrowest gate that covers what changed, so iteration stays fast:

- **Core-only change** (`crates/reposync-core`):
  - `cargo clippy -p reposync-core --all-targets -D warnings`
  - `cargo clippy -p reposync-core --features test-support -D warnings`
  - `cargo test -p reposync-core` (fast tier; see 3.3)
- **Shell-crate change** (`src-tauri`):
  - `cargo clippy -p reposync --all-targets -D warnings`
  - `cargo test -p reposync` (the `src-tauri` library unit tests run on Windows; only genuinely launch-only behavior is out of reach and falls to dogfood)
- **Frontend-only change** (`src/**`):
  - `pnpm typecheck`
  - `pnpm lint`
- **Any change that touches or regenerates the IPC surface**: also run the bindings regen and confirm `bindings.ts` is in sync (no uncommitted drift), then `pnpm typecheck`.

### 3.2 Full-suite sweep (once per phase, Haiku gate-runner, exact commands in order)

The gate-runner runs this whole list and reports pass/fail per command. Any red stops the phase from closing.

1. `cargo fmt --all --check`
2. `cargo clippy --workspace --all-targets -D warnings`
3. `cargo clippy -p reposync-core --features test-support -D warnings`
4. `cargo test --workspace` (fast tier; see 3.3)
5. `cargo test --workspace -- --ignored` (slow git-fixture tier; see 3.3 - runs in the phase sweep and in CI, wall-clock budgeted)
6. `cargo tree -p reposync-core` and assert the tree contains **no** `tauri`, `tauri-*`, `chrono`, or `openssl` (the core dependency-hygiene invariant)
7. `pnpm typecheck`
8. `pnpm lint`
9. `pnpm build`

The gate-runner captures the command list, exit codes, and wall-clock per command into the phase's gate record under `_LOCAL/` so the phase DoD can cite green evidence.

### 3.3 The test-suite tiering (a Phase 0 deliverable that the gates depend on)

Today `cargo test --workspace` does not complete in 10 minutes (the git-fixture tests spawn many git subprocesses and dominate; the audit measured 105/276 core unit tests in ~15 min single-threaded, twice, with no failing test observed). Phase 0 splits the suite into a **fast tier** (pure unit tests, target full local fast-tier gate under ~10 min) and a **slow tier** (the git-fixture / subprocess-heavy tests, marked `#[ignore]` or behind a `slow-tests` feature, invoked explicitly). The exact split mechanism is Phase 0 P0-C work and is detailed with the CI redesign in ci-plan.md. Until Phase 0 lands the tiering, command 4 above is the whole suite and command 5 is a no-op; after Phase 0, command 4 is fast-tier and command 5 is the slow tier.

## 4. Codex adversarial review cadence

After each phase that lands substantive code (Phases 1, 3, 4, and a whole-release pass in Phase 5), run a Codex adversarial review before the phase can close. Phase 0 (rails) and Phase 2 (dogfood) do not get a standalone review: Phase 0 is CI/test-infra + doc commits (its correctness is the gate sweep and a green CI run), and Phase 2's fixes fold into the review of whichever phase's surface they touch (a substantial batch of Phase 2 fixes gets a mini-review).

- **How it runs:** the `codex:adversarial-review` challenge-review command, invoked directly via the codex-companion runtime (it is disable-model-invocation, so Fable runs it explicitly; it supersedes `codex:codex-rescue` for per-effort reviews). Scope the review to the phase's diff.
- **Disposition of each finding (Fable decides):**
  - **Fix now** - the finding is in-scope for the phase and either HIGH severity or cheap to fix correctly. It is fixed before the phase DoD, re-gated, and noted in the phase's doc updates.
  - **Backlog with a BL ID** - the finding is real but out of the phase's scope or a deliberate V1.1 deferral. File a new sequential backlog entry (BL-NI-24 onward; the roadmap-sweep pass owns ID assignment, so coordinate the next free number against `docs/backlog.md`), cite the review, and give it an owning effort or "hardening pass".
- **Hard rule (mirrors runbook gate G1):** no unaddressed HIGH-severity finding for in-scope work may remain open at a phase close, and none may remain at the tag. A HIGH is fix-now or an explicitly-waived documented decision, never a silent backlog.

## 5. Doc-update-in-change protocol

Living docs are updated **inside the change that makes them true**, before the phase gate, not in a batch at the end. Each phase updates the docs its work touched. The mechanical syncs are Haiku work; the judgment edits (readiness prose, framing) are Sonnet/Fable.

| Doc | Updated when | By |
|---|---|---|
| Effort spec **Task Summary** blocks (E-13 tray, E-14 notifications, E-15 autostart, E-16 groups, E-17 branch-intel, E-18 auto-update) | The phase that changes that effort's true state. | Haiku (mechanical state), Sonnet (nuance) |
| `feature-inventory.md` rows | Any phase that changes a feature's shipped/partial/planned state. | Haiku |
| `docs/backlog.md` | Every phase: mark resolved items done, append newly-surfaced items with sequential BL IDs. | Haiku (marks), Sonnet (new-entry prose) |
| `CHANGELOG.md` `[Unreleased]` | Every phase that lands user-visible change: add the line under Added/Changed/Fixed. | Haiku |
| `docs/internal/program-roadmap.md` build-status rows | Any phase that moves an effort's status (E-13/E-16/E-17/E-18 especially). | Haiku |
| `edge-wiring-plan.md` inventory + smoke-test checklist | Phases 1 and 3 (command wiring, plugin chrome, tray) and any IPC amendment. | Sonnet |

A phase gate does not pass until the docs its work touched are current. This is checked as part of the phase DoD, not left to Phase 5's doc sweep (Phase 5 verifies, it does not backfill).

## 6. IPC contract-amendment protocol (additive-only)

The IPC contract (E-06, the typed seam) is frozen. Several work items below need NEW commands or events. The freeze permits **additive** amendments only, handled uniformly:

- **Additive means:** a new command, a new event, or a new optional field on a payload. It must not change the shape or semantics of an existing command/event/field (that would break the frozen contract and any built frontend against it).
- **Process:** add the Rust type in `reposync-core::ipc` (or the command in `src-tauri`), regenerate `bindings.ts`, confirm no drift, run `pnpm typecheck`, and record the amendment in `edge-wiring-plan.md`'s command inventory and the E-06 (IPC contract) spec Task Summary. Each amendment lands on the shell-crate chain (it regenerates bindings).
- **New commands/events this release introduces (all additive):** a db-recovery-notice read command (E-02 AC7); `repos_in_group(group_id) -> Vec<i64>` (BL-NI-22); a per-repo `check_frequency_min` write path (finding 15, either extending `repo_set_policy` additively or a new `repo_set_cadence`); a tray "Check All Now" backend command (E-13); the E-17 branch/PR commands; the E-18 updater commands. Each is called out in its phase.

---

## Phase 0 - Rails

**Goal:** put the working surface on solid ground before any correctness work: land the reconciled doc set, make CI green, make the test suite completable, and capture the backlog. Nothing in Phase 0 changes product behavior; it changes the ground the rest of the build stands on.

### 0.1 Workstreams

| WS | Delivers | Tier | Files / crates | Compiles shell crate? | Upstream deps |
|---|---|---|---|---|---|
| **P0-A** Doc-set commit | Commit the reconciled output of the 2026-07-04 doc pass (PRD, execution-plan, ci-plan, E-16/E-17/E-18 specs+plans, plan sweep, roadmap/backlog/CHANGELOG sweep, AGENTS.md/CLAUDE.md, runbook/checklist). Fixes doc findings 3, 16-23. | Fable (integration) + Haiku (link checks) | `docs/internal/**`, root `AGENTS.md`, `CLAUDE.md`, `CHANGELOG.md`, `docs/backlog.md` | No | This doc pass complete |
| **P0-B** CI repair | Diagnose and fix PR #2's four red checks (build macos-latest + windows-latest); ci.yml predates GUI/tray/groups. Green on both runners for the current tree. | Opus (CI redesign) | `.github/workflows/ci.yml` (+ `release.yml` sanity), possibly `src-tauri` build config | Compiled in CI (not local edit) | ci-plan.md diagnosis |
| **P0-C** Test-suite tiering | Split fast unit tier vs slow git-fixture tier; fast-tier full local gate target under ~10 min; slow tier explicitly invoked (command 5). | Opus | `crates/reposync-core` test attrs / `Cargo.toml` features, CI wiring | No (core + test harness) | none |
| **P0-D** Backlog capture | Ensure every audit code finding not owned by a spec has a durable backlog entry (BL-NI-24 onward), citing the audit file. | Haiku (marks) + Sonnet (prose) | `docs/backlog.md` | No | Audit file |

> P0-A and P0-D operate on docs authored during the current doc pass (the roadmap-sweep agent writes the BL-NI-24+ entries; the plan-sweep and roadmap-sweep agents write the reconciled plan/roadmap/CHANGELOG). Phase 0 commits and verifies them; it does not re-author them. If Phase 0's own CI/test work surfaces new items, they are appended with the next sequential BL IDs.

### 0.2 Serialization DAG

- **Shell-crate chain:** none locally (P0-B/P0-C compile the shell only in CI, and edit yml / test config / core, not `src-tauri` source). Phase 0 has effectively no chokepoint contention, so all four workstreams run in parallel.
- **Parallel:** P0-A (docs), P0-B (CI), P0-C (tests), P0-D (backlog) all independent.
- **Cross-edge:** P0-B (CI) consumes P0-C's tiering decision for how CI runs tests, so P0-C's tier boundary is decided first (a one-message handshake), then both proceed.

### 0.3 Findings dispositioned in Phase 0

- **Finding 3** (release plan claims groups not started + the promised groups spec was never written): the E-16 (groups) spec is authored in this doc pass and committed here; the plan_v0.9.0 reconciliation lands in P0-A. The as-built groups DEFECTS (findings 7, 12, and the group-dialog / swatch lows) are Phase 1, not here.
- **Findings 16-20, 23** (plan self-contradiction, E-13 Task Summary "not started", program-roadmap stale rows, feature-inventory stale across five rows, edge-wiring "stub; no core", CHANGELOG claiming six built things "not built yet"): all fixed by their owners in the doc pass and committed in P0-A.
- **Findings 21, 22** (session log misattributes notifications to E-13; session log understates the tray gap): the session log is an immutable historical `_LOCAL/` artifact; it is not edited. The true tray gap is captured going forward in the E-13 (tray native menu) Task Summary fix (P0-A) and Phase 3's tray completion; notifications are correctly attributed to E-14 (desktop notifications) throughout the new docs. Disposition: closed as superseded, no code or doc-edit action.
- **CI red on all four checks** and **test suite cannot complete in 10 min**: the two non-finding facts from the audit verdict, owned by P0-B and P0-C respectively.

### 0.4 Definition of done (Phase 0)

- The full doc set is committed on `build/e-01-foundation`; link check passes; every doc finding (3, 16-23) is reflected as fixed.
- PR #2 CI is green on both runners (Windows build+bundle, macOS build+bundle, all gates) for the current tree.
- `cargo test --workspace` (fast tier) completes under ~10 min locally; the slow tier runs green when invoked explicitly.
- Every audit code finding has either a Phase 1/3/4 work item in this plan or a backlog entry (no orphans).
- Full-suite sweep (Section 3.2) green. Doc-update protocol satisfied.

---

## Phase 1 - Correctness (audit findings)

**Goal:** fix the confirmed code defects. This is the heaviest phase and the one the chokepoint rule matters most for: the opener, the scheduler wiring, and the groups/frontend fixes all converge on `src-tauri`.

### 1.1 Workstreams

| WS | Delivers | Tier | Files / crates | Compiles shell crate? | Upstream deps |
|---|---|---|---|---|---|
| **P1-A** Opener hardening | Strip the `\\?\` extended-length prefix so open-in works on Windows (finding 1); validate the remote URL scheme and reject/never-execute crafted remotes, handle SSH remotes honestly (finding 2, security); stop `cmd /C` metacharacter injection in the editor path (finding 8); surface real editor-launch failure instead of always toasting success (finding 9); detect full paths to `wt.exe` (low). Path/URL resolution helpers built test-first in core; the OS shell-out stays in `src-tauri`. | Opus (opener/security) | `src-tauri/src/opener.rs`; new path/URL helpers in `crates/reposync-core` (test-first) | **Yes (chokepoint)** | none |
| **P1-B** Scheduler + persistence core logic | Recompute `next_check_at` on settings save so a lowered global cadence takes effect on already-scheduled repos (finding 4, pure logic); the retention-sweep-on-tick hook (low); any pure reschedule/effective-frequency logic the shell will call. Built test-first in core. | Opus (scheduler) | `crates/reposync-core/src/scheduler.rs`, `activity` retention | No (core-only, parallel) | none |
| **P1-C** Shell-crate wiring | Wire P1-B into `src-tauri`: call reschedule on `settings_set`; spawn the scheduler when git arrives late so an absent-at-startup git recovers without restart (finding 6 / BL-NI-23 scope, the primary recovery scenario); stop `settings_set` from silently swapping a working git engine to None and toasting success (finding 5); serialize `settings_set` persist/probe/swap so overlapping saves are not racy (low); attach the activity retention sweep to the scheduler tick, not just startup (low); add the missing event emissions so `repo:state-changed` (and check-started / error:raised) actually fire for the frontend that subscribes to them (low); add the db-recovery-notice read command so E-02 AC7's `db_recovered`/`db_backup_path` reach the UI (low, additive IPC); migrate the 0001 schema default so it does not contradict the inherit model / create silent 6h overrides (low, additive migration `0004`, e.g. `0004_default_cadence_inherit.sql`); remove the stale module doc in `src-tauri/src/windows/mod.rs` (low). | Opus | `src-tauri/src/lib.rs`, `src-tauri/src/commands/mod.rs`, `src-tauri/src/events.rs`, `src-tauri/src/windows/mod.rs`, new migration `0004` | **Yes (chokepoint)** | P1-B (core reschedule logic), P1-A (chokepoint order) |

> **Migration sequence (ratified 2026-07-04, coordinated after the Codex adversarial review caught a collision):** additive migration `0004` is the P1-C default-cadence fix (`0004_default_cadence_inherit.sql`); `0005` is E-17's (branch and PR intelligence) schema addition; `0006` is E-18's (auto-update and distribution) schema addition.
| **P1-D** Frontend correctness + a11y | Group filter no longer false-empties during fan-out load or forever on fan-out failure (finding 7); "Needs attention" rows render the correct status taxonomy (behind = violet arrow-down, dirty = amber triangle, failed = red x-circle) instead of all failed-red (finding 10); the repo detail drawer refetches on background check completion by subscribing to the now-firing events (finding 11, waits on P1-C); a11y batch - focus-visible group rename/delete buttons (finding 12), focus trap + Escape + conditional aria-modal on Drawer/Dialog (finding 13), distinct accessible names on group color swatches (low), a visible attention-row focus indicator (low); fix the group dialog double-Enter double-submit and the wrong "invalid setting: name" remediation on a duplicate name (low). | Sonnet (frontend) | `src/screens/repos.tsx`, `src/screens/dashboard.tsx`, `src/components/repo-detail.tsx`, `src/components/groups-nav.tsx`, `src/components/ui/drawer.tsx`, `src/components/ui/dialog.tsx`, group dialog component | No (frontend-only, parallel) | finding 11 sub-task waits on P1-C event emission |
| **P1-E** repos_in_group query (BL-NI-22) | Replace the O(N)-per-repo group-membership fan-out with a single `repos_in_group(group_id) -> Vec<i64>` store fn + additive IPC command; regen bindings; swap the frontend hook to O(1). | Sonnet | `crates/reposync-core` store fn, `src-tauri` command, `bindings.ts`, `src/hooks/queries.ts` | **Yes (chokepoint, bindings regen)** | P1-C (chokepoint order); frontend swap follows command |
| **P1-F** Regression tests (BL-NI-21) | The three open regression tests: manual-vs-scheduled same-repo contention serializes on the shared `RepoLocks` (command-layer integration test needing a `State`/`AppHandle` harness); a quiet-hours UI round-trip and a non-UTC quiet-window gate. The first needs a `src-tauri` integration harness; the two frontend round-trips need a frontend test runner that does not exist yet (see descope note). | Opus | `src-tauri` integration tests; core tests; (frontend test runner is a gap) | **Yes (chokepoint, integration test)** | P1-A, P1-C landed |

### 1.2 Serialization DAG (Phase 1)

```
shell-crate chain (strict serial):   P1-A  ->  P1-C  ->  P1-E  ->  P1-F
core-only (parallel):                P1-B  (feeds P1-C; land before P1-C wires it)
frontend-only (parallel):            P1-D  (finding-11 sub-task waits on P1-C's event emission)
```

- P1-B (core) runs in parallel with P1-A on the chain, and its reschedule logic must be green before P1-C wires it - so P1-B lands into P1-C's start.
- P1-D (frontend) runs fully in parallel except its drawer-staleness sub-task (finding 11), which cannot be verified until P1-C makes `repo:state-changed` fire; sequence that sub-task last within P1-D.
- P1-E and P1-F both compile the shell crate, so they queue behind P1-C on the chain (P1-E before P1-F; P1-F wants P1-A + P1-C already landed to test against).

### 1.3 Findings dispositioned in Phase 1

Findings 1, 2, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13 (all HIGH and MEDIUM-code except 3, 14, 15) plus the lows: never-emitted events, retention-sweep-only-at-startup, db-recovery notice, 0001 default, settings_set race, group-dialog double-submit, wt path detection, color swatch names, attention-row focus indicator, stale module doc, and BL-NI-22. Findings 14 and 15 are Phase 3 (they need plugin wiring / a new cadence write path, which are OS-integration work); Phase 1 leaves an explicit note in the settings surface that the launch-on-login and notification toggles are wired in Phase 3.

### 1.4 Definition of done (Phase 1)

- Every Phase-1-assigned finding (see 1.3) is fixed and covered by a test where a test is possible; new core logic is test-first.
- Open-in works on Windows for a real repo (verified again in Phase 2 dogfood); the remote opener never executes a non-http(s) crafted remote.
- A lowered global cadence takes effect on already-scheduled repos without restart; a git-absent-at-startup recovery works without restart.
- The subscribed-to events actually fire; the drawer refreshes on background completion.
- Full-suite sweep green (including the slow tier). Codex adversarial review run; HIGH findings fixed-now, others backlogged with IDs. Doc-update protocol satisfied (feature-inventory, backlog, CHANGELOG, E-16 Task Summary, edge-wiring inventory for the new commands).

---

## Phase 2 - Dogfood

**Goal:** run the real app the way a user would, on both the dev harness and a packaged build, walk every flow, and fix what falls out. This is the only place launch-only behavior (tray, real windows, OS shell-out, packaged-install paths) is confirmed; the audit's central lesson is that "all static gates pass" did not mean "the app works" (open-in shipped broken on the primary platform).

### 2.1 Dogfood protocol

Run twice - once under `pnpm tauri dev` (fast iteration, real runtime) and once against a **packaged build** (installer-produced, the real distribution path). The packaged run is non-negotiable: it is the only run that exercises install paths, resource bundling, and the absence of the dev server.

**Launch A - dev harness:** `pnpm tauri dev`.
**Launch B - packaged build:** produce the installer via the E-12 packaging path (`pnpm tauri build`), install it, launch the installed app.

**Flow checklist (walked on both launches; failures logged with repro):**

- [ ] App launches; tray icon appears; the window shows.
- [ ] First-run / onboarding: fresh profile (no DB) opens cleanly; DB is created; no crash on empty state.
- [ ] Add a repo by path; scan a parent folder for repos; both persist and appear in the list.
- [ ] Manual "check now" updates the row and the detail drawer; a real change is reflected; a failure surfaces the correct error, not a false success.
- [ ] Open-in: folder, terminal, editor, and remote each open the right target on Windows (finding 1/2/8/9 regression, the marquee Phase 1 fix).
- [ ] Groups: create a group, assign repos, filter by group (no false-empty during load, finding 7), per-row chips render, rename/delete reachable by keyboard (finding 12).
- [ ] Settings: change the global cadence and observe it take effect (finding 4); a bad git path does not silently disable git and falsely toast success (finding 5).
- [ ] Cadence: a scheduled cycle actually fires on the configured interval; manual + scheduled do not collide on one repo.
- [ ] "Needs attention" taxonomy renders correctly per state (finding 10).
- [ ] Notifications (once wired in Phase 3): a real change fires one coalesced toast; quiet hours suppresses it. **In Phase 2 this box is expected to be unchecked** (wiring lands in Phase 3); note it, do not fail Phase 2 on it.
- [ ] Retention: nothing crashes over a long-resident run; the sweep is observable (finding: retention-sweep-on-tick).

**Evidence capture:** every walk produces a dated dogfood report under `_LOCAL/` (screenshots via the snagit-images or scratchpad path, the flow checklist with pass/fail, and repro notes for each failure). The report is the artifact Phase 2 closes against.

### 2.2 Workstreams

| WS | Delivers | Tier | Files / crates | Compiles shell crate? | Upstream deps |
|---|---|---|---|---|---|
| **P2-A** Dev-harness walk | Launch A, full flow checklist, logged failures. | Fable (drives) | none (observation) | Runs it | Phase 1 done |
| **P2-B** Packaged-build walk | Launch B, installer + full flow checklist, logged failures. | Fable + Opus (packaging) | E-12 packaging path | Builds it | Phase 1 done |
| **P2-C** Dogfood-fallout fixes | Triage the reports; fix defects; each fix respects the chokepoint rule (shell-crate fixes serialize; core/frontend parallel) and is tiered by area. | tiered per fix | wherever the defect is | per fix | P2-A/P2-B reports |

### 2.3 Serialization + review

- The two walks (P2-A, P2-B) can run back to back or in parallel by one driver; the fixes (P2-C) obey the same shell-crate-chain rule as Phase 1.
- A substantial batch of P2-C fixes gets a Codex mini-review; small fixes fold into the Phase 3 review of whatever surface they touched.

### 2.4 Definition of done (Phase 2)

- Both launches walked; a dogfood report per launch is saved under `_LOCAL/`.
- Every flow checklist item passes except the ones explicitly deferred to Phase 3 (notifications wiring, autostart-on-reboot, tray menu completeness), which are noted as expected-open.
- Every defect found is fixed or filed with a BL ID and an owning phase.
- Full-suite sweep green after the fixes. Doc-update protocol satisfied (CHANGELOG, backlog).

---

## Phase 3 - OS integration completion

**Goal:** close the edge-wiring remainder - the plugin chrome and native menu the cores were built behind. Every workstream here compiles the shell crate and/or adds a Tauri plugin, so Phase 3 is almost entirely a serial chain. This is where findings 14 and 15 land.

### 3.1 Workstreams

| WS | Delivers | Tier | Files / crates | Compiles shell crate? | Upstream deps |
|---|---|---|---|---|---|
| **P3-A** E-14 notifications wiring | Add `tauri-plugin-notification`; call the E-14 (desktop notifications) `decide`/`coalesce` core at the scheduler's check-completion; raise the OS toast; emit `notification:fired` (E-14 AC5). Source `LocalMinute` from the scheduler's offset-aware clock. Disposition BL-NI-17 (auth toast shares the failure toggle) - keep V1 behavior, note it. Fixes the notification half of finding 14. | Opus | `src-tauri` setup + scheduler check-completion path, plugin dep | **Yes (chokepoint, plugin)** | Phase 1 scheduler wiring (P1-C) |
| **P3-B** E-15 autostart wiring | Add `tauri-plugin-autostart`; reconcile against OS state on startup keyed off the `autostart` setting; start minimized on the autostart launch arg. Disposition BL-NI-18 (setting-wins vs adopt-OS-change) - ship the authoritative-setting policy, note it. Fixes the launch-on-login half of finding 14. | Opus | `src-tauri` setup, plugin dep | **Yes (chokepoint, plugin)** | P3-A (chokepoint order) |
| **P3-C** E-13 tray completion | Complete the tray native menu (E-13, tray native menu): the "Check All Now" backend command (additive IPC) + menu item, Pause/Resume, Open recent, a Settings item, and close-to-tray. The frameless popover window stays V1.1 (BL-V11-01). Moves E-13 from 2-of-6 to done for V1 scope; fixes findings 17 and 22's true gap. | Opus (chrome) + Sonnet (menu wiring) | `src-tauri/src/tray.rs`, setup, a new command | **Yes (chokepoint)** | P3-B (chokepoint order); Pause/Resume needs the scheduler control surface (Phase 1/P3) |
| **P3-D** Per-repo cadence write path + drawer UI | The IPC write path for `check_frequency_min` (finding 15: none exists today; `repo_set_policy` carries only mode/dirty-handling) - either an additive field or a new `repo_set_cadence` command - plus the drawer UI to set it. Consider landing the BL-NI-23 scheduler-loop live re-probe follow-up here since the loop is already being touched. | Sonnet (command + UI), Opus (scheduler touch) | `src-tauri` command, `bindings.ts`, `src/components/repo-detail.tsx` | **Yes (chokepoint)** for the command; UI part is frontend | P3-C (chokepoint order) |

### 3.2 Serialization DAG (Phase 3)

```
shell-crate chain (strict serial):  P3-A  ->  P3-B  ->  P3-C  ->  P3-D
frontend-only (parallel):           P3-D drawer UI (waits on P3-D command landing)
```

Everything of substance here is on the chain (plugins + native menu + a command). The only parallel slice is the per-repo cadence drawer UI, which waits for its command (P3-D) to land and regen bindings, then proceeds frontend-only.

### 3.3 Findings dispositioned in Phase 3

- **Finding 14** (launch-on-login + notification toggles have zero runtime effect): both halves wired - notifications in P3-A, autostart in P3-B.
- **Finding 15** (per-repo cadence has no IPC write path at all): P3-D.
- **Findings 17, 22** (E-13 cannot be called done; the tray gap is deeper than the log said): closed by P3-C completing the menu and close-to-tray.
- Backlog couplings resolved by wiring: BL-NI-17 (notification auth toggle) and BL-NI-18 (autostart reconcile policy) are dispositioned (V1 behavior kept, noted). BL-NI-16 (daily release-event fidelity) and BL-NI-15b (release ETag / separate release cadence) stay deferred - they are not required for the release and are noted where the refresh cadence is described.

### 3.4 Definition of done (Phase 3)

- Notifications fire (coalesced, quiet-hours-aware) on a real change; the `notification:fired` event emits.
- Autostart registers, survives a reboot, and starts minimized on the reboot launch (verified in a Phase-2-style smoke run appended to the dogfood evidence).
- The tray menu offers Show / Check All Now / Pause-Resume / Open recent / Settings / Quit and close-to-tray works; E-13 is done for V1 scope (popover explicitly V1.1).
- Per-repo cadence is settable from the drawer and takes effect.
- The edge-wiring smoke-test checklist is filled in and saved. Full-suite sweep green. Codex adversarial review run and dispositioned. Doc-update protocol satisfied (E-13/E-14/E-15 Task Summaries -> done/wired, feature-inventory, edge-wiring inventory, CHANGELOG).

---

## Phase 4 - New features

**Goal:** the two ratified SHOULD features. Both have specs authored in this doc pass (E-17, E-18). Both are descope-able to v0.9.1 without blocking the tag (see Descope levers). Cores are Opus; frontend and manifest work is Sonnet.

### 4.1 Workstreams

| WS | Delivers | Tier | Files / crates | Compiles shell crate? | Upstream deps |
|---|---|---|---|---|---|
| **P4-A** E-17 branch and PR intelligence | Per the E-17 (branch and PR intelligence) spec: the core detection/aggregation logic (Tauri-free, test-first), the additive IPC commands to expose it, and the frontend surface. | Opus (core) + Sonnet (frontend) | `crates/reposync-core` (new module), `src-tauri` commands, `bindings.ts`, `src/**` | **Yes (chokepoint)** for the commands; core + frontend parallel around it | E-17 spec; Phase 3 done |
| **P4-B** E-18 auto-update and distribution | Per the E-18 (auto-update and distribution) spec: `tauri-plugin-updater` wiring, the update-check surface, and the winget manifest. **Built and verified now; submission and public endpoints deferred to the public flip** (private repo has no public artifact URL). Verify the updater against a private/local fixture; produce the winget manifest but do not submit. | Opus | `src-tauri` (plugin + updater config), `release.yml`, winget manifest file | **Yes (chokepoint, plugin)** | E-18 spec; P4-A (chokepoint order); BL-DEC-01 (signing) posture noted |

### 4.2 Serialization DAG (Phase 4)

```
shell-crate chain (strict serial):  P4-A commands  ->  P4-B updater
core-only (parallel):               E-17 core detection logic (feeds P4-A commands)
frontend-only (parallel):           E-17 surface (waits on P4-A commands landing)
```

E-17's core builds in parallel; its commands take the chokepoint; its frontend follows the commands. E-18 takes the chokepoint after E-17's commands (it adds a plugin).

### 4.3 Definition of done (Phase 4)

- E-17: branch/PR intelligence is computed, exposed over additive IPC, and rendered; core is test-first; meets its spec AC.
- E-18: the updater is wired and verified against a private/local fixture (not a public endpoint); the winget manifest exists and is validated but not submitted; the private-release constraint is documented at every endpoint/submission touchpoint.
- Full-suite sweep green. Codex adversarial review run and dispositioned. Doc-update protocol satisfied (E-17/E-18 Task Summaries, feature-inventory, program-roadmap rows, CHANGELOG, backlog for winget-submission-deferred).
- **If descoped:** the phase's DoD is instead "E-17 and/or E-18 cleanly parked to v0.9.1 - spec/plan status set to deferred, no half-wired code on the release branch, backlog and roadmap reflect the slip." (See Descope levers.)

---

## Phase 5 - Ship (private)

**Goal:** cut and publish the private v0.9.0 release. This phase is the [cut-tag runbook](../runbook_cut-tag-release.md) (gates G0-G4) executed for real, with the private-release framing applied.

### 5.1 Workstreams

| WS | Delivers | Tier | Files / crates | Compiles shell crate? | Upstream deps |
|---|---|---|---|---|---|
| **P5-A** Version bump + doc sweep | `node scripts/bump-version.mjs 0.9.0` (all four version sources agree: root `Cargo.toml`, `src-tauri/Cargo.toml`, `package.json`, `src-tauri/tauri.conf.json`); finalize `CHANGELOG.md` `[Unreleased]` -> `## [0.9.0] - <date>`; bump README and install instructions; sweep `docs/architecture.md`/`explanation.md`/`faq.md` to remove "not built yet" hedges that no longer hold; mark shipped efforts in the roadmap; park V1.1 backlog items. | Sonnet + Haiku | version files, `CHANGELOG.md`, `README.md`, `docs/**` | Rebuilds after bump | Phases 1-4 done (or descoped) |
| **P5-B** Installer build + smoke | Build the Windows installer with the `dist` profile (full LTO) and smoke-test the installed artifact one last time. macOS bundle green in CI (unsigned; posture stated). | Opus (packaging) | `release.yml`, dist profile | Builds it | P5-A |
| **P5-C** Final gates + whole-release review | The full-suite sweep on the release-prep sha; a whole-release Codex adversarial review; confirm no open HIGH for in-scope work. | Haiku (gate) + Fable (review disposition) | none (verification) | No | P5-A, P5-B |
| **P5-D** Cut + private release | Flip PR #2 (build RepoSync V1) from draft to ready; merge; annotated tag `v0.9.0` on the captured release sha; `release.yml` fires and drafts the GitHub Release with Windows (+ unsigned macOS) artifacts; edit the draft (CHANGELOG body, macOS posture, **private-release note**); publish the **private** Release. Set the release plan frontmatter `status: released`. | Fable | git tag, GitHub Release (private) | tag build | P5-C green |
| **P5-E** Close-out | Closing session log; open a fresh `[Unreleased]`; confirm updater endpoints and winget submission are flagged as public-flip work, not done. | Sonnet | `CHANGELOG.md`, session log | No | P5-D |

### 5.2 Serialization + private-release constraints

- P5-A -> P5-B -> P5-C -> P5-D -> P5-E is strictly serial (each gate consumes the prior). No parallelism; this is a ceremony.
- **Private constraints applied here:** the merge, tag, and Release are on the private repo. The Release is a private GitHub Release (not public). The updater is not pointed at a public endpoint and winget is not submitted; both are recorded as public-flip (BL-DEC-03, go-public timing) work. macOS is unsigned (BL-DEC-01 / human-only signing) and the posture is stated in the Release notes rather than blocking the Windows cut.

### 5.3 Definition of done (Phase 5)

- All runbook gates G0-G4 pass (or any waiver is a documented decision in the plan, never a silent skip).
- All four version sources read 0.9.0; CHANGELOG has a dated 0.9.0 section; README and public-facing docs reflect the shipped state.
- PR #2 merged; annotated `v0.9.0` tag on the release-prep sha; private GitHub Release published with Windows artifacts (and unsigned macOS if green), macOS posture and private-release constraints stated.
- Release plan `status: released`; closing session log written; fresh `[Unreleased]` opened.

---

## 7. Audit-finding traceability (every finding mapped)

Every numbered finding (1-23) and every notable low is mapped to a phase, a workstream, and a disposition. Nothing is orphaned.

### 7.1 HIGH

| # | Finding (handle) | Phase / WS | Disposition |
|---|---|---|---|
| 1 | Open-in broken on Windows (`\\?\` extended-length path) | Phase 1 / P1-A | Fix now (strip prefix; core helper test-first) |
| 2 | `repo_open_remote` executes unvalidated remote URLs (security) | Phase 1 / P1-A | Fix now (scheme validation; never execute non-http(s)) |
| 3 | Release plan claims groups "not started" + groups spec never written | Phase 0 / P0-A | Fixed in doc pass (E-16 spec authored; plan reconciled). Code defects mapped separately (7, 12, lows). |

### 7.2 MEDIUM - code

| # | Finding (handle) | Phase / WS | Disposition |
|---|---|---|---|
| 4 | Global cadence change does not reschedule | Phase 1 / P1-B + P1-C | Fix now (recompute `next_check_at` on settings save) |
| 5 | `settings_set` silently swaps a working git engine to None | Phase 1 / P1-C | Fix now (guard; do not toast false success) |
| 6 | BL-NI-23 understates the gap: no scheduler spawned if git absent at startup | Phase 1 / P1-C | Fix now (spawn when git arrives late; live command-path recovery) |
| 7 | Group filter false-empties during load / failure | Phase 1 / P1-D | Fix now (treat missing membership map as unknown, not non-member) |
| 8 | `cmd /C` metacharacter injection in `open_editor` | Phase 1 / P1-A | Fix now |
| 9 | Misconfigured editor always reports success on Windows | Phase 1 / P1-A | Fix now (surface real failure) |
| 10 | "Needs attention" rows all render failed-red | Phase 1 / P1-D | Fix now (correct status taxonomy per DESIGN.md) |
| 11 | Detail drawer goes stale on background completion | Phase 1 / P1-D (waits on P1-C events) | Fix now (subscribe to the now-firing events) |
| 12 | Group rename/delete focusable but invisible while focused | Phase 1 / P1-D | Fix now (focus-visible) |
| 13 | Drawer/Dialog always-mounted `role=dialog`, no focus trap/Escape | Phase 1 / P1-D | Fix now (conditional aria-modal + focus trap + Escape) |
| 14 | Launch-on-login + notification toggles have zero runtime effect | Phase 3 / P3-A (notif) + P3-B (autostart) | Fix in Phase 3 (plugin wiring); Phase 1 leaves a note |
| 15 | Per-repo cadence has no IPC write path at all | Phase 3 / P3-D | Fix in Phase 3 (additive write command + drawer UI) |

### 7.3 MEDIUM - docs and log accuracy (fixed in the doc pass, committed Phase 0)

| # | Finding (handle) | Owner in doc pass | Phase |
|---|---|---|---|
| 16 | plan_v0.9.0 self-contradicts (E-13 deferred vs GUI-unbuilt vs real GUI) | plan-sweep agent | Phase 0 / P0-A |
| 17 | E-13 (tray) spec Task Summary says "not started" | plan-sweep (Task Summary) + Phase 3 completes E-13 | Phase 0 / P0-A + Phase 3 / P3-C |
| 18 | program-roadmap stale rows (E-13, groups amendment) | roadmap-sweep agent | Phase 0 / P0-A |
| 19 | feature-inventory stale across five rows | plan-sweep agent | Phase 0 / P0-A |
| 20 | edge-wiring-plan "stub; no core" / missing DONE markers | plan-sweep agent | Phase 0 / P0-A |
| 21 | Session log misattributes notifications wiring to E-13 | historical `_LOCAL/` artifact | Closed as superseded (new docs attribute to E-14) |
| 22 | Session log understates the tray gap | historical `_LOCAL/` artifact | Closed as superseded (E-13 Task Summary + Phase 3 carry truth) |
| 23 | CHANGELOG `[Unreleased]` claims six built things "not built yet" | roadmap-sweep agent (owns CHANGELOG) | Phase 0 / P0-A |

### 7.4 Notable lows (each placed)

| Low (handle) | Phase / WS | Disposition |
|---|---|---|
| Declared-but-never-emitted events (`repo:state-changed`, `repo:check-started`, `error:raised`); frontend subscribes to `repoStateChanged` which never fires | Phase 1 / P1-C | Fix now (add emit sites); unblocks finding 11 |
| Activity retention sweep runs only at startup (never attached to tick) | Phase 1 / P1-B + P1-C | Fix now (attach sweep to scheduler tick) |
| E-02 AC7 db-recovery notice cannot reach the UI (no command reads `db_recovered`/`db_backup_path`) | Phase 1 / P1-C | Fix now (additive read command) |
| 0001 schema default (360) contradicts the inherit model (future INSERTs create silent 6h overrides) | Phase 1 / P1-C | Fix now (additive migration 0004; align default with inherit) |
| `settings_set` persist/probe/swap not serialized (racy under overlapping saves) | Phase 1 / P1-C | Fix now (serialize the swap) |
| Double-Enter in the group dialog double-submits; duplicate-name error surfaces wrong remediation | Phase 1 / P1-D | Fix now |
| `wt` detection misses full paths to `wt.exe` | Phase 1 / P1-A | Fix now (opener) |
| Group color swatches share one accessible name | Phase 1 / P1-D | Fix now (distinct names) |
| Attention-row focus indicator effectively invisible | Phase 1 / P1-D | Fix now (visible focus ring) |
| Stale module doc in `src-tauri/src/windows/mod.rs` | Phase 1 / P1-C | Fix now (mechanical cleanup) |
| BL-NI-22 (O(N) repos-in-group filter) | Phase 1 / P1-E | Fix now (additive `repos_in_group` command) |

### 7.5 The two non-finding facts

| Fact | Phase / WS | Disposition |
|---|---|---|
| PR #2 CI red on all four checks (ci.yml predates GUI/tray/groups) | Phase 0 / P0-B | Diagnose + redesign (ci-plan.md); green both runners |
| `cargo test --workspace` does not complete in 10 min | Phase 0 / P0-C | Tier fast vs slow; fast-tier gate under ~10 min |

---

## 8. Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| **CI unknowns** - ci.yml predates GUI/tray/groups; the four red checks may hide more than one root cause (missing plugin features, bundler config, runner drift, deprecated actions per BL-NI-08). | High | Blocks Phase 0 exit and the tag (G0 needs green CI). | Phase 0 P0-B is Opus with a written diagnosis in ci-plan.md; fix incrementally, one red check at a time; land the Node-24 action bumps (BL-NI-08) if they are part of the failure; do not proceed past Phase 0 until both runners are green. |
| **Test-suite runtime** - git-fixture tests spawn many subprocesses and dominate; even tiered, the slow tier may blow the phase-sweep wall-clock budget. | High | Slows every phase gate; risks skipped testing under time pressure. | Phase 0 P0-C tiers the suite; fast tier is the per-change gate; the slow tier runs once per phase and in CI with an explicit wall-clock budget; if the slow tier is unbounded, cap concurrency / shard it in CI rather than skipping it. |
| **Tauri plugin surprises** - `tauri-plugin-notification`, `-autostart`, `-updater` each add a dep, a capability, and OS behavior that is not unit-testable and can fail only at launch/packaged-run. | Medium | Phase 3 / Phase 4 slip; a plugin that works in dev but not packaged. | Each plugin lands as its own thin chokepoint slice with an immediate dogfood smoke line (dev AND packaged); the edge-wiring smoke checklist is the gate; keep capabilities minimal and explicit; verify in the packaged build, never dev-only. |
| **Private-repo updater constraint** - the updater cannot poll a public artifact URL while private; a naive wiring points at a dead endpoint. | Medium | E-18 "works" in a way that cannot be truly exercised until the public flip. | E-18 verifies against a private/local fixture and documents the constraint at every endpoint touchpoint; the winget manifest is built and validated but not submitted; both are recorded as public-flip work, not release blockers. |
| **Contract-freeze amendments** - six new commands/events this release (db-recovery notice, `repos_in_group`, per-repo cadence, Check All Now, E-17 commands, E-18 updater) each regen `bindings.ts` and touch the frozen seam. | Medium | Bindings drift; a frontend built against stale types; two amendments colliding on the chokepoint. | The additive-only protocol (Section 6): additive changes only, regen + typecheck + no-drift check per amendment, record in the E-06 spec + edge-wiring inventory, and take the chokepoint one amendment at a time per the serialization rule. |
| **macOS signing not unblocked** (BL-DEC-01 / BL-DEC-02, human-only). | Medium | macOS ships unsigned or is dropped from the release bar. | Ship Windows GA; macOS as unsigned artifact if the CI bundle is green, else state deferred in the Release notes (per the roadmap's week-4 descope trigger); does not block the Windows cut. |
| **Dogfood surfaces a deep defect** late (Phase 2), forcing rework of a Phase 1 fix. | Medium | Phase 2 -> Phase 1 loopback; schedule slip. | Phase 2 fixes obey the same chokepoint discipline; a large fallout batch gets its own mini-review; the dogfood evidence is captured so the root cause is not re-litigated. |

---

## 9. Descope levers

Pre-committed levers to protect the tag. Under full autonomy after "go", **Fable decides** whether to pull a lever; **jp is notified** of the decision, not asked to ratify it (that is what full autonomy means here). Descope is a lever, not a default: pull it only when a feature is at real risk of blocking the tag.

| Lever | Trigger | Effect | Who |
|---|---|---|---|
| **Slip E-17 (branch and PR intelligence) to v0.9.1** | E-17 is not green and dogfood-clean by the time Phases 0-3 and 5-readiness are otherwise done, or its review surfaces a HIGH that cannot be fixed cheaply. | E-17 spec/plan set to `deferred` (target v0.9.1); no half-wired code on the release branch; roadmap + backlog reflect the slip; the tag proceeds without it. | Fable decides; jp notified. |
| **Slip E-18 (auto-update and distribution) to v0.9.1** | E-18 updater/manifest is not verifiable against the private fixture in time, or the plugin fights the packaged build. | E-18 spec/plan set to `deferred` (target v0.9.1); the release ships without in-app update (the private release is installed manually anyway); roadmap + backlog reflect the slip. | Fable decides; jp notified. |
| **macOS to deferred** (existing roadmap trigger) | macOS signing not unblocked / CI bundle not green by the cut. | Ship Windows-only; macOS stated as deferred in the Release notes. | Fable decides; jp notified. |

Both E-17 and E-18 are SHOULD-tier and additive; neither is a dependency of any MUST-tier surface, so slipping either leaves a coherent, shippable v0.9.0. The correctness phases (0-2) and the OS-integration completion (Phase 3) are **not** descope-able - they are the release's actual quality bar and the reason it is v0.9.0 and not a broken 1.0.

---

## 10. Definition of done (whole release)

The release is done when: every audit finding is dispositioned (fixed, doc-fixed, or backlogged with an owning effort - Section 7 shows none orphaned); Phases 0-3 are complete and dogfood-clean; Phase 4 is complete or cleanly descoped; the full-suite sweep and a whole-release Codex adversarial review are green with no open HIGH for in-scope work; all four version sources read 0.9.0 with a dated CHANGELOG section; PR #2 (build RepoSync V1) is merged; the annotated `v0.9.0` tag sits on the release-prep sha; a private GitHub Release is published with the Windows installer (and unsigned macOS if green); the macOS posture and the private-release constraints (updater not on a public endpoint, winget not submitted, both flagged as public-flip work) are stated; and the closing session log is written. The public flip (BL-DEC-03, go-public timing) remains a separate later milestone.
