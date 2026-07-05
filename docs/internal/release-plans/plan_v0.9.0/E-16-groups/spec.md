---
effort: E-16
title: Groups (repo tags)
tracking-issue: "#18"
status: ready
tier: MUST
scope: V1 (retroactive as-built contract)
depends_on: [E-02]
source: crates/reposync-core/migrations/0002_activity_settings.sql (frozen schema, groups/repo_groups tables); crates/reposync-core/src/store.rs (group_* store functions, lines 504-660); src-tauri/src/commands/mod.rs (7 group IPC commands, lines 371-444); src-tauri/src/lib.rs (command/event registration); src/lib/bindings.ts (generated GroupSummary type + command bindings); src/components/groups-nav.tsx, src/components/group-dialog.tsx, src/screens/repos.tsx, src/components/repo-detail.tsx, src/hooks/queries.ts (frontend surfaces); docs/internal/release-plans/plan_v0.9.0/features-and-outcomes.md Section 5 ("Build and manage the library", the Groups row and scope note); _LOCAL/audit/2026-07-04_18-21_fable-audit.md (finding 3, the stale "spec/build not started" claim this spec corrects, plus findings 7 and 12 in the Known defects section below)
---

# E-16 - Groups (repo tags)

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** Built 2026-07-03 (backend commit a85e0fc, frontend commit 51daaa7). The full vertical, schema, store layer, IPC commands, and every GUI surface (sidebar section, Repos filter, drawer assignment), shipped before this spec existed. This document is written retroactively on 2026-07-04 as the as-built contract, per the 2026-07-04 Fable audit (`_LOCAL/audit/2026-07-04_18-21_fable-audit.md`, finding 3: the release plan claimed groups "spec/build not started" while the full vertical had already shipped, and no spec existed anywhere under the release plan). The `groups` / `repo_groups` schema itself predates this build; it has been frozen in migration `0002_activity_settings.sql` since E-02 (persistence and paths), waiting for a store/IPC/GUI layer.
- **Next:** the six Known defects below are open, none of them blocks the feature working for the common case. Five are frontend correctness/UX bugs (filter false-empty during load or failure, a dialog double-submit, a misleading duplicate-name error, keyboard-invisible row buttons, a delete-active-group navigation surprise); one, BL-NI-22 (O(N) group filter fan-out), is a scalability follow-up. All six feed the v0.9.0 ship plan's Phase 1 (audit findings) work; see `implementation-plan.md`'s Remaining work section for the phase-or-backlog mapping of each.
- **Blockers:** none for the shipped vertical. The open defects are the remaining work, not a blocker to using the feature.

## Context

RepoSync users who track more than a handful of repositories need a way to impose their own taxonomy on the library ("work," "personal," "forks," "reference") instead of scrolling one flat list. The `groups` / `repo_groups` many-to-many schema was frozen into migration 0002 early in the build (owned by E-02, persistence and paths), but the store layer, the IPC commands, and the GUI were explicitly deferred: `docs/internal/release-plans/plan_v0.9.0/features-and-outcomes.md` Section 5 called the detailed spec "intentionally deferred until the GUI is finalized, because the feature is primarily a UI surface and should be designed as one coherent screen alongside the rest of the interface."

That GUI finalized on 2026-07-03, and groups shipped as part of the same session as the sidebar, the Repos screen, and the repo-detail drawer, without a spec ever being written first. This document is that deferred spec, written after the fact from the code as it actually behaves (an as-built contract), not from a pre-build design. Where the shipped behavior has a real bug, it is recorded below in Known defects rather than smoothed over: this spec describes what RepoSync does today, correct and incorrect alike.

## In scope

- The N:M `groups` / `repo_groups` schema (already frozen; described, not re-specified, in Data model below).
- Seven IPC commands covering the full CRUD + membership surface: `group_list`, `group_create`, `group_rename`, `group_delete`, `group_assign`, `group_unassign`, `groups_for_repo`.
- Three GUI surfaces: the sidebar "Groups" section (list, create, rename, delete), the Repos screen's group filter and per-row group chips, and the repo-detail drawer's per-repo group-assignment toggles.
- A fixed 8-swatch preset color model for new groups, with a graceful "no color" fallback everywhere a group's color renders.
- Cascade-delete semantics: deleting a group removes its memberships, never the repos in it.

## Out of scope

