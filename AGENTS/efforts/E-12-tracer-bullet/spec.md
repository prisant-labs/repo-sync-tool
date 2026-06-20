---
effort: E-12
title: Tracer Bullet and Packaging Spike
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-02, E-03, E-06]
source: docs/internal/v1-architecture-and-decisions.md (Sections 6, 4.1, 4.4)
---

# E-12 - Tracer Bullet and Packaging Spike

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started.
- **Next:** wire `repo_add_path` end to end (throwaway UI -> Tauri command -> core -> git2 -> SQLite) on a real Windows build, then `repo_check_now` through to an emitted event.
- **Blockers:** none beyond E-02 (schema + pool), E-03 (git2 read + CLI fetch), and E-06 (the frozen IPC types for the two commands and the event).

## Context

This is the effort that pierces the whole architecture once, while the codebase is tiny and cheap to change. It is pulled forward to **week 1** by design: the riskiest cross-platform unknowns - does this actually build, bundle, and run on Windows, and does the same source bundle in CI on macOS - get early answers instead of surfacing in week 6 when packaging is on the critical path to GA.

It delivers two things in one effort. **(1) The tracer bullet:** `repo_add_path` + `repo_check_now` end to end, exercising every layer exactly once - a deliberately throwaway debug UI (a button plus a `<pre>` JSON dump) -> a Tauri command -> `reposync-core` `repo::add` + git inspect -> a real filesystem repo via `git2` -> a SQLite `INSERT`; then `repo_check_now` -> core policy + git CLI fetch -> `UPDATE repo_local_state` + `activity_records` -> emit `repo:check-completed` -> the UI listener prints it. All of it inside a **real Windows build**, with the macOS bundle green in CI. **(2) The packaging spike:** a signed-or-documented Windows artifact produced early from CI (MSI/NSIS via the Tauri bundler, user-mode install, WebView2 `downloadBootstrapper`), plus the macOS signing/notarization path documented even though it cannot be exercised on Windows hardware.

The durable output is the **proven pattern and the frozen IPC contract exercised end to end** - not the UI. The button and the `<pre>` dump are explicitly disposable scaffolding; they exist to prove the round trip and to give the bundler something to package, and they cannot pre-commit any UI/UX decision. Once the contract pattern is proven on these two commands, every later command is a variation on a proven pattern rather than a fresh integration risk, and the backend breadth (policy, scheduler, parsers, activity, GitHub) can be built out fast and in parallel.

This effort assembles thin slices of E-02, E-03, and E-06; it does not deepen them. It uses the minimum git reads/fetch from E-03, the schema + pool from E-02, and the frozen types for two commands and one event from E-06. The Apple Developer enrollment and any secret storage are **human-only per `EXECUTION.md`**; this effort documents the macOS path, it does not execute it.

## In scope

