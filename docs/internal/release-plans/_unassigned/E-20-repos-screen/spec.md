---
effort: E-20
title: Repos Screen (the repository list surface)
type: spec
status: draft
tier: MUST
scope: Retroactive contract for a surface that already ships, plus the decisions the register has settled but nobody has built. The open column-set and sync-model questions are deliberately carved out.
created: 2026-09-18
updated: 2026-09-21
linked-effort: E-20
linked-plan: null
linked-strategy-brief: null
linked-release: null
depends_on: [E-06, E-09, E-10, E-11, E-16, E-17]
ac-count: 19
source-count: 12
source: No tracked spec has ever owned this screen. Written from the user guide (the only tracked description of the intended behaviour), DESIGN.md, the shipped code, and the design decision register at `_local/design/3-decisions/ui-delivery-plan.md`.
---

# E-20 - Repos Screen (the repository list surface)

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** DRAFT, written 2026-09-18. This is a RETROACTIVE spec: most of what it describes already
  ships. It exists because the Repos screen is the largest surface in the app and the only major one
  with no acceptance criteria anywhere in the tracked tree, so nothing could say whether a change to
  it was a fix or a regression.
- **Effort id:** E-20 is provisional, assigned as the next free id after E-19 (tray popover) and
  parked in `_unassigned/` pending release slotting.
- **Next:** the maintainer answers the four open questions below, principally the default column set
  (composite pin 39, now decision R4 on the decisions bench). Until then the column set in AC-7 is
  descriptive of what ships, not prescriptive of what should.
- **Built 2026-09-21:** AC-18 (every status reachable as a filter) and AC-19 (one authority for group
  membership), the two criteria this spec was written with as unmet. 17 of 19 criteria were already
  met by the shipped screen; these were the two that were not. AC-18 carries a test PROVEN to fail
  against the pre-fix code. AC-19's test would have passed on the old code by design: that defect was
  three implementations of one rule, not a wrong answer, so its test pins the contract rather than
  catching a bug.
- **Blockers:** none for the criteria written here. Four decisions listed under Open questions block
  the criteria deliberately NOT written here.

## Context

RepoSync answers a quieter question than a Git client does: is my copy current, and is the project
still active (`docs/user-guide.md` section 1). The Dashboard answers that for the library as a whole.
The Repos screen is where the answer becomes per-repository and actionable, and it is the screen a
user spends most of their time in.

It has never had a spec. Nineteen efforts (E-01 through E-19) carry specs with numbered acceptance
criteria; the repository list, its status taxonomy, its filters, its columns and its scoping behaviour
are described only in the user guide, in `DESIGN.md`, in a gitignored design decision register, and
in the code itself. The practical cost is that a change to this screen has nothing to be checked
against. Four accessibility failures shipped on it and adjacent surfaces before a colour gate was
written, and each one passed every test and every review, because nothing tracked said what the screen
was supposed to do.

This spec closes that gap for what is settled. It does not try to settle what is open. The default
column set and the sync model are live questions the maintainer has not answered, and writing criteria
for them would be inventing decisions rather than recording them.

## Approach

Record the contract that already holds, at the level a future agent can verify.

The criteria below are written from four kinds of source, and each one says which:

1. **The user guide** (`docs/user-guide.md`), which is tracked, user-facing, and the only place the
   intended behaviour of this screen has ever been written down. Where the guide and the code
   disagree, the guide is treated as intent and the disagreement is recorded as drift.
2. **`DESIGN.md`**, which owns the status taxonomy and the separate signal register.
3. **The shipped code**, for behaviour that was decided in code and is worth pinning so it cannot
   drift silently.
4. **The design decision register** (`_local/design/3-decisions/ui-delivery-plan.md`), which is the
   only file permitted to record a UI decision. It is gitignored, so criteria sourced from it name
   the decision id and its handle rather than relying on the reader opening it.

Rejected alternative: **splitting this into several smaller specs** (one for the table, one for
filtering, one for the row actions). Nineteen criteria is above the usual comfortable ceiling for one
spec, and the split was considered. It was rejected because the Repos screen is a single surface with
a single set of interacting behaviours: the filters, the counts, the group scope and the columns are
not separable without each spec having to restate the others. The nineteen sibling specs are each one
effort covering one coherent thing, and this is one coherent thing.

## In scope

- The repository list itself: the row, its columns, and what each column shows.
- The status taxonomy as it renders on this screen, including the priority order when more than one
  state is true.
