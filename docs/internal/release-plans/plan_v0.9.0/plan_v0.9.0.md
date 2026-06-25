---
version: v0.9.0
title: "Release plan: v0.9.0"
type: release-plan
status: in-progress          # draft -> in-progress -> released
created: 2026-06-22
updated: 2026-06-22
target-date:                 # TBD
includes: [E-01, E-02, E-03, E-04, E-05, E-06, E-07, E-08, E-09, E-10, E-11, E-12, E-13, E-14, E-15]
spec-count: 15
plan-count: 15
checklist-complete: false
---

# Release Plan: v0.9.0

## Theme

RepoSync V1 (MUST scope), Windows GA. The first public release: a working, scheduled, fast-forward-only repo-freshness tray utility on Windows, with macOS shipped as an unsigned beta if it is not yet unblocked.

## Context

This is the first release of RepoSync. It bundles the entire V1 effort set (E-01 through E-12) plus the GUI. It is deliberately `0.9.0`, not `1.0.0`: the backend foundation and the frozen IPC contract are done, but the engines (E-04, E-07, E-08, E-09), the GitHub client (E-10), the summary engine (E-11), and the entire GUI are not built yet, macOS is unsigned and never launched, and nothing has been dogfooded. `0.9.0` signals "feature-complete enough to try" and buys a `0.9.x` beta to dogfood and unblock macOS before committing to `1.0.0`'s stability promise. See `_local/audit/2026-06-22_audit.md` Section 13 for the full rationale.

Scope authority: `docs/internal/program-roadmap.md` (the execution plan, dependency graph, scope ledger, descope triggers). This plan aggregates; it does not redefine scope or invent acceptance criteria. AC live in each effort's `spec.md`.

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
| E-09 | Activity Log Writer and Retention | MUST | ready | ready | Not started |
| E-10 | GitHub Metadata Client | SHOULD | ready | ready | Not started |
| E-11 | Summary Engine (Daily) | SHOULD | ready | ready | Not started |
| E-12 | Tracer Bullet and Packaging Spike | MUST | ready | ready | Done |
| E-13 | Tray Native Menu | MUST | ready | ready | Not started |
| E-14 | Desktop Notifications | SHOULD | ready | ready | Not started |
| E-15 | Autostart (Launch on Login) | SHOULD | ready | ready | Not started |

**Not an effort, tracked here because it gates the release:** the **GUI**. `src/App.tsx` is a throwaway debug surface; the four mockups in `docs/internal/mockups/` are Draft 1 design intent. At least the dashboard + repo list must ship for v0.9.0. (Build status: Not started.)

Summary: 15 efforts in scope. 9 done (E-01, E-02, E-03, E-04, E-05, E-06, E-07, E-08, E-12), 6 not started (E-09-E-11, E-13-E-15), plus the unbuilt GUI. E-13 to E-15 are the integration efforts added 2026-06-23 to close the category-C gap.

---

## Readiness Checks

These doc-readiness conditions block tagging; verify them by hand. They are a provisional project convention, not a fixed standard - the exact set may change, and a release tool could automate the check later. Nothing here is required to be skill-driven.

| Check | Condition | Current status |
|-------|-----------|----------------|
| (a) Spec final | Every effort's `spec.md` is final/agreed, not a draft | OK (specs are `status: ready`) |
| (b) Coupled plan | Every effort has an `implementation-plan.md` | PASS (12 of 12) |
| (c) AC addressed | Every plan addresses its spec's acceptance criteria | Verify per effort as it is built |
| (d) Work complete | Every effort's implementation is complete and verified | NOT YET (6 not started + GUI unbuilt) |
| (e) Not stale | No spec changed after its plan without the plan catching up | PASS (moved together this session) |

**Current readiness: NOT READY (expected, mid-build).** Check (d) is the real driver: most of the work is unbuilt. (a)/(b)/(e) are fine; (c) is confirmed per effort as each lands.

---

## Doc-Update Checklist (desktop app)

Every box must be checked before the `v0.9.0` tag. The list draws on the project conventions in `docs/internal/release-plans/release-checklist.yaml`.

| Doc / artifact | Update | Done |
|----------------|--------|------|
| Version bump | Run `scripts/bump-version.mjs 0.9.0` (Cargo workspace + src-tauri/Cargo.toml + package.json + tauri.conf.json all agree) | [ ] |
| `CHANGELOG.md` | Move `[Unreleased]` items into a `v0.9.0` section with the date | [ ] |
| `README.md` | Bump version references; add install instructions for the Windows artifact | [ ] |
| `docs/architecture.md`, `docs/explanation.md`, `docs/faq.md` | Reflect the shipped state (remove "not built yet" hedges that no longer hold) | [ ] |
| `docs/internal/program-roadmap.md` | Mark shipped efforts; record any descope-trigger outcomes | [ ] |
| `docs/backlog.md` | Move anything resolved out; confirm V1.1 items are parked | [ ] |
| macOS posture note | State in the Release notes whether macOS ships (unsigned beta) or is deferred | [ ] |
| GitHub Release | Draft from the `CHANGELOG.md` v0.9.0 section; attach Windows + (if shipping) macOS artifacts | [ ] |
| Git tag `v0.9.0` | Annotated tag on the release-prep sha, once every box above is checked | [ ] |

---

## Open Questions / Decisions

| ID | Title | Resolution | Status | Updated |
|----|-------|------------|--------|---------|
| D1 | First-release version number | `v0.9.0` (beta of V1 MUST scope), not `v1.0.0` | Recommended; pending jp ratification | 2026-06-22 |
| D2 | macOS in v0.9.0 GA bar | Ship Windows GA; macOS as unsigned beta or deferred per the week-4 descope trigger | Open | 2026-06-22 |
| D3 | Release tooling not canonical yet | The jp-library release skills are not final; keep the release process manual and self-contained; revisit adopting automation when the standard settles | Open | 2026-06-22 |

### D1: First-release version number (recommended, pending ratification)

**Summary.** Tag the first public build `v0.9.0`, not `v1.0.0`.

**Recommendation.** `v0.9.0`. The product is pre-GA (engines + GUI unbuilt, macOS unsigned/unlaunched, nothing dogfooded). `0.9.0` is honest, buys a `0.9.x` beta cycle, and aligns with the program roadmap's Windows-GA-first descope trigger. Reserve `1.0.0` for the real GA bar (dogfooded + macOS signed, or an explicit Windows-only-`1.0.0` decision).

> **Maintainer decision:** _(pending)_
>
> * **Status:** Open
> * **Choice:** (none)
> * **Reasoning:** (none)
> * **Decided by / date:** (none)

---

## Notes

- This plan was created during the 2026-06-22 scaffolding restructure that migrated the 12 efforts out of `AGENTS/efforts/` into this release folder. All 12 efforts target v0.9.0, so `_unassigned/` is currently empty (reserved for future V1.1 efforts).
- The tag-cutting ceremony is in `docs/internal/release-plans/runbook_cut-tag-release.md`.
