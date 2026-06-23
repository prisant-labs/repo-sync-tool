---
effort: E-12
plan_for: spec.md
status: ready
---

# E-12 Implementation Plan

## Approach

Drive the tracer in the order data flows, one command at a time, so each layer is proven before the next is added: get `repo_add_path` green end to end (UI -> command -> core -> git2 -> SQLite) first, then add `repo_check_now` (command -> policy + CLI fetch -> DB update -> emitted event -> UI listener). Use the thinnest slice of E-02, E-03, and E-06 that makes the round trip *real* - real git, real SQLite, a real emitted event - and resist deepening any of them; breadth is other efforts' jobs. Treat the debug UI as disposable from the first line. Then run the whole thing in a real Windows build and get CI bundling on both runners, because the cross-platform packaging answer is half the value of doing this in week 1. Keep `reposync-core` Tauri-free throughout; only the `src-tauri` command/event shells touch Tauri.

## Steps

1. **Confirm the thin slices exist.** Verify E-02 gives a migrated DB + `SqlitePool`, E-03 gives a `git2` inspect read and a CLI `fetch`/`rev-list`, and E-06 gives frozen `tauri-specta` types for `repo_add_path`, `repo_check_now`, and the `repo:check-completed` event. If any is not yet landed, use the minimal local slice and flag the dependency.
2. **`repo_add_path` core path.** In `reposync-core`, implement the minimal `repo::add(path)`: run the `git2` inspect (HEAD SHA, branch, dirty, detached), `INSERT` a `repos` row (`local_path`, `remote_origin_url`, `host_type`, defaults) and an initial `repo_local_state` row, return a `RepoId`. Pure-ish over the injected pool; no Tauri.
3. **`repo_add_path` command shell.** In `src-tauri/src/commands/`, add the thin `#[tauri::command]` wrapper that calls the core function and returns the E-06 `RepoId` type. Register it in the builder. No logic in the shell.
4. **Throwaway debug UI - add.** A single disposable component: a button that invokes `repo_add_path` (through the generated `commands.*` binding, not raw `invoke`) against a hardcoded/test path, and a `<pre>` that dumps the returned JSON. Mark the file clearly as throwaway (a banner comment) so it is obvious it gets deleted.
5. **`repo_check_now` core path.** Implement the minimal check: acquire the work, run the CLI `fetch` (E-03 `cli.rs`, capturing `raw_command`/stdout/stderr/exit/duration), run `rev-list --left-right --count` to get ahead/behind, apply a **minimal inline policy decision** named `tracer-inline-policy` (if `behind_count > 0` and the tree is clean and not detached, report `would-fast-forward`; otherwise report `skip-with-reason`; replaced by E-07 when E-07 lands), `UPDATE repo_local_state`, then write the activity row. The direct `INSERT` of an `activity_records` row from the captured fields is the **week-1 thin-slice stand-in** for the E-09 activity writer (E-09 is the future owner; swapped to the E-09 `record(...)` writer when E-09 lands). Return a `CheckResult`.
6. **Emit the event.** From the command layer (the `AppHandle` lives in `src-tauri`, so core returns the data and the shell emits), emit `repo:check-completed` with the E-06 typed payload after the check completes. Core stays Tauri-free; the emit helper is in `src-tauri/src/events.rs`.
7. **Throwaway debug UI - check + listen.** Add a button invoking `repo_check_now` and a `listen` on `repo:check-completed` (through the generated `events.*` binding) that appends the payload to the `<pre>`. This closes the round trip visibly.
8. **Real Windows build.** Run `pnpm tauri dev` and a real `tauri build` on Windows; click both buttons; confirm the JSON dump and the event payload appear, the `repos`/`repo_local_state`/`activity_records` rows exist in the SQLite file, and the fetch actually hit the network (AC1, AC2, AC4).
9. **CI: macOS bundle green.** Confirm the same source compiles and bundles on the macOS runner (compiles + bundles only, no human-validated clause), keeping the macOS target honest from week 1 (AC4).
10. **Packaging spike - Windows.** Configure the Tauri bundler for MSI/NSIS, **user-mode (per-user) install**, WebView2 `downloadBootstrapper`. Produce the artifact from CI. Sign it if the human-only certificate exists; otherwise produce an unsigned artifact and document the signing step (AC6).
11. **Packaging spike - macOS docs.** Write the macOS signing/notarization runbook: `codesign` -> `xcrun notarytool` -> `stapler`, run on a macOS CI runner holding Apple credentials as secrets, with Apple Developer enrollment and secret storage flagged **human-only per `EXECUTION.md`**. This is documentation only; it cannot be exercised on Windows hardware (AC7).
12. **Verify and mark disposable.** Confirm all seven acceptance criteria; leave a clear note that the debug UI and any seed path are throwaway and to be removed when real screens land.