- The lag signal, the release and pull-request chips, and the visual register they are drawn in.
- Filtering: the status chips, the name search, the group scope, how they combine, and what the chip
  counts count.
- The screen's entry points: the add-repositories control, and opening a repository's detail drawer.
- Keyboard reachability and the colour-contrast floor for every pair this screen paints.

## Out of scope

- **The default column set and the width at which columns drop.** Composite pin 39 (the 1143px
  question) is unanswered, so AC-7 describes the columns that ship today and does not claim they are
  the right ones. A sync-mode column is specified nowhere, which is why the `updateMode` field
  shipped with no column behind it.
- **The sync model and the "Sync all" wording.** Conflict 2 is unresolved, and open question 2 says
  why it is less settled than it is usually described as being.
- **The repository detail drawer's internals.** The drawer opens from this screen; what it contains
  belongs to its own effort.
- **A card-based alternative to the column table.** Deferred by the maintainer to a future release
  (backlog BL-NI-115, the card view for the Repos screen).
- **Batch operation results.** The toast behaviour after a Check all run is a filed defect
  (backlog BL-NI-114, batch check results reported in stacking toasts) with an undecided shape.
- **Bulk assignment of repositories to groups**, which the user guide states does not exist in this
  release.
- **Whether an empty cell renders blank or as a dash.** The register settles blank; the shared table
  ships a dash, and the composite's own drawing hedges on whether the rule is table-wide or numeric
  columns only. A separate change made the dash readable, because while it exists it has to be.
  Removing it is this open decision, not that fix.

## Contract / deliverables

1. A repository list where every tracked repository is one row carrying identity, status, lag,
   recency and a per-row check action.
2. A status rendering that is colour plus icon plus word everywhere, with a defined priority when
   several states are true at once.
3. A separate signal register for release and pull-request information that can never be mistaken for
   a status colour.
4. Three filter dimensions (status chips, name search, group scope) that combine, with counts that
   are honest about which dimensions they include.
5. Keyboard reachability for every row and every primary action.
6. A machine-checked colour floor for every pair this screen paints.

## Acceptance criteria

- [x] **AC-1: Every tracked repository renders as one row** carrying identity (name and host), its
  status badge, a lag signal, a last-checked time, and a one-click check-now control. Source:
  `docs/user-guide.md` section 4 ("The Repos list shows every tracked repo as a row"). [S1]
- [x] **AC-2: Status is never carried by colour alone.** Every state renders as colour plus a distinct
  icon plus a text word, so the screen is readable in grayscale and by colourblind users with no loss
  of information. Source: `docs/user-guide.md` sections 4 and 16; `DESIGN.md` status taxonomy. [S1][S2]
- [x] **AC-3: When more than one state is true, the row shows the highest-priority one**, in the order
  paused beats failed beats dirty beats behind beats ahead beats in sync. A repository that is both
  dirty and behind shows Dirty, because the uncommitted change is what the user must act on first.
  Source: `docs/user-guide.md` section 4 (priority sentence). [S1]
- [x] **AC-4: A truly diverged history surfaces as Failed**, carrying the specific message that the
  branch has diverged and cannot fast-forward. It does not get its own colour, and the app never
  attempts a merge on the user's behalf. Source: `docs/user-guide.md` section 4. [S1]
- [x] **AC-5: The lag signal reads as a magnitude, not a number.** It is empty when the repository is
  current and fills further the more commits behind it is, up to a visual cap, so "a little behind"
  is distinguishable from "wildly behind" without reading the count. Source: `docs/user-guide.md`
  section 4. [S1]
- [x] **AC-6: Status filter chips narrow the list** across All, Behind, Dirty, Failed, Paused, Ahead
  and In sync, and a name filter box narrows it further by typing. Source: `docs/user-guide.md`
  section 4. [S1]
