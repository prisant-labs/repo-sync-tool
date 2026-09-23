---
effort: E-21
title: The composite's unbuilt decisions
type: spec
status: draft
tier: SHOULD
scope: The eleven settled-but-unbuilt items the 2026-09-16 composite recorded as `build` pins, audited against the code on 2026-09-22. Criteria are written only for the four that are unblocked; the rest carry their blocker instead.
created: 2026-09-22
updated: 2026-09-23
linked-effort: E-21
linked-plan: null
linked-strategy-brief: null
linked-release: null
depends_on: [E-06, E-16, E-17, E-18, E-20]
ac-count: 12
source-count: 4
source: The annotation rail of `_local/design/4-composite/2026-09-16_composite.html`, which its own header calls "the build list"; the audit of all 29 build and backend pins recorded as section L.4 of `_local/design/3-decisions/2026-09-22_ui-delivery-plan.md`; and the code, read 2026-09-22.
---

# E-21 - The composite's unbuilt decisions

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** DRAFT, written 2026-09-22.
- **Why it exists:** the 2026-09-16 composite carries 43 pins in four kinds, 27 of them `build` -
  already decided, not questions. The 2026-09-21 decisions bench asked the `open` and `conflict`
  pins and **dropped all 27 build pins**, correctly (a build pin is not a question) but with nothing
  carrying them afterwards. Eleven turned out to be unbuilt with no spec, no roadmap row and no
  test. This is that missing home.
- **Built 2026-09-23** by a seven-agent workflow, then audited: AC-1 through AC-4 all land. Every
  audit returned WEAK on first pass and the findings were fixed in place - see below.
- **Two things the build changed about the spec itself.** AC-4's link-out is GONE: `on_navigation`
  (`src-tauri/src/lib.rs:440`) routes every navigation through `allow_navigation`, which permits
  only the `tauri` scheme, `tauri.localhost`, and dev `localhost`, so an https anchor is refused.
  The link rendered, took focus and did nothing. Removed, and AC-12 now carries the backend opener
  it would need. And AC-2 gained a gate nobody asked for: the sidebar's check is an ON-LAUNCH check,
  which `auto_update_check` exists to control, so shipping it ungated would have put a network call
  on every launch for a user who had turned that off.
- **Next:** the seven blocked criteria, in the order their blockers clear. AC-11 (the column budget)
  unblocks three of them at once.
- **Blockers:** AC-5 needs a backend field; AC-6 to AC-9 need design decisions that do not exist;
  AC-10 and AC-11 are hard-blocked on the Repos column budget (register L.6).

## Context

**A bench asks what is undecided. It cannot carry what is decided and unbuilt.** Those are two
artifacts, and collapsing them is how eleven settled items went untracked for six days. The
protocol written 2026-09-22 (`_local/design/_process/2026-09-22_ROUND-PROTOCOL.md`) now classifies
every item as BUILD, CHOICE, BLANK or CAPABILITY before anything is drawn, precisely so a BUILD item
lands here rather than on a switch.

**The audit that produced this list.** All 29 `build` and `backend` pins were checked against the
code on 2026-09-22: **16 built, 11 not, 2 needing a human look.** Two pattern results were wrong on
the first pass and corrected by reading the files - pin 2 was a false negative, pin 34 a false
positive. Pin 34 was the only one that was a live defect rather than absent work, and it shipped in
`d38310b`.

## In scope

The eleven unbuilt pins, each either built to a criterion below or carrying its blocker.

## Out of scope

- **Pins 15 and 26**, which the audit marked INSPECT. They need a look at the rendered screen, not a
  code check, and guessing at them is what this spec exists to stop.
- **The J.4 sync model.** Bench decision R2 is marked Unsure. AC-10 describes only the column that
  model would need, not the model.
- **Re-deciding any pin.** Every item here was settled. If one looks wrong now, it goes back through
  the register, not through this spec.

## Acceptance criteria

### Unblocked - build these

- [x] **AC-1: The Activity tab shows how many entries it holds.** The repo detail drawer's Activity
  tab carries a count beside its label. The panel already fetches this repository's activity on
  mount regardless of which tab is showing, so the number exists before the tab is opened and no new
  read is introduced. A repository with no activity shows no count rather than a zero, matching the
  sidebar's own null-versus-zero rule. Composite pin 33, register ledger BADGE. [S1][S2]
- [x] **AC-2: The sidebar says when an app update is available.** A line under the app name appears
  only when `app_check_for_update` reports `available == true`, and carries the new version. It does
  not appear when the app is up to date, and it does not appear when the update server could not be
  reached - those are three distinct states the `UpdateAvailability` type already separates, and
  conflating the last two would report a network failure as "you are current". The check and install
  controls stay in Settings (pin 41, already built); this is notification only. Composite pin 1,
  round-three note J.3: *"move to the sidebar - top under the app name, or bottom above Settings, in
  an obvious but nuanced way"*. [S1][S3]
- [x] **AC-3: Settings has a docked section navigation.** A nav rail lists the screen's sections and
  marks the one currently in view as the page scrolls. Every section is reachable by keyboard, and
  the marker follows the scroll position rather than only responding to clicks. Composite pin 28,
  register F.3 / STG1. [S1][S2]
