---
date: 2026-06-01T10:07:49-07:00
repo: product-on-purpose/repo-sync-tool
branch: main
summary: "Analyzed V1 roadmap, surfaced Windows-only blind spot, decided dual-platform Windows-first, shipped architecture+decisions brief"
files-changed:
  - docs/internal/v1-architecture-and-decisions.md
  - AGENTS/session-log/2026-06-01_10-07_claude_reposync-v1-architecture-decisions-brief.md
session-type: planning
parent-session: none
model: claude opus 4.8
model-settings: "ultracode (xhigh + dynamic workflow orchestration), explanatory output style"
agent: claude-code
status: completed
decisions-count: 16
duration-minutes: 150
commit-sha: c13dd8a
tags: [reposync, architecture, decisions, cross-platform, tauri, planning, workflow-orchestration]
transcript-path: C:\Users\jpris\.claude\projects\E--Projects-product-on-purpose-repo-sync-tool\5955ba3d-f7dc-4103-bed5-508f417263ff\
adrs-created: []
---

# Session: RepoSync V1 Architecture and Decisions Brief

## Summary

The session began as a request to analyze RepoSync's existing plans and roadmap and name the most important decisions needed to execute the cross-platform V1. A six-agent analysis workflow ranked the execution decisions and adversarially verified the ranking, surfacing the project's single biggest blind spot: the roadmap is written as if a two-platform team ships macOS and Windows simultaneously, while the real operator is one person on a Windows-only machine building through AI agents. jp made one firm decision (true dual-platform, but ship Windows first with maximally common architecture), asked for the autonomy boundary and scope line to be explained, and asked for a rich shareable artifact. A second four-agent workflow drafted the architecture, UI/UX, build-now plan, and decision explainers, which were assembled into a 990-line shareable brief at `docs/internal/v1-architecture-and-decisions.md`. No code was written; this was analysis, decision-capture, and documentation.

## Work Completed

- Read the full planning corpus: `docs/internal/strategy-and-roadmap.md` (1176 lines), `_LOCAL/repo_updater_functionality_breakdown_gpt-5.4.md` (593 lines), and the four UI mockups under `docs/internal/mockups/` plus `index.html`.
- Ran a six-agent analysis workflow (`reposync-v1-decision-analysis`): four parallel analysts (open decisions, already-made decisions, gaps, roadmap critique), a synthesizer that produced a 15-item ranked decision list, and an adversarial verifier that corrected the ranking and caught false gaps. ~573K subagent tokens.
- Delivered an in-chat ranked analysis separating decisions into three tiers: decide-before-code, binds-at-publish/GA, and Windows-ship gaps the plan never named.
- Captured jp's platform decision and his preference for collaboration artifacts to project memory.
- Read the four mockups in full to ground the UI/UX section in the real design system.
- Ran a four-agent drafting workflow (`reposync-v1-brief-sections`): architecture, UI/UX, build-now plan, and decision explainers. ~268K subagent tokens.
- Assembled the four sections plus an authored intro, decision ledger, table of contents, and next-steps into `docs/internal/v1-architecture-and-decisions.md` (990 lines) via a Python assembly script that normalized heading levels and enforced the no-em-dash rule deterministically.
- Validated the three least-conventional mermaid diagrams (two architecture flowcharts, the gantt) with the Mermaid MCP tool; all valid. The other three are simpler flowcharts.
- Wrote four memory files and updated the memory index.
- Delivered the brief to jp as a file and answered the autonomy/scope/why-it-matters questions inline.

## Decisions Made

Decisions are captured in the brief's decision ledger (Section 1). Status taxonomy: **firm** (jp decided), **recommended** (sound, awaiting jp ratification), **agent-default** (engineering call the agent will proceed on), **deferred**.

**Architectural / strategic:**

1. **Platform target (firm, jp).** True dual-platform is the end goal, but ship Windows first with maximally common architecture; macOS degrades to "compiles + bundles in CI" until real Mac access exists. Rationale: jp's only machine is Windows 11, so he cannot interactively run, sign, notarize, or QA a macOS build. Alternatives considered: Windows-only (throws away cross-platform goal); simultaneous dual GA (blocks the shippable Windows build behind a Mac he does not have). Windows-first with a staged macOS GA keeps macOS first-class and cheap to finish while quarantining the unverifiable work.

2. **V1 scope line (recommended, pending ratification).** Keep the core loop + unauthenticated GitHub enrichment + daily summary; cut the separate frameless tray popup window (keep the cheap native right-click menu), the keyring-backed GitHub PAT, weekly summary, grouping/tags, saved filters, custom command recipes, and auto-updater to V1.1. Rationale: ~13 deliverable groups in 6 weeks for one dev, and the heaviest items are the least testable on Windows. Recommended pre-committed descope triggers so slippage is a deliberate cut.