- [ ] **AC-7: The table renders exactly ten columns**, in this order: Repository, Status, Branch,
  Ahead, Behind, Groups, Folder, Checked, Stars, Forks, followed by an unlabelled actions column.
  This criterion is DESCRIPTIVE, not prescriptive: it pins today's set so a silent addition or removal
  is visible, and it becomes prescriptive only when the maintainer answers composite pin 39 (the
  1143px default-column question). Source: `src/screens/repos.tsx` lines 314 to 487 (the column
  definitions). [S9]

  > **The shipped ten and the settled ten are two different sets of ten, and it is worth being exact
  > about that.** The register settles the default as Repository, Status, Source, Sync mode, Branch,
  > Ahead, Behind, Checked, Updated, Functions, explicitly dropping Groups, Stars and Forks. So the
  > shipped set is neither a subset nor a superset of the settled one. Three of its columns are not in
  > the settled set at all, and three settled columns have never been built. Two of those three are
  > blocked rather than merely unbuilt: a sync-mode column waits on pin 39, which is why `updateMode`
  > ships on the wire with nothing rendering it, and an "updated" column has no field to read at all,
  > because the repository summary the table renders carries no last-updated timestamp. That backend
  > gap appears in no tracked document and is recorded here for the first time. Source: design
  > decision register, section J.2; composite pin 39. [S3][S12]
- [x] **AC-8: Release and pull-request information renders in the signal register, never a status
  colour.** The six status colours are reserved for repository freshness; release and pull-request
  chips use the separate magenta signal colour, so a new-release badge can never be read as a sync
  state. A repository can be in sync and carry an active release badge at the same time, because they
  are independent facts. Source: `docs/user-guide.md` section 5; `DESIGN.md`. [S1][S2]
- [x] **AC-9: Unknown upstream data is shown as unknown, never as zero.** A repository that is
  private, unreachable, rate-limited or not on GitHub shows its pull-request count as not yet checked
  or unavailable, and a previously known value is kept and stamped with an honest as-of time rather
  than being replaced by a fabricated zero. Source: `docs/user-guide.md` section 5. [S1]
- [x] **AC-10: Group scope, status chips and name search combine**, so a user can look at just the
  Dirty repositories inside one group. Selecting a group in the sidebar scopes this screen, and the
  screen shows which group is engaged, how many repositories are in scope, and a control that clears
  the scope. Source: `docs/user-guide.md` section 6. [S1]
- [x] **AC-11: The engaged group is marked on every screen, not only this one.** Decision A2 in the
  register (mark the engaged group everywhere) was built in PR #93; a scope that is invisible on the
  screen it governs is the failure this closes. Source: design decision register, decision A2; PR #93.
  [S3]
