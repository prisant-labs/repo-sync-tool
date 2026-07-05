---
effort: E-16
title: Groups (repo tags) - implementation plan
status: ready
---

# E-16 - Groups (repo tags) - Implementation Plan

This is an **as-built record**, written after the fact from the shipped code and its commits, not a plan that was followed forward. It documents what was built, in what order, and what gates it passed, then lists the remaining work (the six Known defects in `spec.md`) mapped to where each is scheduled to be fixed.

## Ordered steps (as-built)

1. **Schema (already frozen, no change needed).** The `groups` and `repo_groups` tables shipped in migration `0002_activity_settings.sql` under E-02 (persistence and paths), well before this effort. Nothing in the 2026-07-03 build touched the schema; it was waiting for a store/IPC/GUI layer.
2. **Store layer (backend commit a85e0fc).** Six pure functions plus one query added to `crates/reposync-core/src/store.rs`: `groups_list`, `group_create`, `group_rename`, `group_delete`, `group_assign`, `group_unassign`, `groups_for_repo`. Duplicate-name and foreign-key-violation mapping reused the same `is_unique_violation` / `is_foreign_key_violation` helper pattern already established elsewhere in the file (the `repo.rs` settings functions), rather than inventing a new error-mapping convention.
3. **Five unit tests, store-layer, git-independent.** `group_create_lists_with_repo_count`, `duplicate_name_on_create_and_rename_maps_to_invalid_setting`, `assign_lists_and_unassign_round_trip`, `assign_missing_repo_or_group_is_not_found`, `delete_cascades_memberships_and_is_idempotent` (`store.rs`, test module lines 1093-1249). Each spins up a fresh temp SQLite pool and inserts bare `repos` rows directly (bypassing git) so the group tests always run rather than being skipped when git is absent, matching the test harness convention set by E-04 (git fixture test harness).
4. **IPC commands (same backend commit).** Seven thin `#[tauri::command]` adapters added to `src-tauri/src/commands/mod.rs`, each forwarding straight to its `store::` counterpart with no additional logic (grouping needs no per-repo lock and no git engine). No new command-layer tests were added; the adapters are exercised transitively through the store-layer tests above plus manual exercise through the GUI.
5. **Registration.** All seven commands added to the `use commands::{...}` import list and the `collect_commands!` macro in `src-tauri/src/lib.rs`, which is the single source of truth `tauri_specta` uses for both the runtime handler set and the generated bindings, so `src/lib/bindings.ts` regenerated in lockstep (no hand edits).
6. **Frontend surfaces (frontend commit 51daaa7).** Three consumers built on top of the generated bindings and the `useGroups` / `useGroupsForRepo` / `useRepoGroupMemberships` hooks in `src/hooks/queries.ts`:
   - `src/components/groups-nav.tsx` + `src/components/group-dialog.tsx`: the sidebar list, inline rename/delete, and the shared create/rename dialog.
   - `src/screens/repos.tsx`: the active-group filter (combined with status-chip and name-query filters), the "Filtered to..." banner, and per-row group chips.
   - `src/components/repo-detail.tsx`'s `GroupsSection`: the drawer's per-repo assignment toggles.
7. **No dedicated frontend tests.** The repo has no frontend test runner wired yet (no vitest), the same gap noted independently in `docs/backlog.md` BL-NI-21 (regression tests for the edge-wiring / GUI fixes). The three frontend surfaces above were verified by manual exercise during the build session, not by an automated test, and remain untested at that layer today.

## Gates passed (at the time of the 2026-07-03 build, per the 2026-07-04 Fable audit)

- `cargo clippy --workspace --all-targets -D warnings`: PASS.
- `pnpm typecheck` / `pnpm lint` / `pnpm build`: PASS / PASS / PASS.
- The five store-layer unit tests above: PASS (part of the broader `cargo test --workspace` run, which the audit separately flagged as not completing within 10 minutes for unrelated reasons, git-fixture tests spawning many subprocesses; the group tests themselves are git-independent and fast).
- No Codex adversarial review is recorded specifically for the groups vertical; the 2026-07-04 Fable audit is the first structured review this feature has received, and it is the source of every Known defect in `spec.md`.