3. **Human/agent autonomy boundary (recommended, pending ratification).** A 7-item human-only allowlist (Apple enrollment, Windows cert, storing signing secrets, repo private-to-public flip, merge to default, cut release tag, force-push) gated on money/identity/irreversibility/publishing; agent autonomous everywhere else; tiered merges (self-merge while private, human-reviewed once public). A drop-in `EXECUTION.md` skeleton is in the brief.

4. **Code signing decoupled from GA (recommended, pending).** Ship the first public build unsigned with documented install steps; add Windows signing via Azure Trusted Signing as a fast-follow. Binds at ship, not at start.

**Agent-default engineering decisions (flagged for visibility, agent will proceed):**

5. sqlx posture: use the runtime query API (no compile-time `DATABASE_URL` / `.sqlx` offline-cache friction), or macros + committed cache + CI check. Runtime API recommended for the agent-driven CI model.
6. IPC types: `tauri-specta` to generate TypeScript from Rust so the contract cannot drift.
7. `scoped_bookmark_blob TEXT` column added to `repos` in the first migration (free MAS hedge, hard Phase-0 deadline since post-V1 schema is additive-only).
8. Per-repo async mutex so a scheduled check and a manual check cannot run two git ops in one working tree (index.lock corruption fix the roadmap missed).
9. WebView2 evergreen `downloadBootstrapper` to keep the bundle under 30MB.
10. App data in `%LOCALAPPDATA%`, never a OneDrive-synced folder, to avoid SQLite WAL corruption; defined migration-failure startup recovery.
11. git discovery order + minimum version floor (git >= 2.30); "git not found" is a first-class state on Windows.
12. `git2`/`libgit2-sys` pinned vendored + no-OpenSSL transport (network goes through the CLI), behind a `GitEngine` trait so all-CLI fallback is cheap.
13. OSS defaults ratified: no telemetry, no in-app crash reporting, DB-only settings, inherit system credential helper, ship only check_only/fetch_only/pull_ff_only, 6h cadence, resident-tray scheduler.

**Pending jp calls that bind later (deferred):**

14. Go-public timing + first-commit contents; quarantine `docs/internal/` and `_LOCAL/` out of public history (one-way door).
15. License: MIT default, Apache 2.0 defensible; binds at first public commit.
16. Brand name: keep "RepoSync" working title, isolate to one constant; decide 4-6 weeks before GA.

**Process decisions (not counted above):** used multi-agent workflows for both the analysis and the drafting (ultracode mode); authored the final artifact by hand to control voice, cross-references, and the no-dash rule; assembled programmatically because retyping ~100K characters of agent output would be error-prone.

## Files Changed

In-repo (working tree, untracked, not committed):

- `docs/internal/v1-architecture-and-decisions.md` (new, 990 lines) - the session's primary deliverable.
- `AGENTS/session-log/2026-06-01_10-07_claude_reposync-v1-architecture-decisions-brief.md` (this log).