- [x] **AC-12: Filter chips use the filled visual language of the status chips.** Decision C1 in the
  register (give filter chips the status chip's filled language) was built in PR #93. Source: design
  decision register, decision C1; PR #93. [S3]
- [x] **AC-13: A filter chip's count counts the group scope and the name search, but NOT the status
  dimension the chips themselves select.** Counting after the status filter would make every chip
  read zero except the engaged one. The chips and the table body share one filtered base, so they are
  structurally unable to disagree. Source: PR #95; session log 2026-09-18 (decisions made). [S5]
- [x] **AC-14: Every action on a row is reachable by keyboard through a real button, and the row
  itself is NOT a keyboard control.** Each row carries a dedicated "Open details" button that opens
  the drawer, a check-now button, and a folder button; clicking anywhere else on the row also opens
  the drawer, as a mouse convenience only. The row deliberately does not carry `role="button"` or a
  tab index, because nesting a keyboard-operable row around those buttons produced an invalid
  accessibility tree and an ambiguous Enter or Space target. Source: `src/screens/repos.tsx` lines 433
  to 444 and 668 to 686, which record the decision and the Codex review of PR #73 that prompted it.
  [S9][S10]

  > **Documentation drift, and it is a bug by this project's own cadence.** `docs/user-guide.md`
  > section 16 still tells users that repository rows are focusable controls and that Tab then Enter
  > or Space opens the drawer. That was true once and is not true now. The guide describes a keyboard
  > path that does not exist, which is worse than describing none, because a keyboard user following
  > it will conclude the app is broken. Fixing the guide belongs with this spec, not after it.
- [x] **AC-15: Every colour pair this screen paints is measured against its WCAG floor by an automated
  gate**, and a new text colour on a new background adds a row to that gate in the same change. The
  gate reads token values out of `src/index.css`, so editing a token is what makes it fail. Source:
  `src/lib/contrast.test.ts` (the pair list and its documented limitation); PR #95. [S4]
- [ ] **AC-16: The screen has no control that renders but does nothing**, and no hardcoded value
  standing in for a per-repository setting. One known violation is open: the detail drawer hardcodes
  a fast-forward-only update mode regardless of what the repository is actually configured for, which
  belongs to the sync-model slice rather than to this screen. Source: session log 2026-09-18
  (outstanding issues). [S5]

- [ ] **AC-17: A status filter that is selected stays visible even when it matches nothing.** A chip
  renders when its count is above zero OR when it is the selected filter, so a filter that is still
  narrowing the table can never be invisible. Without this, selecting Behind and then searching for
  an in-sync repository leaves an empty table beside chips reading All 1 and In sync 1, with nothing
  on screen explaining why no rows are shown. Source: the Codex adversarial review of PRs #93 to #96,
  finding 3, 2026-09-18. [S11]

- [x] **AC-18: Every status a repository can be in is reachable as a filter.** One was not. The chip
  row iterates a fixed list of six statuses that omits "no upstream", so a repository in that state is
  counted in the All total, renders its status in the table, and cannot be isolated by any chip. The
  user can see the state exists and has no way to ask for it. Source: `src/screens/repos.tsx` line 38
  (the status order) against `src/lib/status.ts`, which defines seven. [S9]
  **MET 2026-09-21.** `STATUS_ORDER` now lives in `src/lib/status.ts` beside the `RepoStatus` type it
  must cover, carries `noUpstream`, and is proved exhaustive at compile time - dropping a state from
  it fails `pnpm typecheck` with an error naming the missing state. A runtime test in
  `status.test.ts` checks the same property against `STATUS_STYLE`'s keys, and
  `repos.test.tsx` proves a no-upstream repository can actually be isolated by its chip.
- [x] **AC-19: One fact has one authority.** The rule for whether a repository falls inside the
  engaged group is implemented twice: once in a shared module the sidebar and the Dashboard use, and
  again inline on this screen, twice over. The observable consequence is that the sidebar's repository
  count and this screen's All chip can disagree, because the shared module never sees this screen's
  name filter. That module's own documentation says it exists precisely to stop this. Source:
  `src/lib/group-scope.ts` against `src/screens/repos.tsx` lines 216 to 248. [S9]
  **MET 2026-09-21.** All three inline copies on this screen (the chip-count population, the group
  pill's count, and the still-loading guard on the table body) now derive from a single
  `groupScope(activeGroupId, membershipMap)` call, the same one the sidebar and the Dashboard use.
  The two populations stay deliberately different - the chip counts apply the name filter and the
  group pill's count does not - and a test pins that difference so it cannot be "fixed" into a
  third implementation.

## Dependencies

- **Upstream:** E-06 (the IPC contract that defines `RepoSummary`, which is what a row renders),
  E-09 (activity records behind the last-checked time), E-10 (the GitHub client behind the release
  and pull-request chips), E-11 (the summary engine behind the counts), E-16 (groups, which supply
  the scope), E-17 (branch and pull-request intelligence, which supplies the signal chips).
- **Downstream:** the repository detail drawer opens from here, and the tray popover (E-19) reuses
  this screen's status logic rather than reimplementing it.
- **Cross-cutting:** this is a frontend-only surface. It adds no backend command of its own; every
  criterion above reads an existing one.

## Testing

- **Component tests** in the existing vitest harness for the filter combination in AC-10 and AC-13,
  including the case the screenshots never covered: name search and status filter applied together
  inside a group scope.
- **The colour gate** (`src/lib/contrast.test.ts`) already covers AC-15 for the pairs on its list.
  AC-15's real risk is completeness of that list, which no jsdom test can settle because jsdom
  computes no colours.
- **Screenshots through the browser preview harness** for anything about layout, width or wrapping.
  Two defects in one session came from opening an image, and nothing in jsdom could have caught
  either: a label wrapping inside a fixed-height row, and a fixture claiming a release count its own
  list contradicted. Use `_local/design/_gate/headless-shot.mjs`, which drives a headless browser over
  the debugging protocol with no window, never a tool that opens a real browser on the maintainer's
  desktop.
- **Manual, maintainer only:** whether the column set in AC-7 is the right one. That is a judgement
  about a screen, not a property a test can assert.

## Open questions

These four are the maintainer's, and each one blocks acceptance criteria this spec deliberately does
not contain.

1. **The default column set and the width at which it degrades** (composite pin 39, the 1143px
   question). It gates AC-7, and it separately gates the sync-mode column, which is why the
   `updateMode` field shipped on the wire with nothing rendering it. Until this is answered, AC-7
   records what ships rather than what should.
2. **The sync model and the "Sync all" wording** (conflict 2). This is less settled than it looks
   from outside, and the distinction matters. The sync model that "Sync all" supposedly conflicts with
   is recorded in the register as a RECOMMENDATION the maintainer asked for, not as a mark he made,
   and his own recorded words on it are "I don't know what to do with this information". His own
   wording, "Sync all", is marked Keep in three separate places. So this is not two settled decisions
   in tension; it is a kept phrase against an unratified proposal. Independently of the name, no
   bulk-apply command exists in the backend at all, so any criterion here would describe something
   that cannot be built yet.
3. **The group control's shape**, which is a different question from where it sits. The two get run
   together and should not be. Nothing settles the shape. After decision C1 gave filter chips a filled
   language, the group control is the only outlined round pill left in the app, and the two register
   items that would settle the tag family are marked Keep and Unsure respectively, which is not an
   answer. Treat the shape as an open criteria gap rather than assuming C1 converts it.
4. **Where the group scope is shown** (conflict 5). The register keeps the scope folded into the
   toolbar's own control, which is built, and a later, more specific requirement asks for it above the
   filter row. These may be two different things, a filter CONTROL and a scope INDICATOR, and the
   composite draws both. It gates any criterion about where a user looks to see what is scoping them.
5. **Whether nineteen criteria in one spec is the right unit.** The usual guidance is to split above
   ten. It was kept as one because the Repos screen's filters, counts, scope and columns cannot be
   specified independently of each other without restating each other. If the maintainer disagrees,
   the natural split is the table and its columns in one spec, and filtering and scoping in another.

## Sources

| Id | Source | Class | What it supports |
|---|---|---|---|
| S1 | `docs/user-guide.md` sections 4, 5, 6, 16 | A (tracked, user-facing, authoritative on intent) | AC-1 to AC-6, AC-8, AC-9, AC-10, AC-14 |
| S2 | `DESIGN.md` (status taxonomy and the separate signal register) | A (tracked, the design system's source of truth) | AC-2, AC-8 |
| S3 | `_local/design/3-decisions/ui-delivery-plan.md` (decisions A2 and C1) | A (the only file permitted to record a UI decision), but gitignored | AC-11, AC-12 |
| S4 | `src/lib/contrast.test.ts` | A (the shipped gate itself) | AC-15 |
| S5 | `_local/_session-logs/2026-09-18_07-46_claude_four-prs-and-the-gate-that-paid-immediately.md` | B (a session record, accurate at the time of writing) | AC-7, AC-13, AC-16 |
| S6 | `docs/backlog.md` rows BL-NI-114 (batch check results in stacking toasts) and BL-NI-115 (a card view for the Repos screen) | A (tracked) | Out of scope |
| S7 | PRs #93 and #95 on `prisant-labs/repo-sync-tool` | A (merged code) | AC-11, AC-12, AC-13, AC-15 |
| S8 | `docs/internal/release-plans/README.md` (the effort folder and promote flow) | A (tracked) | The placement of this spec |
| S9 | `src/screens/repos.tsx` (column definitions at 314 to 487; the row keyboard decision at 433 to 444 and 668 to 686; `countBase` at 216 to 255) | A (shipped code) | AC-7, AC-13, AC-14 |
| S10 | The Codex adversarial review of PR #73, finding 2 (the nested keyboard row) | B (a review record, cited in a code comment rather than read directly) | AC-14 |
| S11 | The Codex adversarial review of PRs #93 to #96, 2026-09-18, saved at `_local/codex/2026-09-18_adversarial-review_prs-93-96.md` | A (a review record, read directly, and its finding verified against the code) | AC-17 |
| S12 | `src/lib/bindings.ts`: the repository summary the table renders carries no last-updated timestamp, while the detail type does | A (generated from the Rust wire types) | AC-7 |

## Revisions

| Date | Change | Why |
|---|---|---|
| 2026-09-18 | Created. | The Repos screen is the largest surface in the app and had no acceptance criteria anywhere in the tracked tree. |

## Provenance

Written 2026-09-18 as the first tracked contract for a surface that had shipped without one. The
criteria are recorded, not invented: each one cites the user guide, `DESIGN.md`, the design decision
register, the shipped colour gate, or a merged pull request. The four open questions were left open
on purpose, because writing criteria for an unanswered decision would put an invented answer into the
one place the project treats as authoritative.
