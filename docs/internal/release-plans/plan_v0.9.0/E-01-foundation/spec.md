---
effort: E-01
tracking-issue: 3
title: Foundation, Workspace, and CI
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: []
source: docs/internal/v1-architecture-and-decisions.md (Sections 4.3, 6)
---

# E-01 - Foundation, Workspace, and CI

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** complete and committed on branch `build/e-01-foundation`. Workspace + `reposync-core` (Tauri-free stubs) + `src-tauri` shell (real icons, downloadBootstrapper WebView2) + frontend + repo hygiene + Windows/macOS CI are all in place. Local gate GREEN on Rust 1.96.0 stable: `cargo check`, `clippy --all-targets -- -D warnings`, `cargo test`, `cargo fmt --check`, and the `cargo tree -p reposync-core` no-tauri hygiene gate all pass; frontend `pnpm install`/`typecheck`/`lint`/`build` pass. A Codex adversarial review ran; its one CI blocker (a macOS git-pin assert that would fail the macOS leg) is fixed (exact git pin now scoped to the Windows runner per E-04), and two non-blocking findings are filed in `docs/backlog.md` (BL-NI-01, BL-NI-02). AC1, AC2, AC3, AC5, AC6, AC7 closed; AC8 static-verified.
- **Next:** push `build/e-01-foundation` to run the Windows+macOS CI matrix and close AC4 (the only remaining criterion). Then start the week-1 tracer (minimal E-02 + E-03 + E-12).
- **Blockers:** none for local work. AC4 (dual-OS CI bundle green) requires a push, which is human-gated per `EXECUTION.md`.

## Context

RepoSync is a single-process Tauri v2 desktop app split into a logic crate with **zero Tauri dependencies** (`reposync-core`) and a thin Tauri shell (`src-tauri`). This effort stands up that workspace, the module skeleton, the open-source repo hygiene, and the CI matrix, so every later effort has a place to land and a green-by-default pipeline. It writes **no business logic**: modules are stubs, the schema is empty (E-02 owns it), the IPC types are empty (E-06 owns them). The deliverable is a compiling, testing, CI-green skeleton.

The load-bearing constraint, set here and enforced forever: `reposync-core` must not pull `tauri` even transitively. This is what keeps the product logic headlessly testable and makes the macOS port a thin edge.

Platform framing for the CI gate: Windows is the real GA bar; macOS is "compiles + bundles only" in CI until real Mac access exists (per the ratified platform decision and `EXECUTION.md`). This framing is context for why both runners are in the matrix; it is not itself a pass/fail criterion. The CI acceptance criterion (AC4) is simply that both runners are green.

## In scope