- A dedicated, full-page group-management screen (today's create/rename/delete lives entirely in the sidebar's inline rows and the `GroupDialog`).
- Bulk operations (assigning many repos to a group at once, merging two groups, bulk delete).
- Nested groups, group hierarchy, or rule-based/smart (dynamic-membership) groups.
- Editing a group's color after creation (the rename dialog changes the name only; see Contract below).
- A per-group check cadence or update policy (cadence and policy are still per-repo only; groups are a labeling/filtering surface, not a policy scope).
- The `repos_in_group(group_id)` batch query. Filtering and per-row chips both fan out one `groups_for_repo` call per repo today; the batch query is BL-NI-22 below, not yet built.

## Contract / deliverables

### (a) Data model

Frozen in `crates/reposync-core/migrations/0002_activity_settings.sql` (owned by E-02, persistence and paths):

```sql
CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    color TEXT
);
CREATE TABLE repo_groups (
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (repo_id, group_id)
);
```

- `groups.name` is `UNIQUE`; the store layer maps its constraint violation to a typed error rather than letting a raw database error escape (see Command surface below).
- `groups.color` is a nullable, unconstrained `TEXT` column. The schema places no format requirement on it; today only the frontend's fixed preset list (see Color model below) ever writes a value into it.
- `repo_groups` is a pure composite-key join table: no extra columns, both foreign keys `ON DELETE CASCADE`. A repo can belong to zero, one, or many groups; a group can have zero or many member repos.
- The wire type is `GroupSummary` (`crates/reposync-core/src/ipc.rs`, camelCase over IPC per `src/lib/bindings.ts`):

```ts
export type GroupSummary = {
	id: number,
	name: string,
	color: string | null,
	repoCount: number,
};
```

  `repoCount` is not a stored column; it is computed fresh on every `group_list` read (see below).

### (b) Command surface

All seven commands are thin adapters in `src-tauri/src/commands/mod.rs` over the `reposync_core::store` group functions (`crates/reposync-core/src/store.rs`, lines 504-660): grouping is pure SQLite metadata with no git and no per-repo lock, so each handler just forwards the pool.

| Command | Params | Returns | Errors |
|---|---|---|---|
| `group_list` | none | `Vec<GroupSummary>`, name-ordered | none beyond a generic DB failure |
| `group_create` | `name: String, color: Option<String>` | `GroupSummary` (fresh group, `repoCount: 0`) | `AppError::InvalidSetting { field: "name" }` on a duplicate name |
| `group_rename` | `id: i64, name: String` | `()` | `InvalidSetting { field: "name" }` on a duplicate name; `AppError::NotFound { entity: "group {id}" }` if `id` does not exist |
| `group_delete` | `id: i64` | `()` | none; idempotent (a missing `id` is not an error) |
| `group_assign` | `repo_id: i64, group_id: i64` | `()` | `AppError::NotFound { entity: "repo {repo_id} or group {group_id}" }` if either id does not exist; otherwise idempotent (re-assigning an existing membership is a no-op) |
| `group_unassign` | `repo_id: i64, group_id: i64` | `()` | none; idempotent (removing a nonexistent membership is not an error) |
| `groups_for_repo` | `repo_id: i64` | `Vec<i64>`, ascending, de-duplicated | none; an unknown `repo_id` returns an empty list rather than an error, because the query has no foreign-key check of its own |

Detail worth calling out:

- `group_list` computes `repoCount` with a `LEFT JOIN` + `GROUP BY` over `repo_groups`, so an empty group correctly reports `0` rather than being excluded (`store.rs` lines 511-533).
- The duplicate-name mapping on both `group_create` and `group_rename` goes through a shared `is_unique_violation` check on the SQLite error, so a raw constraint violation never reaches the caller as an opaque database error (`store.rs` lines 539-593, 656+).
- `group_assign`'s idempotency is `INSERT OR IGNORE`, which swallows the primary-key collision for a duplicate membership but does **not** swallow a foreign-key violation; a missing `repo_id` or `group_id` is detected via `is_foreign_key_violation` and re-mapped to `AppError::NotFound` (`store.rs` lines 605-626).
- `group_delete`'s idempotency and `repo_groups`'s `ON DELETE CASCADE` together mean deleting a group is always safe to call twice and always leaves zero orphaned membership rows (see Behavior on delete below).
- Registration: all seven commands are wired into the `tauri_specta` builder in `src-tauri/src/lib.rs` (both `collect_commands!` and the `use commands::{...}` import list), so `src/lib/bindings.ts` stays generated, not hand-written, for this surface.

