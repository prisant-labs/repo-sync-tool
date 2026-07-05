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

**All six Known defects are fixed as of 2026-07-05.** Five landed in commit `4ab54bf`; the sixth (BL-NI-22) landed in commit `261a689` (core store fn + unit test, additive IPC command, bindings regen, frontend swap). Table kept for the historical record of where each was scheduled and what actually fixed it (the original plan called all six "Phase 1 (audit findings)"; that held).

| Defect (spec.md Known defects #) | Description | Scheduled | Fixed by |
|---|---|---|---|
| 1 | Group filter false-empties during membership load or on failure (audit finding 7) | Phase 1 (audit findings), frontend fixes bucket | **Fixed**, commit `4ab54bf`: `inGroupCount` and the list body distinguish `membershipMap === null` (loading/error) from a genuine empty match, rendering the shared `AsyncPanel` presentation instead of "No repositories match this filter". |
| 2 | BL-NI-22 (O(N) group filter fan-out) | Tracked as its own backlog entry, `docs/backlog.md` BL-NI-22; landed as Phase 1 / P1-E per `execution-plan.md` | **Resolved**, commit `261a689`: a single bulk `repo_group_memberships() -> Vec<RepoGroupMembership>` replaces the `groups_for_repo` fan-out for both the filter and the chips - the originally-proposed `repos_in_group(group_id)` was superseded, since it would have served only the filter, not the chips. |
| 3 | Group dialog double-Enter double-submit | Phase 1 (audit findings), frontend fixes bucket | **Fixed**, commit `4ab54bf`: `submit()` returns immediately if `busy` is already `true`. |
| 4 | Duplicate-name error surfaces as "invalid setting: name" with the wrong (Settings-screen) remediation | Phase 1 (audit findings); the underlying `AppError::InvalidSetting` message itself is E-05 (error taxonomy)'s to change | **Fixed at the UI layer**, commit `4ab54bf`: the dialog now detects the duplicate-name case by error code + field and toasts "That name is already taken." The raw `AppError::InvalidSetting` display/remediation text is unchanged for any other caller; a generic or per-field remediation override remains open, owned by E-05. |
| 5 | Rename/delete buttons invisible while keyboard-focused (audit finding 12) | Phase 1 (audit findings), the a11y batch | **Fixed**, commit `4ab54bf`: `group-focus-within/row:opacity-100` added alongside the hover reveal, plus a visible `focus-visible` ring on the row-icon buttons. |
| 6 | Delete-active-group force-navigates to Repos | Phase 1 (audit findings), frontend fixes bucket | **Fixed**, commit `4ab54bf`: a new `onClearActiveGroup` prop (`app-shell.tsx`'s `clearActiveGroup`) clears `activeGroupId` with no `setView` side effect; `doDelete` uses it instead of `onSelectGroup` when the deleted group was the active filter. |

## Definition of done (for the remaining work, not the shipped vertical)

- All six Known defects above are fixed, with the fix noted in `spec.md`'s Task Summary and each defect entry - **done 2026-07-05**.
- Defect 2 (BL-NI-22) is fixed; the AC9 source citation and the "Out of scope" / V1.1-extension-points lines calling out the missing `repos_in_group` query in `spec.md` were updated in the same pass to say it is superseded by the bulk `repo_group_memberships` read - **done 2026-07-05**.
- No regression in the five existing store-layer unit tests; the fix added a sixth (`bulk_repo_group_memberships_groups_by_repo` in `store.rs`) alongside them. Not independently re-run by this doc-sync pass (docs-only scope; verified by reading the test code, not by executing `cargo test`).
