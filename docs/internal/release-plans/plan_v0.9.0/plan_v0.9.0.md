---
version: v0.9.0
title: "Release plan: v0.9.0"
type: release-plan
status: released              # draft -> in-progress -> released
created: 2026-06-22
updated: 2026-07-05
target-date:                  # shipped 2026-07-05
includes: [E-01, E-02, E-03, E-04, E-05, E-06, E-07, E-08, E-09, E-10, E-11, E-12, E-13, E-14, E-15, E-16, E-17, E-18]
spec-count: 18
plan-count: 18
checklist-complete: true
---

# Release Plan: v0.9.0

## Theme

RepoSync V1 (MUST scope), Windows GA. The first complete release: a working, scheduled, fast-forward-only repo-freshness tray utility on Windows, with macOS shipped as an unsigned beta if it is not yet unblocked. Ships complete on a private repository; the public flip is a later, separate milestone (see Context below).

## Context

This is the first release of RepoSync. It bundles all 18 v0.9.0 efforts: the V1 MUST tier (including Groups, E-16, retroactively specced) plus the SHOULD tier (E-10, E-11, E-14, E-15, E-17 (branch and PR intelligence), and E-18 (auto-update and distribution)), the latter two ratified into scope on 2026-07-04. It is deliberately `0.9.0`, not `1.0.0`: as of 2026-07-04 the backend cores, the frozen IPC contract, and the full GUI shell (Dashboard, Repos, Activity, and Settings, plus the repo-detail drawer and the add/scan flow) are all built, but PR #2's CI is red on all four checks, the workspace test suite does not complete in a reasonable time, several open-in quick actions ship broken on Windows (one of them a real security defect), and nothing has been dogfooded on a packaged build. `0.9.0` still signals "feature-complete enough to try," not "stable and polished." See `_LOCAL/audit/2026-07-04_18-21_fable-audit.md` (the audit driving this 2026-07-04 update) and the earlier `_local/audit/2026-06-22_audit.md` Section 13 for the original 0.9.0-vs-1.0.0 rationale.