### (c) UI surfaces

**Sidebar "Groups" section** (`src/components/groups-nav.tsx`). Below the primary nav, a "Groups" header with a "New group" plus-button, then:
- An "All repositories" row that clears the active group filter.
- One row per group: a color dot, the name, and the live member count. Hovering a row reveals rename (pencil) and delete (trash) icon buttons in its place; clicking delete swaps the count for an inline "Delete?" confirm/cancel pair rather than opening a modal.
- Selecting a group row calls `onSelectGroup(id)`, which in `src/components/app-shell.tsx`'s `selectGroup` both sets the active group id and unconditionally switches the main view to Repos (`setView("repos")`). This is deliberate for the common "click a group, see its repos" path, but has a side effect on delete; see Known defects.
- `GroupDialog` (`src/components/group-dialog.tsx`) backs both create and rename, opened from this section (see Color model below for its create-only color picker).

**Repos screen filter** (`src/screens/repos.tsx`). When a group is active:
- A banner reading "Filtered to {name}, N repos" with a "Clear filter" action, `N` being `inGroupCount` (members of the active group before the status-chip and name-query filters narrow further, lines 101-105).
- The group filter combines with the status-chip filter and the name-query filter in one `filtered` memo (lines 107-115): a repo must pass all three to show.
- Each visible repo row renders a small chip per group it belongs to (color dot + name, `GroupChip`, lines 354-364), looked up from the same membership map the filter uses.
- Membership data for the filter and the chips both come from `useRepoGroupMemberships` (`src/hooks/queries.ts`), which fans out one `groups_for_repo` IPC call per visible repo into a `Map<repoId, groupId[]>`. This is the O(N) pattern tracked as BL-NI-22 below.

**Drawer assignment** (`src/components/repo-detail.tsx`, `GroupsSection`). The repo-detail drawer lists every group with a toggle switch reflecting this repo's current membership (`memberIds.includes(g.id)`). Flipping a switch calls `group_assign` or `group_unassign` for that one group, then refreshes three things in sequence: this repo's own memberships, the group list (so sidebar counts stay live), and the parent screen's list/membership map via `onChanged`. Each switch disables individually while its own toggle is in flight (`groupBusyId`), not the whole panel.

### (d) Behavior on delete

- `group_delete(id)` is unconditionally idempotent: deleting an id that does not exist returns `Ok(())`, never an error (`store.rs` lines 597-603, exercised by the `delete_cascades_memberships_and_is_idempotent` test).
- The `repo_groups.group_id` foreign key carries `ON DELETE CASCADE`, so deleting a group row cascades away every `repo_groups` membership row for it in the same transaction. The repos themselves, and their membership in any *other* group, are untouched; only rows keyed to the deleted `group_id` disappear.
- On the frontend, `GroupsNav`'s `doDelete` calls `group_delete`, toasts "Group deleted," and, if the deleted group was the active filter (`activeGroupId === id`), calls `onSelectGroup(null)` to clear it. Because `onSelectGroup` is wired to `selectGroup` in `app-shell.tsx`, which unconditionally does `setView("repos")` as well as clearing the id, deleting the currently-filtered group also force-navigates the user to the Repos screen, even if the delete was triggered from Dashboard, Activity, or Settings (the sidebar, and its Groups section, renders on every screen). This is Known defect 6 below, not a designed behavior.

### (e) Color model

- A new group's color is chosen at create time from 8 fixed preset swatches, oklch strings tuned to the Graphite palette so each reads on both light and dark card surfaces (`GROUP_COLORS` in `src/components/group-dialog.tsx`, lines 15-24). The first swatch is the default selection; the picker is a row of filled circles with `aria-pressed` on the selected one.
- Rename mode does not expose the color picker at all: `group_rename`'s IPC signature carries only `id` and `name`, so a group's color, once set at creation, cannot be changed later short of deleting and recreating the group.
- `color` is stored and transmitted as a raw string with no format validation anywhere in the stack (schema, store layer, or command layer); the only writer today is the fixed preset list above, so in practice every stored value is one of the 8 known oklch strings or `null`.
- A `null` color renders as a muted gray dot (`bg-muted-foreground/50`) rather than an inline `backgroundColor` style, consistently across the sidebar rows, the Repos-screen group chips, and the drawer's group list. There is no "no group has a color yet" empty state distinct from this per-row fallback.