Outside the repo (project memory at `C:\Users\jpris\.claude\projects\E--Projects-product-on-purpose-repo-sync-tool\memory\`):

- `decision_platform_strategy.md` (new) - the firm platform decision.
- `pref-artifact-capture.md` (new) - jp values durable collaboration artifacts.
- `v1_execution_brief.md` (new) - artifact location + the 6 pending decisions.
- `MEMORY.md` (updated) - added three index pointers.

Pre-existing at session start (not created this session): `docs/internal/strategy-and-roadmap.md`, `docs/internal/mockups/*`, `_LOCAL/*`.

Transient scratch (safe to delete): `E:\tmp\reposync-sections\*.md`, `E:\tmp\mmd\*.mmd`, `E:\tmp\assemble_brief.py`.

## Verification

- [x] Brief assembled and written to disk (990 lines, confirmed by the assembly script output and a heading-structure grep).
- [x] No-em-dash / no-en-dash rule enforced deterministically (script reports 0 remaining U+2014 and U+2013; the Edit/Write hook also passed).
- [x] Heading hierarchy verified clean (H1 > H2 sections 1-7 > H3 > H4) via grep; doubled-rule and in-fence demotion artifacts fixed and re-verified.
- [x] Mermaid: 3 of 6 diagrams validated via Mermaid MCP (the two architecture flowcharts with `\n` labels and the gantt with embedded colons) - all `valid: true`. The other three are simpler `flowchart LR` / `graph LR` blocks.
- [x] Content grounded in the source docs and the actual mockup markup (agents read the files; UI tokens reproduced verbatim).
- [ ] NOT verified: nothing was built or run. No Rust/TS code exists yet. The architecture is a design proposal, not an implementation.
- [ ] NOT verified: `tauri-specta` v2 is at release-candidate (`2.0.0-rc.21` per Context7); flagged to pin and re-check at scaffold time, not yet pinned or compiled.
- [ ] NOT verified: the 2-3 week Phase 0 estimate and all acceptance targets (memory, cold-start, bundle size) are unvalidated projections.
- [ ] NOT verified: the 6 pending decisions await jp's ratification; the brief records recommendations, not confirmed calls.

## Outstanding Issues

- **Six decisions await jp ratification:** autonomy boundary, V1 scope line, code-signing posture, go-public timing, license, brand. The first two block the start of focused build work; the rest bind at publish/GA.
- **`strategy-and-roadmap.md` is not yet reconciled.** It still asserts macOS Phase 0/1 acceptance bars that no human on the project can verify. The brief recommends adding a platform-access risk row and rewriting acceptance criteria to be per-platform. The two docs currently disagree.
- **Nothing is committed.** The entire `docs/` tree is untracked, as is the new `AGENTS/` dir. Per jp's standing rule, the agent does not commit or push without being asked.
- **No `EXECUTION.md` exists yet.** The skeleton is in the brief but has not been written to disk pending ratification of the autonomy model.
- **Repo is private with internal strategy material untracked.** When it goes public, `docs/internal/` and `_LOCAL/` must be quarantined before the first public commit, or strategy/PII lands in public git history.

## What's Next

1. jp ratifies the **autonomy boundary** (Section 3) and the **V1 scope line** (Section 3). These two unblock focused execution.
2. Write `EXECUTION.md` from the brief's skeleton once the autonomy model is confirmed.
3. Reconcile `docs/internal/strategy-and-roadmap.md`: add the platform-access risk row and rewrite Section 7 acceptance criteria to be per-platform, so the canonical plan stops asserting unverifiable macOS bars.
4. Scaffold Phase 0 + the tracer bullet (all UI-independent): Cargo workspace + `reposync-core`, SQLite schema + migrations (with `scoped_bookmark_blob`), the git engine, the fixture test harness, the typed IPC contract via `tauri-specta`, and CI on both runners. First slice: `repo_add_path` + `repo_check_now` end to end through real git into SQLite emitting a real event, in a real Windows build, behind a throwaway UI.
5. Optional: generate a printable HTML/PDF version of the brief for easier sharing (jp values shareable artifacts; consider the `jp-guide` skill).
6. Decide on quarantine + license + first public commit before going public at Phase 0 exit.

## Evidence Index

- Primary deliverable: `docs/internal/v1-architecture-and-decisions.md`.
- Analysis workflow output (6-agent, ranked decisions + adversarial verdict): `C:\Users\jpris\AppData\Local\Temp\claude\E--Projects-product-on-purpose-repo-sync-tool\5955ba3d-f7dc-4103-bed5-508f417263ff\tasks\wyua3mmcv.output`.
- Drafting workflow output (4-agent sections): `C:\Users\jpris\AppData\Local\Temp\claude\E--Projects-product-on-purpose-repo-sync-tool\5955ba3d-f7dc-4103-bed5-508f417263ff\tasks\w05jap2vv.output`.
- Workflow scripts: `...\5955ba3d-...\workflows\scripts\reposync-v1-decision-analysis-wf_0adedb9d-83f.js` and `...\reposync-v1-brief-sections-wf_0bfa2888-5b5.js`.
- Source plan: `docs/internal/strategy-and-roadmap.md`. UI source: `docs/internal/mockups/`.
- Mermaid validation results: under `...\5955ba3d-...\tool-results\mcp-claude_ai_Mermaid_Chart-*.txt` (all `valid: true`).

## Verification Detail

| Check | Method | Result | Notes |
|-------|--------|--------|-------|
| Brief written | Assembly script stdout | 990 lines, 107910 chars | Single file at the target path |
| No em/en dashes | Script count of U+2014/U+2013 + Write hook | 0 remaining | Hook also blocked an earlier script that contained literal dashes |
| Heading hierarchy | `grep -nE '^#{1,4} '` | Clean H1>H2>H3>H4 | Fixed doubled `---` and in-fence demotion |
| Mermaid diagram 1 (system overview) | Mermaid MCP validate | valid: true (flowchart) | `\n` labels render as line breaks |
| Mermaid diagram 2 (platform abstraction) | Mermaid MCP validate | valid: true (flowchart) | Dotted undirected link + subgraph class OK |
| Mermaid diagram 5 (gantt) | Mermaid MCP validate | valid: true (gantt) | Embedded colons in task names tolerated |
| Mermaid diagrams 0, 3, 4 | Not individually validated | Assumed valid | Simpler `graph LR` / `flowchart LR` |
| Code compiles/runs | Not attempted | N/A | No code exists; analysis + docs only |

## Continuation Prompt

```text
You are resuming work on RepoSync, a cross-platform (macOS + Windows) desktop tray
utility (Tauri v2 + Rust + React/TypeScript + shadcn/ui + SQLite) that keeps a personal
library of cloned-but-not-actively-developed Git repos fresh and visible. It is an
open-source community contribution (not commercial), built by one developer (jp) on a
Windows 11 machine through AI agents.

REPO: E:\Projects\product-on-purpose\repo-sync-tool  (branch: main, remote:
github.com/product-on-purpose/repo-sync-tool, currently PRIVATE, only commit c13dd8a).
The entire docs/ tree and the new AGENTS/ dir are UNTRACKED and uncommitted.

READ FIRST (in order):
1. docs/internal/v1-architecture-and-decisions.md  - the architecture + decisions brief
   produced last session. Start with Section 1 (the decision ledger) to see what is
   decided vs pending. This is the source of truth for execution.
2. docs/internal/strategy-and-roadmap.md           - the original plan it extends.
3. AGENTS/session-log/2026-06-01_10-07_claude_reposync-v1-architecture-decisions-brief.md
   - last session's log (this file's source).

STATE OF DECISIONS:
- FIRM (decided by jp): platform = true dual-platform but Windows-first with maximally
  common architecture; macOS degrades to "compiles + bundles in CI" until jp has real
  Mac access (a used Apple Silicon Mac mini or a cloud Mac). Keep all platform-specific
  code behind thin seams (paths module, tray.rs, CI bundling); reposync-core stays
  #[cfg]-free.
- PENDING jp ratification (do NOT assume these; ask jp to confirm before acting on them):
  (a) the human/agent autonomy boundary, (b) the V1 scope line (cut tray popup window +
  keyring GitHub PAT + weekly summary + groups + saved filters + recipes + auto-updater
  to V1.1, keep core + unauthenticated GitHub enrichment + daily summary).
- PENDING but binds later: code-signing posture, when/how to go public (quarantine
  docs/internal/ and _LOCAL/), license (MIT default), brand name (RepoSync working title).

IMMEDIATE NEXT ACTION: ask jp to confirm or amend (a) the autonomy boundary and (b) the
V1 scope line from Section 3 of the brief. These two gate focused build work. Do not
start scaffolding before they are settled.

THEN, in order, once ratified:
1. Write EXECUTION.md from the drop-in skeleton in Section 3 of the brief.
2. Reconcile docs/internal/strategy-and-roadmap.md: add a platform-access risk row to
   Section 8 and rewrite Section 7 acceptance criteria to be per-platform (Windows =
   launches + human-validated + signed-or-documented; macOS = compiles + bundles in CI).
3. Scaffold Phase 0 + the tracer bullet (all UI-independent; see Section 6 of the brief):
   Cargo workspace + crates/reposync-core (zero Tauri deps), SQLite schema + sqlx
   migrations INCLUDING the scoped_bookmark_blob TEXT column on repos in the first
   migration, the git engine (git/cli.rs shell-outs + git/inspect.rs git2 reads behind a
   GitEngine trait), the typed IPC contract via tauri-specta (verify its v2 RC version
   via Context7 and pin it), the fixture test harness (programmatic bare+working repo
   pairs in tempdirs for all 7 states), and CI on Windows + macOS runners. First vertical
   slice: repo_add_path + repo_check_now end to end through real git -> SQLite -> emitted
   event in a real Windows build, behind a throwaway debug UI.

AGENT-DEFAULT engineering choices already decided (proceed unless jp objects): sqlx
runtime query API (no compile-time DATABASE_URL/.sqlx cache), tauri-specta for IPC types,
per-repo async mutex around git ops, WebView2 evergreen downloadBootstrapper, app data in
%LOCALAPPDATA% (never OneDrive-synced), git discovery + min version 2.30, libgit2-sys
vendored + no-OpenSSL, no telemetry, inherit system git credential helper, only
check_only/fetch_only/pull_ff_only modes, 6h cadence, resident-tray scheduler.

HARD RULES:
- Never use em-dashes (U+2014) or en-dashes (U+2013) anywhere. Use " - " or restructure.
  (Enforced by a PreToolUse hook on Edit/Write.)
- Do NOT commit or push, flip the repo public, or merge to main without jp explicitly
  asking. Branch first. These are human-only actions per the autonomy boundary.
- reposync-core must never import tauri, even transitively.
- The session used ultracode (workflow orchestration) and explanatory output style; match
  jp's preference for capturing collaboration in durable shareable artifacts.

Useful context: last session ran two multi-agent workflows (analysis + drafting); their
outputs are saved under
C:\Users\jpris\AppData\Local\Temp\claude\E--Projects-...\5955ba3d-...\tasks\
(wyua3mmcv.output and w05jap2vv.output) if you need the raw analysis.
```