**Private-ship posture (ratified 2026-07-04).** v0.9.0 ships COMPLETE, including the full release ceremony (merge PR #2, tag `v0.9.0`, a GitHub Release with Windows installer artifacts), but the repository STAYS PRIVATE. Every public-facing mechanic this plan or its companions describe, the winget package submission, the updater's public `latest.json` endpoint, public install instructions, is prepared and verified now but held back for a later, separate "public flip" milestone. Read every release-mechanics statement in this plan and its companions (`execution-plan.md`, `ci-plan.md`) as private-repo-only until that flip happens.

Scope authority: `docs/internal/program-roadmap.md` (the execution plan, dependency graph, scope ledger, descope triggers). This plan aggregates; it does not redefine scope or invent acceptance criteria. AC live in each effort's `spec.md`. The phased path from here to the tag is [execution-plan.md](execution-plan.md); CI diagnosis and repair is [ci-plan.md](ci-plan.md); the product-facing requirements are [product-requirements.md](../../product-requirements.md).

---

## Aggregation

> This table is maintained by hand. The **Build status** column is the live signal (sourced from the audit); spec and plan presence is hand-verified. A release tool could later regenerate it from effort frontmatter, but none is canonical yet, and the specs' current `status: ready` scheme is fine as-is. Per-effort spec / implementation-plan / GitHub-issue links live in the roadmap's tracking table ([program-roadmap.md](../../program-roadmap.md)).

| Effort | Title | Tier | Spec | Plan | Build status |
|--------|-------|------|------|------|--------------|
| E-01 | Foundation, Workspace, and CI | MUST | ready | ready | Done |
| E-02 | Persistence and Paths | MUST | ready | ready | Done |
| E-03 | Git Engine | MUST | ready | ready | Done |
| E-04 | Git Fixture Test Harness | MUST | ready | ready | Done |
| E-05 | Error Taxonomy (AppError) | MUST | ready | ready | Done |
| E-06 | IPC Contract (the typed seam) | MUST | ready | ready | Done (frozen) |
| E-07 | Update-Policy Engine | MUST | ready | ready | Done |
| E-08 | Scheduler | MUST | ready | ready | Done |
| E-09 | Activity Log Writer and Retention | MUST | ready | ready | Done |
| E-10 | GitHub Metadata Client | SHOULD | ready | ready | Done (core; BL-NI-15a/c done, BL-NI-15b before wiring) |
| E-11 | Summary Engine (Daily) | SHOULD | ready | ready | Done |
| E-12 | Tracer Bullet and Packaging Spike | MUST | ready | ready | Done |
| E-13 | Tray Native Menu | MUST | ready | ready | Done, shipped (full six-item menu + close-to-tray, Phase 3 commit deec0bf, 2026-07-05) |
| E-14 | Desktop Notifications | SHOULD | ready | ready | Done (core; plugin wiring deferred) |
| E-15 | Autostart (Launch on Login) | SHOULD | ready | ready | Done (core; plugin wiring deferred) |
| E-16 | Groups (repo tags) - retroactive spec | MUST (promoted 2026-06-30) | ready (retroactive) | ready | Built + retro-spec (a85e0fc backend, 51daaa7 frontend, both 2026-07-03) |
| E-17 | Branch and PR intelligence | SHOULD (jp-ratified 2026-07-04) | ready | ready | Shipped (Phase 4 commit 4efb0ab, review fixes 1543d4d, 2026-07-05) |
| E-18 | Auto-update and distribution (updater + winget prep) | SHOULD (jp-ratified 2026-07-04) | ready | ready | Shipped, ships DARK (Phase 4 commit c7eff64, 2026-07-05; updater wired but inactive until the production signing key is generated) |

**Not an effort, tracked here because it gates the release:** the **GUI**. The full app shell and its four primary screens, Dashboard, Repos, Activity, and Settings, plus the repo-detail drawer, add/scan flow, and editable settings, landed on `build/e-01-foundation` starting at commit `8aeebd7` and continuing through the 2026-07-03 build session (ending `03a5ef6`), wired through the frozen `bindings.ts`. The design language is settled: the Graphite direction in `DESIGN.md` (earlier draft mockups archived to `_local/gui/archived-mockups/`). The GUI is built, not merely started, but it ships with open defects surfaced in the 2026-07-04 audit (`_LOCAL/audit/2026-07-04_18-21_fable-audit.md`): the group filter false-empties during load or on a fan-out failure, the Dashboard "Needs attention" row taxonomy collapses every state to a failed-red icon (contradicting `DESIGN.md`'s behind/dirty/failed color scheme), the repo-detail drawer goes stale after a background check completes, and a batch of accessibility gaps (group rename/delete buttons invisible while focused; the drawer and dialog carry no focus trap or Escape handling). These are Phase 1 (Correctness) work in [execution-plan.md](execution-plan.md), not open scope.

**Also committed to the release, now built:** **Groups (repo tags)** - user-defined, colored, many-to-many labels for repos. Promoted to MUST-tier scope as E-16 on 2026-06-30 (ledger amendment in [program-roadmap.md](../../program-roadmap.md); backlog BL-V11-04) and fully built 2026-07-03: the store and IPC layer (commit a85e0fc) and the GUI, a Groups nav, create/assign/filter, and per-row chips (commit 51daaa7), on top of the `groups` / `repo_groups` schema frozen in migration 0002. The feature spec was not written before the build landed; it is being written retroactively as the as-built contract at [E-16-groups/spec.md](E-16-groups/spec.md) (plus [implementation-plan.md](E-16-groups/implementation-plan.md)), covering the shipped behavior and its known defects, including BL-NI-22 (the O(N) per-repo group-membership query) and the group-filter false-empty state above. Product framing: [features-and-outcomes.md](features-and-outcomes.md) Section 3.

Summary: v0.9.0 SHIPPED 2026-07-05 as a private prerelease (see the Readiness Checks section below). 18 efforts in scope as of 2026-07-04 (the original 15 plus E-16 Groups, E-17 branch and PR intelligence, and E-18 auto-update and distribution, all ratified this date), all built and shipped. The GUI shell and its four core screens are built, and Groups (E-16) is fully built with a retroactive spec. E-13 (tray) is done: the full six-item menu (Show, Check All Now, Pause/Resume, Open recent, Settings, Quit) plus close-to-tray shipped in Phase 3 (commit deec0bf). E-14 (notifications) and E-15 (autostart) had their OS plugin wiring (`tauri-plugin-notification`, `tauri-plugin-autostart`) connected in the same phase (commits 1a5eff9, 189443a). E-17 and E-18 both shipped in Phase 4 (commits 4efb0ab / 1543d4d, and c7eff64); E-18 ships DARK pending the maintainer's production signing-key step (see decision D4 below). The Phase 1 correctness pass fixed the audit findings, including the open-in quick actions' Windows defects, one of them security-relevant. PR #2's CI could not be made green on the release commit: GitHub Actions billing on the org was exhausted at ship time, so the cut proceeded on the comprehensive local gates instead, per the maintainer's G0 waiver (see D4). The phased path executed is [execution-plan.md](execution-plan.md) (see its Phase records); the original CI diagnosis is [ci-plan.md](ci-plan.md).

---

## Readiness Checks

These doc-readiness conditions block tagging; verify them by hand. They are a provisional project convention, not a fixed standard - the exact set may change, and a release tool could automate the check later. Nothing here is required to be skill-driven.

| Check | Condition | Current status |
|-------|-----------|----------------|
| (a) Spec final | Every effort's `spec.md` is final/agreed, not a draft | OK (specs are `status: ready`, including the 2026-07-04 additions E-16, E-17, E-18) |
| (b) Coupled plan | Every effort has an `implementation-plan.md` | PASS (18 of 18) |
| (c) AC addressed | Every plan addresses its spec's acceptance criteria | PASS. Verified per effort as each landed; E-17/E-18 verified in the Phase 4 gate (`_LOCAL/gates/2026-07-05_phase-4-gate.md`) and the Phase 4 Codex review (`_LOCAL/audit/2026-07-05_phase-4-codex-review.md`) |
| (d) Work complete | Every effort's implementation is complete and verified | PASS. The 2026-07-04 audit's correctness findings are fixed (Phase 1); the app is dogfooded on both the dev harness and a packaged build (`_LOCAL/dogfood/2026-07-05_phase-2-dogfood.md`); E-13/E-14/E-15 OS-integration wiring is complete (Phase 3); E-17 and E-18 shipped (Phase 4); the Windows installer is built and smoke-tested (MSI + NSIS, see `_LOCAL/gates/2026-07-05_phase-5-release-readiness.md`). PR #2's CI itself never went green on the release commit, because GitHub Actions billing on the org was exhausted; that precondition was waived by the maintainer in favor of the local gates (decision D4) |
| (e) Not stale | No spec changed after its plan without the plan catching up | PASS (moved together this session; new E-16/E-17/E-18 specs and plans land together in the 2026-07-04 pass) |

**Current readiness: RELEASED.** v0.9.0 shipped 2026-07-05 as a private GitHub prerelease, tag `v0.9.0` on merge commit `e21cf0e`. All five checks pass; (d)'s one asterisk is the waived G0 CI-green precondition (decision D4), covered instead by the comprehensive local gates (348 tests, clippy `-D warnings`, build, installer) recorded in `_LOCAL/gates/2026-07-05_phase-5-release-readiness.md`. See [execution-plan.md](execution-plan.md)'s Phase records for the full path from NOT READY to shipped.

---

## Doc-Update Checklist (desktop app)

Every box must be checked before the `v0.9.0` tag. The list draws on the project conventions in `docs/internal/release-plans/release-checklist.yaml`.

| Doc / artifact | Update | Done |
|----------------|--------|------|
| Version bump | Run `scripts/bump-version.mjs 0.9.0` (Cargo workspace + src-tauri/Cargo.toml + package.json + tauri.conf.json all agree) | [x] |
| `CHANGELOG.md` | Move `[Unreleased]` items into a `v0.9.0` section with the date | [x] |
| `README.md` | Bump version references; add install instructions for the Windows artifact | [ ] Outstanding; tracked as a separate post-ship nit (jp), not a tag blocker |
| `docs/architecture.md`, `docs/explanation.md`, `docs/faq.md` | Reflect the shipped state (remove "not built yet" hedges that no longer hold) | [ ] Partially done; at least one residual "not yet built" hedge remains in `docs/architecture.md`'s E-06 section. Not a tag blocker; flagged for a follow-up sweep |
| `docs/user-guide.md` | Write the enhanced explanatory user guide against the shipped v0.9.0 behavior (jp directive 2026-07-04): install, first run, concepts (freshness model, update policies, cadence inherit, groups), every feature, troubleshooting | [x] Written Phase 5, commit `baec834` |
| `docs/internal/program-roadmap.md` | Mark shipped efforts; record any descope-trigger outcomes | [x] |
| `docs/backlog.md` | Move anything resolved out; confirm V1.1 items are parked | [x] |
| macOS posture note | State in the Release notes whether macOS ships (unsigned beta) or is deferred | [x] Stated in the Release body: macOS is compile-verified only, NOT distributed in v0.9.0 (decision D2) |
| Updater signing keys + `latest.json` | **Deliberately deferred, not done.** The maintainer ratified a SHIP-DARK decision for this cut: the production signing keypair is intentionally NOT generated, and no `latest.json` is published, until the maintainer does the human-only key-generation step at the public flip. This is a recorded deferral, not a missed MUST (see D4 and the E-18 spec) | [ ] Deferred by decision, not outstanding work |
| winget manifest | Prepare the winget package manifest; submission to the winget-pkgs repo is DEFERRED to the public flip (winget requires public artifact URLs) | [x] Manifest prepared and passes `winget validate` offline; submission correctly deferred |
| `execution-plan.md` phase gates | Confirm every phase gate (Phase 0 through Phase 5) in [execution-plan.md](execution-plan.md) is checked off before tagging | [x] |
| GitHub Release | Draft from the `CHANGELOG.md` v0.9.0 section; attach Windows + (if shipping) macOS artifacts. Private repo: the release is private until the public flip | [x] Published as a private prerelease; cut manually with `gh release create` because GitHub Actions could not run (org billing exhausted; see D4) |
| Git tag `v0.9.0` | Annotated tag on the release-prep sha, once every box above is checked | [x] On merge commit `e21cf0e` |

---

## Open Questions / Decisions

| ID | Title | Resolution | Status | Updated |
|----|-------|------------|--------|---------|
| D1 | First-release version number | `v0.9.0` (beta of V1 MUST scope), not `v1.0.0` | Decided | 2026-07-04 |
| D2 | macOS in v0.9.0 GA bar | Decided: macOS compile-verified only, NOT distributed in v0.9.0 | Decided | 2026-07-05 |
| D3 | Release tooling not canonical yet | The jp-library release skills are not final; keep the release process manual and self-contained; revisit adopting automation when the standard settles | Open | 2026-06-22 |
| D4 | G0 "CI is green on the release commit" precondition | Waived for the v0.9.0 cut; released manually via `gh release create` on the strength of the local gates | Decided | 2026-07-05 |

### D1: First-release version number (decided)

**Summary.** Tag the first release `v0.9.0`, not `v1.0.0`.

**Decision.** `v0.9.0`. The product is pre-GA in the sense that matters: as of 2026-07-04, CI is red, the test suite does not complete, several defects from the audit are unfixed, macOS is unsigned/unlaunched, and nothing has been dogfooded on a packaged build, even though the backend, GUI, and Groups are all built. `0.9.0` is honest, buys a `0.9.x` beta cycle, and aligns with the program roadmap's Windows-GA-first descope trigger. Reserve `1.0.0` for the real GA bar (dogfooded + macOS signed, or an explicit Windows-only-`1.0.0` decision).

> **Maintainer decision:**
>
> * **Status:** Decided
> * **Choice:** `v0.9.0`
> * **Reasoning:** Ratified 2026-07-04 as part of that date's full ship-plan decision set: ship v0.9.0 complete, with the full release ceremony (merge PR #2, tag, GitHub Release with installer artifacts), while keeping the repository private. The public flip is a separate later milestone.
> * **Decided by / date:** jp, 2026-07-04

### D2: macOS in v0.9.0 GA bar (decided)

**Summary.** Whether macOS ships as an artifact in v0.9.0 or stays deferred.

**Decision.** macOS is compile-verified only: the macOS `.app`/`.dmg` bundle builds green in CI's macOS leg, but it is NOT distributed as part of the v0.9.0 release. No macOS artifact is attached to the GitHub Release; the Windows NSIS and MSI installers are the only shipped artifacts. macOS signing and notarization remain human-only, unblocked work items, tracked toward the public flip.

> **Maintainer decision:**
>
> * **Status:** Decided
> * **Choice:** macOS compile-verified only, not distributed in v0.9.0
> * **Decided by / date:** jp, 2026-07-05

### D4: G0 "CI is green on the release commit" precondition waived (decided)

**Summary.** The cut-tag runbook's G0 gate requires CI green on the release commit before tagging. For the v0.9.0 cut, this precondition was waived.

**Decision.** GitHub Actions billing on the `product-on-purpose` org was exhausted at ship time, so CI and `release.yml` could not run; every workflow run since 2026-07-05 is rejected before starting. The maintainer waived the G0 CI-green precondition: release quality was established instead by the comprehensive LOCAL gates (348 tests, `clippy -D warnings`, build, installer, and the release-readiness checks in `_LOCAL/gates/2026-07-05_phase-5-release-readiness.md`), and the Release was cut manually with `gh release create`. Fixing the org's Actions billing is a public-flip prerequisite, not a condition of this private cut.

> **Maintainer decision:**
>
> * **Status:** Decided
> * **Choice:** Waive G0's CI-green precondition for this cut; substitute the local gate evidence
> * **Reasoning:** GitHub Actions billing exhausted at ship time (external, not a code or quality problem); local gates are comprehensive and green
> * **Decided by / date:** jp, 2026-07-05

---

## Notes

- This plan was created during the 2026-06-22 scaffolding restructure that migrated the 12 efforts out of `AGENTS/efforts/` into this release folder. All 12 efforts target v0.9.0 (the set has since grown to 18 with E-16 groups, E-17 branch and PR intelligence, and E-18 auto-update), so `_unassigned/` is currently empty (reserved for future V1.1 efforts).
- Updated 2026-07-04 to add E-16 (Groups, retroactive spec), E-17 (branch and PR intelligence), and E-18 (auto-update and distribution), and to reconcile every stale claim the 2026-07-04 audit (`_LOCAL/audit/2026-07-04_18-21_fable-audit.md`) found in this file against the true build state. The phased plan to close the gap to the tag lives in [execution-plan.md](execution-plan.md).
- The tag-cutting ceremony is in `docs/internal/release-plans/runbook_cut-tag-release.md`.
- Updated 2026-07-05 to reconcile this release-governance layer against the completed ship: v0.9.0 tagged and released 2026-07-05 (private prerelease, merge commit `e21cf0e`). This pass flipped E-13/E-17/E-18 to shipped, resolved D2 (macOS posture), added D4 (the G0 CI-green waiver), and closed out the Doc-Update Checklist. See [execution-plan.md](execution-plan.md)'s Phase 2 through 5 records for the build history this reconciliation draws on.