- [x] **AC-4: Settings has an About section.** It names the running version, which
  `UpdateAvailability.currentVersion` and `getVersion()` both already supply. **Past releases are
  explicitly NOT in this criterion**: no data source for them exists, and inventing one is a
  capability, not a build. **Nor does it link out**, which is a correction to this criterion made
  while building it: `on_navigation` (`src-tauri/src/lib.rs:440`) routes every navigation through
  `allow_navigation`, which permits only the `tauri` scheme, `tauri.localhost`, and dev
  `localhost`. An https anchor is refused, so the link rendered, took keyboard focus and did
  nothing - worse than no control, and the shape `src/lib/ui-reachability.test.ts` guards one level
  up. AC-12 carries the backend opener a working link would need. Composite pin 29, register STG4.
  [S1][S4]

### Blocked - the blocker is the deliverable

- [ ] **AC-5: The Dashboard's release tag links out.** BLOCKED: `latestReleaseUrl` exists on
  `RepoDetail` but **not on `RepoSummary`**, which is what the Dashboard reads. Needs an additive
  backend field and a bindings regeneration, which is the serial `src-tauri` chokepoint. Composite
  pin 11, register S3. [S1]
- [ ] **AC-6: The drawer's repository name becomes a switcher.** BLOCKED on design. "Type-search
  switcher" is three words describing an interaction nobody has drawn. This is a CHOICE or BLANK
  item wearing a build pin's clothes, and it goes back to the round protocol's Stage 1 rather than
  being invented here. Composite pin 31, register P4. [S1]
- [ ] **AC-7: Multi-select with bulk actions.** BLOCKED. Register section C defers this *pending the
  partial-failure story being designed first* - what the interface says after "7 of 9 removed". That
  story still does not exist, and building the checkbox column without it produces an action whose
  failure mode is undefined. Composite pin 18, register RC3 + P6. [S1][S2]
- [ ] **AC-8: Resizable columns.** BLOCKED on AC-10's outcome. Resizing a table whose last two
  columns are already invisible at the maintainer's window width solves the wrong half of the
  problem. Composite pin 23. [S1][S2]
- [ ] **AC-9: Source gets its own column.** BLOCKED on the column budget, same as AC-10. Composite
  pin 20, register RR2. [S1][S2]
- [ ] **AC-10: The sync mode appears in the Repos toolbar or as a column.** BLOCKED twice over: the
  J.4 model it serves is unratified (bench decision R2, Unsure), and **the table has no width for
  another column.** Measured 2026-09-22 against the code: the ten current columns need 1218px
  including the 112px actions column, the sidebar is 232px, and at a 1440 window that leaves 1160px
  - so Checked truncates and Stars and Forks do not render at all. Adding an eleventh column while
  two are invisible is going backwards. Register L.6 carries the full budget table. Composite pin
  16, register P7.5. [S1][S2]

### Recorded gaps, not work

- [ ] **AC-11: The column budget is resolved before AC-8, AC-9 or AC-10 are attempted.** A minimum
  supported window width is chosen and the column set fits inside it. The maintainer's own proposal
  - Folder becomes an icon with the path on hover - saves 106px, more than twice any other single
  change, and is the largest available lever. It reverses T1, which was settled the same day
  *without the width budget attached*: a Stage 0 failure, recorded as such. [S2]
- [ ] **AC-12: The About section can neither list releases nor link to them, and both gaps are
  written down.** Two separate misses, found while building AC-4. (a) No data source for RepoSync's
  own release history exists - every `commands.*` release read takes a repo id and reads THAT
  repo's upstream. (b) There is no generic "open this URL" command either: every external open in
  this app is a bespoke repo- or path-scoped backend command (`repo_open_remote`,
  `repo_open_homepage`, `diagnostics_open_log_dir`), and the webview refuses a plain anchor. A
  working link therefore needs `src-tauri` work - an `app_open_releases`-style command through
  `opener.rs`'s existing URL-handling pattern. Until one exists, About names the version and stops.
  [S4]

## Dependencies

- **Upstream:** E-06 (the IPC contract behind every field named here), E-16 (groups), E-17 (branch
  and release intelligence), E-18 (the updater, which AC-2 reports on), E-20 (the Repos screen
  contract that AC-8 to AC-10 would change).
- **Downstream:** none. Nothing waits on this; it is settled work catching up with itself.

## Testing

Each built criterion carries a test that **fails against the code as it stands today**, and the
probe must be the historical absence itself - not an equivalent-looking edit. That rule is in the
round protocol's Stage 5, and it is there because the pin 34 tests were reported as proven when two
of them passed against the exact bug they were written to catch.

## Open questions

1. **The minimum supported window width.** 1280, 1366 or 1440. Everything in AC-8 to AC-10 waits on
   it.
2. **Is hover-only acceptable for the repository path?** It is invisible to keyboard and touch, and
   a truncated path at least names the drive and parent folder.
3. **Pins 15 and 26** need a look at the rendered screen before they can be classified at all.

## Sources

- [S1] `_local/design/4-composite/2026-09-16_composite.html`, the `ANN` annotation array - 43 pins,
  27 of kind `build`, which the file's own header calls the build list.
- [S2] `_local/design/3-decisions/2026-09-22_ui-delivery-plan.md`, revision 12, sections L.4 (the
  29-pin audit) and L.6 (the measured column budget).
- [S3] `src/lib/bindings.ts`, the `UpdateAvailability` type and the `app_check_for_update` command,
  which already separate available / up-to-date / unreachable.
- [S4] The code, read 2026-09-22: `getVersion()` in `app-shell.tsx`, and the absence of any release
  history source for RepoSync itself.

## Provenance

Written after the maintainer asked what the right path forward was, and after an audit found eleven
settled decisions with nothing tracking them. The count is the audit's, not an estimate: 16 built,
11 not, 2 needing a human look, out of 29 checked.