## Test strategy (as-built, not prescriptive)

- Store-layer logic (CRUD, idempotency, cascade, error mapping) is exhaustively unit-tested and Tauri-free, matching the seam principle used elsewhere in `reposync-core`.
- The IPC command layer is untested in isolation; it is a direct pass-through with no branching logic of its own, so the store-layer coverage is the practical coverage.
- The three frontend surfaces (sidebar, Repos filter, drawer) have zero automated test coverage. This is a gap, not a decision; closing it depends on a frontend test runner existing first (BL-NI-21 territory), which is out of this effort's scope.

## Files touched

- `crates/reposync-core/src/store.rs` (group store functions + 5 unit tests; no new file, added to the existing store module).
- `src-tauri/src/commands/mod.rs` (7 command adapters).
- `src-tauri/src/lib.rs` (command registration in the `tauri_specta` builder).
- `src/lib/bindings.ts` (regenerated; `GroupSummary` type + 7 command bindings).
- `src/hooks/queries.ts` (`useGroups`, `useGroupsForRepo`, `useRepoGroupMemberships`).
- `src/components/groups-nav.tsx` (new), `src/components/group-dialog.tsx` (new).
- `src/screens/repos.tsx` (group filter, banner, chips).
- `src/components/repo-detail.tsx` (`GroupsSection`, `toggleGroup`).
- `src/components/app-shell.tsx` (`selectGroup`, sidebar wiring).

## Remaining work

Each Known defect in `spec.md` maps to one of two places: the v0.9.0 ship plan's Phase 1 (audit findings, per `execution-plan.md`) or a standalone backlog entry, for the one item that already has one.

| Defect (spec.md Known defects #) | Description | Where it is scheduled |
|---|---|---|
| 1 | Group filter false-empties during membership load or on failure (audit finding 7) | Phase 1 (audit findings), frontend fixes bucket, per `execution-plan.md`; a small `useRepoGroupMemberships` consumer fix (distinguish "loading/unknown" from "not a member" in `repos.tsx`'s filter). |
| 2 | BL-NI-22 (O(N) group filter fan-out) | Already tracked as its own backlog entry, `docs/backlog.md` BL-NI-22 (cross-references this spec; do not duplicate). Not scheduled to a specific phase yet; a natural fit for Phase 1 alongside the other audit-derived frontend/backend fixes if repo counts warrant it before ship, otherwise a post-ship follow-up. |
| 3 | Group dialog double-Enter double-submit | Phase 1 (audit findings), frontend fixes bucket; a one-line guard (ignore Enter while `busy`) in `group-dialog.tsx`. |
| 4 | Duplicate-name error surfaces as "invalid setting: name" with the wrong (Settings-screen) remediation | Phase 1 (audit findings); the actual fix (a per-field remediation override, or a generic non-Settings-specific message, on `AppError::InvalidSetting`) belongs to E-05 (error taxonomy)'s owner, since the error type is shared across every setting, not just groups. |
| 5 | Rename/delete buttons invisible while keyboard-focused (audit finding 12) | Phase 1 (audit findings), the a11y batch; add a `focus-within`/`focus-visible` reveal alongside the existing `group-hover/row` one in `groups-nav.tsx`. |
| 6 | Delete-active-group force-navigates to Repos | Phase 1 (audit findings), frontend fixes bucket; `doDelete`'s clear-filter call needs a path that clears `activeGroupId` without also switching `view`, separate from the "select a group" path that intentionally does both. |

## Definition of done (for the remaining work, not the shipped vertical)

- All six Known defects above are either fixed (with the fix noted in `spec.md`'s Task Summary and the AC/defect entry updated) or explicitly re-triaged to a later release with a recorded reason.
- If defect 2 (BL-NI-22) is fixed, the AC9 source citation in `spec.md` and the "Out of scope" line calling out the missing `repos_in_group` query are updated together, since they will no longer be accurate.
- No regression in the five existing store-layer unit tests; any fix to the command or store layer keeps them green.