## Acceptance criteria

- [x] AC1: A repo can belong to zero, one, or many groups via the `repo_groups` join table (composite primary key `(repo_id, group_id)`, both foreign keys `ON DELETE CASCADE`). Source: migration `0002_activity_settings.sql`, lines 30-40. **Done** (frozen schema, predates this build).
- [x] AC2: `group_list` returns every group with a live, correctly-computed member count (0 for an empty group), name-ordered, and an empty database returns an empty list rather than an error. Source: `store::groups_list` (`store.rs` lines 511-533); `group_list` command (`commands/mod.rs` lines 381-383); test `group_create_lists_with_repo_count`. **Done.**
- [x] AC3: `group_create(name, color)` creates a group and returns it as a `GroupSummary` with `repoCount: 0`; a duplicate `name` is rejected as `AppError::InvalidSetting { field: "name" }`, never a raw database error. Source: `store.rs` lines 539-566; migration 0002's `name TEXT NOT NULL UNIQUE`; test `duplicate_name_on_create_and_rename_maps_to_invalid_setting`. **Done** (see Known defect 4 for the error's misleading remediation text).
- [x] AC4: `group_rename(id, name)` renames a group in place; a duplicate name is rejected the same way as create; a missing `id` is `AppError::NotFound { entity: "group {id}" }`. Source: `store.rs` lines 568-593; same test as AC3. **Done.**
- [x] AC5: `group_delete(id)` is idempotent (a missing id is `Ok(())`, not an error), and deleting a group cascades away every membership row for it without touching the member repos. Source: `store.rs` lines 595-603; migration 0002's `ON DELETE CASCADE`; test `delete_cascades_memberships_and_is_idempotent`. **Done.**
- [x] AC6: `group_assign(repo_id, group_id)` is idempotent for an already-existing membership, but a missing repo or group surfaces as `AppError::NotFound` via the foreign-key violation rather than silently doing nothing; `group_unassign` is unconditionally idempotent. Source: `store.rs` lines 605-641; tests `assign_lists_and_unassign_round_trip`, `assign_missing_repo_or_group_is_not_found`. **Done.**
- [x] AC7: `groups_for_repo(repo_id)` returns the ascending, de-duplicated list of group ids a repo belongs to (empty for no memberships or an unknown repo id). Source: `store.rs` lines 643-654; test `assign_lists_and_unassign_round_trip`. **Done.**
- [x] AC8: The sidebar "Groups" section lists every group (color dot, name, live member count) alongside an "All repositories" clear row and a "New group" affordance; selecting a group filters the Repos screen to its members. Source: `src/components/groups-nav.tsx`; `src/components/app-shell.tsx` `selectGroup`. **Done** (see Known defect 6 for a side effect of this same wiring).
- [x] AC9: The Repos screen filters its list to the active group's members, combinable with the status-chip and name-query filters, shows a "Filtered to {name}, N repos" banner with a clear action, and renders a chip per group on each visible repo row. Source: `src/screens/repos.tsx` lines 101-148, 216-224, 354-364. **Done** (see Known defect 1 for a false-empty edge case and Known defect 2 / BL-NI-22 for the fan-out this relies on).
- [x] AC10: The repo-detail drawer's "Groups" section lists every group with a toggle reflecting this repo's membership; toggling calls `group_assign` / `group_unassign` and refreshes this repo's memberships, the group list, and the parent screen. Source: `src/components/repo-detail.tsx` `GroupsSection`, `toggleGroup`. **Done.**
- [x] AC11: A new group's color is chosen from 8 fixed oklch preset swatches at create time; rename does not expose a color change; a `null` color renders a consistent muted-gray fallback dot everywhere a group's color would otherwise show. Source: `src/components/group-dialog.tsx` `GROUP_COLORS`; color-dot fallbacks in `groups-nav.tsx` and `repos.tsx`. **Done.**

## Known defects (open)

These six are real, verified bugs in the shipped vertical, surfaced by the 2026-07-04 Fable audit (`_LOCAL/audit/2026-07-04_18-21_fable-audit.md`). None of them is fixed by this spec; the spec only records them so the as-built contract is honest about what does not yet work as intended. See `implementation-plan.md`'s Remaining work section for where each is scheduled.