## Test strategy

- **End-to-end manual on Windows.** The tracer's primary proof is a real `tauri build` run on Windows: click `repo_add_path`, see the JSON; click `repo_check_now`, see the `repo:check-completed` payload printed by the listener. This is the integration test the effort exists to perform.
- **DB assertions.** After the round trip, assert the `repos` row, the `repo_local_state` row (updated ahead/behind/dirty/detached, `last_checked_at`), and an `activity_records` row (with captured `raw_command`/stdout/stderr/exit/duration) exist in the SQLite file.
- **Core unit slices.** The `repo::add` and the minimal check decision are testable in plain `cargo test` against a tempdir SQLite and a fixture repo (reuse E-03/E-04 fixtures if available), headless, no Tauri.
- **Typed-binding check.** Confirm the UI calls the generated `commands.*`/`events.*` bindings, not raw `invoke`/`listen`, so a contract change would break the TS build (the seam guarantee).
- **CI matrix.** Windows builds + bundles + (where feasible) runs the smoke; macOS compiles + bundles green. The bundle step itself is the packaging-spike test.
- **No deep unit suites here.** Exhaustive policy/scheduler/parser tests belong to E-07/E-08/E-03; this effort proves wiring, not breadth.

## Files / modules touched

- `crates/reposync-core/src/repo.rs` (minimal `repo::add`) and a minimal check path (in `repo.rs` or a small tracer module) - thin, replaced/extended by later efforts.
- `src-tauri/src/commands/` (thin `repo_add_path` + `repo_check_now` wrappers), `src-tauri/src/events.rs` (the `repo:check-completed` emit helper), `src-tauri/src/main.rs` (register the two commands, managed pool state).
- Frontend: one throwaway debug component (button + `<pre>`) wired to the generated bindings; clearly marked disposable.
- `src-tauri/tauri.conf.json` (bundle config: MSI/NSIS, user-mode install, `downloadBootstrapper`).
- `.github/workflows/` (the build+bundle matrix already from E-01; this effort confirms the Windows artifact and the green macOS bundle).
- A short macOS signing/notarization runbook doc (e.g. under `docs/internal/` or the effort folder) - documentation only.

## Risks and mitigations

- **Native build / WebView2 / capability friction on the first real Windows build.** This is exactly the risk the tracer exists to surface early. Mitigate by doing it in week 1 while the codebase is tiny; a failure here is cheap to fix now and expensive in week 6.
- **macOS bundle fails in CI for a non-code reason (signing config).** Keep the macOS bundle unsigned and downgrade CI from "accepted" to "not broken"; signing is human-only and a later job. Per `EXECUTION.md`, macOS is compiles + bundles only.
- **Scope creep into real logic.** The temptation is to build the real policy engine or activity writer here. Mitigate by hard-scoping to a minimal inline check decision and a single `activity_records` INSERT, and swapping in E-07/E-09 only once they land.
- **The throwaway UI quietly becoming load-bearing.** Mitigate with an explicit disposable banner in the component and a definition-of-done note that it is deleted when real screens arrive; it must pre-commit no UI/UX decision.
- **Human-only packaging blockers (certs, Apple enrollment).** These cannot be done by the agent. Mitigate by producing an unsigned artifact + a documented signing path now, and flagging the human-only steps clearly rather than stalling the spike.
- **Dependency thin-slice not ready (E-06 types, E-03 fetch).** Mitigate by using a minimal local slice and flagging the dependency; the tracer can prove the pattern against a thin real slice and tighten once the full effort lands.

## Definition of done

All seven acceptance criteria checked: both commands run end to end on a real Windows build through the frozen typed bindings, the event round-trips to the listener, the SQLite rows are written, a Windows MSI/NSIS artifact is produced from CI (signed-or-documented, user-mode, `downloadBootstrapper`), the macOS bundle is green in CI, and the macOS signing/notarization path is documented as a human-only follow-up. `reposync-core` still has no `tauri` in its dependency tree, the debug UI is marked disposable, and the branch is ready for self-merge per `EXECUTION.md`.
