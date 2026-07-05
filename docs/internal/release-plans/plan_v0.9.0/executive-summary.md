# Executive summary: shipping RepoSync v0.9.0 (private)

- **Date:** 2026-07-04
- **Author:** Claude Fable 5 (orchestrator)
- **Status:** Ratified by jp; execution authorized ("full autonomy" after go, given 2026-07-04)
- **Plan of record:** [execution-plan.md](execution-plan.md) (v0.9.0 ship execution plan) - this document summarizes it

## The decision

Ship RepoSync v0.9.0 **complete and fully documented, but private**. The full release ceremony happens on the private repo: merge PR #2 (Build RepoSync V1), tag `v0.9.0`, publish a private GitHub Release with a Windows installer. The **public flip** (repo visibility, winget submission, live updater endpoint) is a separate later milestone with its own checklist in the release runbook.

Two features were promoted into scope on 2026-07-04: **E-17 (branch and PR intelligence)** and **E-18 (auto-update and distribution)**. Two other proposals (MCP server + CLI companion; Ctrl+K command palette) were considered and left for a later cycle.

## Where the product stood (Fable audit, 2026-07-04)

The first Fable audit of this repo (36 agents; full report: `_LOCAL/audit/2026-07-04_18-21_fable-audit.md`) verified that the 2026-07-03 build session's ledger was **honest and exact** (commits, branch stats, PR #2 claims all verify), but found the "all gates green" picture incomplete:

- **PR #2's CI is red on all four checks.** Root cause found: `cargo fmt --all -- --check` fails on unformatted 2026-07-03 code. Trivially fixable; the deeper CI drift is addressed in [ci-plan.md](ci-plan.md).
- **`cargo test --workspace` cannot finish in 10 minutes** (slow git-fixture tests; no failing test observed). Fixed by test tiering in Phase 0.
- **23 confirmed findings** (all survived adversarial refutation), including two HIGH: the open-in feature is broken on Windows for every repo (`\\?\` verbatim paths), and `repo_open_remote` executes unvalidated remote URLs (a security defect: any cloned repo controls that string).
- The release-plan docs lagged the built state by one full session (tray, groups, and the GUI were all described as unbuilt). Now reconciled.

## What ships in v0.9.0

18 efforts: E-01 through E-15 (built; E-13 tray partial, E-14/E-15 cores built with OS wiring pending), E-16 (groups, built 2026-07-03, retroactive spec written 2026-07-04), E-17 (branch and PR intelligence, new), E-18 (auto-update and distribution, new). Full scope and per-effort status: [plan_v0.9.0.md](plan_v0.9.0.md); product requirements: [product-requirements.md](../../product-requirements.md).

## The execution plan (phases 0-5)

Fable orchestrates; subagents execute by tier (Opus: scheduler/locking/security/CI and the E-17/E-18 cores; Sonnet: standard feature and frontend work plus docs; Haiku: gate runs and mechanical syncs). Anything compiling the Tauri shell crate serializes; core-only and frontend-only work parallelizes. Full detail: [execution-plan.md](execution-plan.md).

| Phase | What | Exit gate |
|---|---|---|
| 0 - Rails | Commit the doc set; `cargo fmt`; CI repair per ci-plan.md; test tiering so gates complete; tracking issues for E-16/E-17/E-18 | CI green on PR #2's head |
| 1 - Correctness | All audit code findings: opener hardening (BL-NI-24), cadence reschedule, settings_set probe failure, group filter false-empty, attention-row taxonomy, drawer staleness, a11y batch, BL-NI-21 regression tests, BL-NI-22 repos_in_group | Full gates + findings closed |
| 2 - Dogfood | Run the real app (dev + packaged), walk every flow per the dogfood protocol, file a report, fix fallout | Dogfood report clean |
| 3 - OS integration | E-14 notifications wiring, E-15 autostart wiring, E-13 tray completion (Check All Now, Pause/Resume, Open recent, Settings, close-to-tray), per-repo cadence write path + UI, missing event emissions, retention sweep on tick | Toggles real; tray meets spec |
| 4 - Features | E-17 (branch and PR intelligence) then E-18 (auto-update); migrations 0005 and 0006 respectively (0004 is Phase 1's default fix) | Specs' AC met |
| 5 - Ship (private) | Version bump, CHANGELOG, installer build + smoke test, final gates + whole-release Codex review, flip PR #2, merge, tag, private GitHub Release | Runbook complete |

Codex adversarial reviews run after each phase with substantive code; findings are fixed or backlogged with BL IDs.

## Review outcomes on this plan (before execution)

Three independent review layers ran on the doc set itself:

1. **Codex adversarial review**: needs-attention, 3 HIGH + 1 MEDIUM - all accepted and amended into the specs before execution: the migration `0004` collision (resolved: 0004 = Phase 1 default fix, 0005 = E-17, 0006 = E-18); E-17's unauthenticated GitHub budget (resolved: 30/hour request budgeter, 1 request/repo/pass, spread backfill, "as of" staleness rendering, new mock-server AC); the updater E2E transport that Tauri production builds would reject (resolved: test-only config overlay + release gate proving production config stays clean); the signing-key boundary contradiction (resolved: production signing is CI-secret-only; local E2E uses a disposable test key).
2. **Cross-doc consistency check** (Opus): 9 findings (4 medium), all fixed: stale backlog rows for groups and the auto-updater, a wrong audit citation in ci-plan.md, dependency-list drift between roadmap and specs, two docs both titled "Execution Plan", and two stale prose glosses in plan_v0.9.0.md.
3. **Mechanical validation** (Haiku): no em/en dashes, no broken links; one genuinely malformed table row (fixed); TBD tracking-issue placeholders resolve when Phase 0 creates the E-16/E-17/E-18 GitHub issues.

## Risks and levers

- **Test-suite runtime** is the biggest schedule risk for green CI; the tiering strategy in ci-plan.md is the mitigation, with cargo-nextest as the fallback.
- **E-17/E-18 can slip to v0.9.1** without blocking the tag (descope lever in execution-plan.md); the MUST tier does not depend on them.
- **Updater while private**: the live endpoint cannot be exercised until the public flip; the E2E proof runs against a local channel with a disposable key.
- **Dogfood unknowns**: the app has never been run interactively; Phase 2 exists precisely to absorb this risk early rather than at the ship gate.

## Human action items for jp

1. **Generate the production updater keypair** (Tauri signer), store the private key + password as GitHub Actions secrets, commit the public key. This is deliberately human-only. Fallback if not done by Phase 5: the updater ships dark (wired, disabled) and activates at the public flip.
2. Nothing else blocks: everything through the private release is delegated.

## Resourcing note

Planning-stage spend: roughly 4.0M subagent tokens across the audit workflow (36 agents), the doc-production workflow (12 agents), the consistency re-check, and the review-fix agents - with the orchestrator thread kept lean by structured-output reports and the context-pack pattern (`_LOCAL/plans/2026-07-04_18-25_ship-plan-context-pack.md`). Execution continues the same pattern: Fable plans and integrates; Opus/Sonnet/Haiku execute by tier.