- A deliberately **throwaway** debug UI: a button that invokes `repo_add_path`, a button that invokes `repo_check_now`, and a `<pre>` that dumps the returned JSON and the received event payload. Explicitly disposable; built to be deleted once breadth begins.
- `repo_add_path(path) -> RepoId` end to end: Tauri command -> `reposync-core` `repo::add` + git inspect (`git2` read of HEAD/branch/dirty/detached) -> SQLite `INSERT` into `repos` (and the initial `repo_local_state` row).
- `repo_check_now(id) -> CheckResult` end to end: Tauri command -> core policy + git **CLI** fetch (via E-03's `cli.rs`) and `rev-list` -> `UPDATE repo_local_state` + an `activity_records` row -> emit `repo:check-completed` -> the UI listener receives and prints it. The "policy" here is a **minimal inline decision** named `tracer-inline-policy` (if `behind_count > 0` and the tree is clean and not detached, report `would-fast-forward`; otherwise report `skip-with-reason`), replaced by E-07 when E-07 lands. The direct `activity_records` INSERT is the **week-1 thin-slice stand-in** for the E-09 activity writer (E-09 is the future owner), swapped to the E-09 `record(...)` writer when E-09 lands.
- The two commands invoked through the **frozen E-06 IPC types** (`tauri-specta`-generated bindings), not raw `invoke`, so the round trip exercises the real typed seam.
- A **real Windows build** that runs the round trip, with the macOS bundle building green in CI (compiles + bundles only, no human-validated clause).
- Packaging spike: produce a Windows artifact from CI early - **MSI/NSIS via the Tauri bundler, user-mode (per-user) install, WebView2 `downloadBootstrapper`** - and either sign it or document the signing path.
- A written macOS signing/notarization path (`codesign` -> `xcrun notarytool` -> `stapler`, the CI-runner-holds-Apple-secrets model), documenting what a human must unblock, even though it cannot be exercised on Windows hardware.

## Out of scope

- Any business logic beyond the minimum the two commands need: the full policy engine (E-07), the scheduler (E-08), the activity-writer retention sweep (E-09), the GitHub client (E-10), and the summary engine (E-11) are all out; the tracer uses the thinnest slice that makes the round trip real.
- The full command/event surface (E-06 owns the whole contract; this effort exercises only `repo_add_path`, `repo_check_now`, and `repo:check-completed`).
- Deepening E-02 or E-03 (schema breadth, the fixture harness, the git2/CLI cross-check, all 7 states); this effort consumes their thin slices, it does not extend them.
- Executing Apple Developer enrollment or storing signing/notarization secrets - **human-only per `EXECUTION.md`**; this effort only documents the path.
- Any real UI/UX: the debug UI is scaffolding to be deleted and must not be treated as a screen design.
- The actual `repo_scan_parent` / `repo_update_now` / metadata commands; only the two tracer commands are wired here.

## Contract / deliverables

1. `repo_add_path` runs end to end on a real Windows build: throwaway UI -> Tauri command -> core `repo::add` + `git2` inspect -> SQLite `INSERT`, returning a `RepoId`.
2. `repo_check_now` runs end to end: command -> core policy + CLI fetch + `rev-list` -> `UPDATE repo_local_state` + `activity_records` INSERT -> emit `repo:check-completed`, and the UI listener prints the payload.
3. Both commands go through the frozen E-06 `tauri-specta` typed bindings, proving the seam pattern, not raw `invoke`/`listen`.
4. The throwaway UI is a button-plus-`<pre>` dump, clearly marked disposable, exercising the round trip with no UI/UX commitment.
5. A Windows MSI/NSIS artifact is produced from CI (user-mode install, `downloadBootstrapper`); it is signed or the signing path is documented.
6. The macOS bundle builds green in CI (compiles + bundles), and the macOS signing/notarization path is documented as a human-only follow-up.

## Acceptance criteria

- [ ] AC1: `repo_add_path` pierces every layer once - throwaway UI -> Tauri command -> `reposync-core` repo::add + git inspect -> filesystem repo via `git2` -> SQLite `INSERT`. Source: brief Section 6 (tracer-bullet recommendation, the mermaid flow) and Section 4.4 (`repo_add_path`).
- [ ] AC2: `repo_check_now` pierces every layer once - command -> core policy + git CLI fetch -> `UPDATE repo_local_state` + `activity_records` -> emit `repo:check-completed` -> the UI listener prints it. Source: brief Section 6 (tracer-bullet recommendation) and Section 4.4 (`repo_check_now`).
- [ ] AC3: Both commands are invoked through the frozen `tauri-specta`-generated typed bindings (E-06), exercising the real IPC seam rather than raw `invoke`. Source: brief Section 6 ("validates the seam before breadth") and Section 4.4 (IPC as the API).
- [ ] AC4: The round trip runs inside a **real Windows build** with the macOS bundle building green in CI (compiles + bundles only). Source: brief Section 2 (per-platform acceptance criteria) and Section 6 (CI/packaging workstreams; "inside a real Windows build... with the macOS bundle running in CI").
- [ ] AC5: The debug UI is a button plus a `<pre>` JSON dump, explicitly disposable, and pre-commits no UI/UX decision. Source: brief Section 6 ("The UI here is a debug button and a `<pre>` dumping JSON. It is meant to be deleted.").
- [ ] AC6: The packaging spike produces a Windows MSI/NSIS artifact from CI (user-mode install, WebView2 `downloadBootstrapper`), signed-or-documented. Source: brief Section 6 (packaging-spike workstream) and Section 4.10a (`downloadBootstrapper`).
- [ ] AC7: The macOS signing/notarization path is documented (cannot be exercised on Windows hardware); Apple enrollment and secret storage are flagged human-only. Source: brief Section 6 (packaging spike) and `EXECUTION.md` (human-only allowlist).

## Dependencies

- Upstream: E-02 (schema + `SqlitePool`), E-03 (minimal `git2` read + CLI fetch), E-06 (frozen IPC types for the two commands and the event).
- Downstream: every later command effort inherits the proven pattern this establishes; the packaging spike de-risks the GA packaging path used at ship.

## V1.1 extension points

- The throwaway debug UI is deleted once real screens land; nothing downstream depends on it.
- The macOS signing/notarization job becomes real once a human unblocks Apple enrollment + Mac access + CI secrets (human-only); the documented path is the runbook for that job.
- The Windows signing step (Authenticode / Azure Trusted Signing) is wired into the packaging job once the certificate is procured (human-only, money + identity).

## Open questions

- Exact MSI vs NSIS choice for the V1 Windows installer: default to whichever the Tauri bundler produces most reliably user-mode with `downloadBootstrapper`; flag the final pick to jp at the packaging spike since it affects the install experience.
- Whether `repo_check_now`'s "policy" slice should call the real E-07 policy engine (if available) or a minimal inline decision for the tracer; default to the minimal inline decision so this effort stays a thin slice, and swap to E-07 once it lands. Flag if E-07 is ready early.
- Whether the Windows artifact is signed in this spike or only documented depends on whether the human-only certificate procurement has happened; default to documenting the path and producing an unsigned artifact, and sign once the cert exists.