1. **Group filter false-empties during membership load or on failure** (audit finding 7). `useRepoGroupMemberships` (`src/hooks/queries.ts`) exposes `data: Map | null`, `null` both while the fan-out is still loading and forever after it fails. In `src/screens/repos.tsx`'s `filtered` memo (line 110), `!membershipMap?.get(r.id)?.includes(activeGroupId)` treats a `null` map the same as "this repo is not a member," so with an active group filter the Repos list renders "No repositories match this filter" during every load and permanently if the fan-out ever fails, instead of a loading state or an error state.
2. **BL-NI-22 (O(N) group filter fan-out).** `useRepoGroupMemberships` issues one `groups_for_repo` IPC round-trip per visible repo rather than a single batched query. Fine at V1 repo counts, but it does not scale; the fix is a `repos_in_group(group_id) -> Vec<i64>` store function plus IPC command (or folding group ids directly into `RepoSummary`), additive to the schema and the IPC surface. Tracked in `docs/backlog.md` as BL-NI-22; do not duplicate the backlog entry, this spec only cross-references it.
3. **Group dialog double-Enter double-submits.** `GroupDialog`'s name `Input` calls `submit()` directly from its `onKeyDown` handler on Enter (`src/components/group-dialog.tsx` lines 111-113) with no guard against a second Enter firing before the first `await` resolves and `busy` re-renders as `true`. Two rapid Enter presses can issue two `group_create` (or `group_rename`) calls for the same input.
4. **Duplicate-name error surfaces as "invalid setting: name" with wrong remediation.** `AppError::InvalidSetting`'s display message is `"invalid setting: {field}"` and its remediation is `"A setting has an invalid value. Correct it in Settings."` (`crates/reposync-core/src/error.rs`). For a duplicate group name this reaches the user verbatim via the dialog's error toast: a message written for the Settings screen, pointing at a Settings screen that has no group-name field at all, when the actual fix is to pick a different name in the group dialog that is still open.
5. **Rename/delete buttons are focusable but invisible while keyboard-focused** (audit finding 12). In `GroupRow` (`src/components/groups-nav.tsx` line 188), the rename/delete icon buttons are revealed only via `opacity-0` to `group-hover/row:opacity-100`, a mouse-hover-only transition with no `focus-within`/`focus-visible` counterpart. A keyboard user who tabs to either button lands on a control that is present, focusable, and fully transparent.
6. **Delete-active-group force-navigates to Repos.** As described in Behavior on delete above, deleting the group that is the current active filter clears the filter through the same `onSelectGroup` path used for selecting a group, which unconditionally calls `setView("repos")` in `app-shell.tsx`. A user who opens the sidebar from Dashboard, Activity, or Settings and deletes their currently-active group filter is unexpectedly dropped onto the Repos screen.

## Dependencies

- Upstream: E-02 (persistence and paths) owns migration 0002 and the `groups` / `repo_groups` tables this effort builds on; no schema change was needed to ship the store/IPC/GUI layer.
- Downstream: none yet. E-17 (branch and PR intelligence) and E-18 (auto-update and distribution) do not read or write group data.

## V1.1 extension points

- `repos_in_group(group_id) -> Vec<i64>` (or folding group ids into `RepoSummary`) to remove the O(N) fan-out (BL-NI-22 above).
- A color picker (not just 8 presets) and the ability to change a group's color after creation via `group_rename` or a new command.
- Bulk assignment (drag-select or checkbox multi-select repos into a group) and a dedicated group-management screen, instead of the sidebar's inline rows.
- Smart/dynamic groups (rule-based membership, e.g. "all repos with a failed status") layered on top of the static membership model here.
- A per-group default check cadence or update policy, if the "policy scope" question comes up again; today groups are purely a labeling and filtering surface.

## Open questions

- **Tier confirmation:** this spec marks E-16 MUST because the feature already shipped and is in active use in the built GUI (per `docs/internal/program-roadmap.md`'s 2026-07-04 note); flag if that should be revisited.
- Whether the six Known defects should each get their own backlog entry (beyond BL-NI-22, which already has one) or ride along as one line item in the Phase 1 (audit findings) work; see `implementation-plan.md`.
- Whether "Correct it in Settings" should become a generic, field-agnostic remediation string for `InvalidSetting` (defect 4) or whether `InvalidSetting` should gain a per-field remediation override; either fix belongs to E-05 (error taxonomy)'s owner, not this spec.