- Root `Cargo.toml` workspace declaring `members = ["crates/reposync-core", "src-tauri"]`.
- `crates/reposync-core` with `lib.rs` and stub modules: `error.rs`, `ipc.rs`, `repo.rs`, `policy.rs`, `scheduler.rs`, `activity.rs`, `summary.rs`, `github.rs`, `paths.rs`, and `git/{mod.rs, cli.rs, inspect.rs}`. An empty `migrations/` directory.
- `src-tauri` shell skeleton: `main.rs` (builder + managed-state placeholders), `commands/`, `events.rs`, `tray.rs`, `windows/` (empty placeholders), `tauri.conf.json` with the WebView2 `downloadBootstrapper` strategy and a <30MB bundle posture.
- Frontend skeleton sufficient to `pnpm typecheck`/`pnpm lint` (Vite + React + TypeScript + Tailwind, plus shadcn **initialized only, zero components**); no real screens. shadcn is set up (config + theme/CSS variables) but no components are added, since no E-01 acceptance criterion exercises a shadcn component and the skeleton must not over-pull component dependencies.
- Repo hygiene: in-repo `.gitignore` listing `_local/` (NOT `docs/internal/`, which is tracked), `LICENSE` (MIT), `.github/` templates (bug report, feature request, PR template, `FUNDING.yml`), `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
- CI workflow: Windows + macOS matrix running `cargo check`, `cargo clippy --all -- -D warnings`, `cargo test`, `pnpm typecheck`, `pnpm lint`; the `cargo tree -p reposync-core | grep -i tauri` dependency-hygiene gate; pinned `git` for the fixture harness; build + bundle on both runners.

## Out of scope

- The real SQLite schema and migration runner (E-02).
- Real `AppError` variants (E-05) and real IPC payload types (E-06).
- Any git, scheduler, policy, activity, summary, or GitHub logic (E-03, E-07, E-08, E-09, E-10, E-11).
- Code signing and Apple/Windows certificate work (human-only per `EXECUTION.md`).

## Contract / deliverables

1. `cargo build` and `cargo test` succeed at the workspace root on Windows.
2. `pnpm typecheck` and `pnpm lint` succeed.
3. The CI workflow is green on both Windows and macOS runners (build + bundle).
4. The dependency-hygiene gate proves `reposync-core` has no `tauri` in its tree.
5. `LICENSE`, `.gitignore`, and `.github/` templates are present; `_local/` is ignored and `docs/internal/` is not.

## Acceptance criteria

- [ ] AC1: Workspace layout matches the In-scope file checklist exactly (which is the brief Section 4.3 layout made concrete here). Specifically, all of the following exist: root `Cargo.toml` with `members = ["crates/reposync-core", "src-tauri"]`; `crates/reposync-core/src/lib.rs` plus the stub modules `error.rs`, `ipc.rs`, `repo.rs`, `policy.rs`, `scheduler.rs`, `activity.rs`, `summary.rs`, `github.rs`, `paths.rs`, and `git/{mod.rs, cli.rs, inspect.rs}`; an empty `crates/reposync-core/migrations/` directory; and the `src-tauri` shell (`main.rs`, `commands/`, `events.rs`, `tray.rs`, `windows/`, `tauri.conf.json`). The crate split (`reposync-core` + `src-tauri`) and module names match this list with no additions or omissions. Source: brief Section 4.3; In-scope file checklist above.
- [ ] AC2: `reposync-core` compiles with no `tauri`/`tauri-*` dependency; CI asserts an empty `cargo tree -p reposync-core | grep -i tauri`. Source: brief Section 4.3 ("dependency hygiene").
- [ ] AC3: `tauri.conf.json` selects the evergreen WebView2 `downloadBootstrapper`. Source: brief Section 4.10a.
- [ ] AC4: The CI matrix is green - both the Windows and the macOS runner build and bundle successfully (pass/fail gate). Source: brief Section 6 (CI workstream) and `EXECUTION.md`. (Context for this gate: Windows is the real GA bar and macOS is compiles-and-bundles-only until real Mac access; see the Context section. That framing does not change the gate, which is simply both runners green.)
- [ ] AC5: `.gitignore` ignores `_local/` and does NOT ignore `docs/internal/`; `LICENSE` is MIT. Source: ratified gitignore decision (2026-06-19) recorded in `docs/internal/release-plans/plan_v0.9.0/README.md` ("Ratified decisions this plan assumes" table, Gitignore row), which overrides the brief Section 6 repo-hygiene wording about quarantining `docs/internal/`.
- [ ] AC6: A throwaway/no-op `cargo test` exists in `reposync-core` so the test gate is exercised from day one.
- [ ] AC7: `Cargo.toml` pins EXACT versions (no `^`/`~`/range, e.g. `=2.0.0`) for every `tauri*` crate and for `specta` and `tauri-specta`, because the brief Section 4.4 caveat flags `tauri-specta` v2 as a release candidate; pinning prevents a silent RC bump from breaking codegen. Source: brief Section 4.4.
- [ ] AC8 (rescoped 2026-06-20): The CI workflow verifies the `git` floor (>= 2.30, the E-03 engine requirement) on both runners. The exact byte-stable `git` pin the E-04 fixture harness will need is DEFERRED to E-04: the harness does not exist yet, and pinning an exact `git` on the hosted runners proved unreliable in practice (choco's installer conflicts with the preinstalled git; portable MinGit's PATH ordering did not shadow it). `GIT_VERSION` in the workflow records the intended exact target; the robust exact-pin mechanism is tracked in `docs/backlog.md` (BL-NI-03) and owned by E-04. Source: brief Section 6 (CI workstream); E-04 spec. NOTE: this is a deliberate, jp-flagged narrowing of the original "pin the exact version" wording.

## Dependencies

- Upstream: none.
- Downstream: every other effort (E-02 through E-12) lands inside this skeleton.

## V1.1 extension points

- The CI matrix gains a real macOS signing/notarization job once Apple credentials exist (human-only).
- A Linux WebKitGTK smoke target may be added later as a cheap WKWebView-divergence canary (brief Section 4.2).

## Open questions

- License pick is MIT by default but is jp's call and binds only at first public commit. Use MIT now; flag at go-public.
