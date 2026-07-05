# Release Plans

The version-scoped home for RepoSync's release governance. Each release is a self-contained folder that aggregates every effort (spec + implementation plan) in its scope, plus the readiness checks and doc-update checklist that gate the tag.

The folder structure here is deliberately self-contained: a release is a hand-maintained Markdown plan plus the effort folders in its scope. It adapts the `pm-skills` release pattern for a desktop app (not an agent plugin); see `_local/repo-scaffolding.md` for the rationale.

> **Convention status: provisional.** The jp-library release skills (`jp-release-plan` and friends) are NOT canonical yet, and the `agent-plugins` standard still defers the release-subsystem and issue conventions. So nothing in this repo depends on those skills. Any skill-command references below are illustrative of *optional future automation* - the process works by hand today, and the exact field names / gate vocabulary may change when the standard settles.

## Layout

```
release-plans/
├─ README.md                      this file
├─ runbook_cut-tag-release.md     the 6-gate tag-cutting ceremony (G0-G4)
├─ release-checklist.yaml         project-specific doc-update checklist rows
├─ _unassigned/                   efforts not yet committed to a release (staging)
│  └─ E-NN-slug/{spec.md, implementation-plan.md}
└─ plan_vX.Y.Z/                   one folder per release
   ├─ plan_vX.Y.Z.md              the release plan (theme, aggregation, gates, checklist, decisions)
   └─ E-NN-slug/{spec.md, implementation-plan.md}   per-effort, promoted into the release
```

## How it works (manual process; tool-agnostic)

- **Author** a spec + implementation plan per effort in `_unassigned/E-NN-slug/`.
- **Promote** an effort into a release by moving its folder `_unassigned/E-NN-slug/` -> `plan_vX.Y.Z/E-NN-slug/`. The move is the explicit act of committing the effort to ship in that release.
- **Aggregate** by keeping the plan's aggregation table current with the effort folders in scope (their status and presence).
- **Check readiness** against the doc-readiness gates + the doc-update checklist in the plan.
- **Cut** the tag via `runbook_cut-tag-release.md` once readiness is met.

If/when the jp-library release skills become canonical, they can automate the promote/aggregate/gate steps. They are not required, and this repo does not depend on them.

## Two checklists, two jobs

1. The **readiness checks** in each `plan_vX.Y.Z.md` answer "are the plans ready to ship?" (spec final, plan exists, AC addressed, work complete, not stale).
2. The **6-gate cut runbook (G0-G4)** is the "how we actually cut and publish the tag" ceremony. G0 consumes the readiness checks.

## Relationship to other docs

- `docs/internal/program-roadmap.md` - the cross-release execution plan, dependency graph, and scope ledger. The release plan aggregates; the roadmap sequences.
- `docs/internal/v1-architecture-and-decisions.md` - architecture + ratified decisions.
- `docs/internal/decisions/` - MADR ADRs for cross-cutting decisions (release-scoped decisions stay inline in the plan).
- `CHANGELOG.md` (repo root) - the user-facing NOTES layer; the GitHub Release body is derived from it. Kept separate from this internal governance.
