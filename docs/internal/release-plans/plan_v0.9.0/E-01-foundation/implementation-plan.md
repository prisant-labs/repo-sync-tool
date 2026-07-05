---
effort: E-01
plan_for: spec.md
status: ready
---

# E-01 Implementation Plan

## Approach

Build the skeleton bottom-up: workspace first, then the Tauri-free core crate, then the shell, then the frontend, then hygiene, then CI last so it gates a thing that already compiles. Keep every module a compiling stub. The goal is a green pipeline and a correct shape, not behavior.

## Steps

1. **Workspace root.** Create root `Cargo.toml` with `[workspace] members = ["crates/reposync-core", "src-tauri"]` and a shared `[workspace.dependencies]` table for pinned versions (serde, thiserror, tokio, sqlx, git2, specta). Pin all Tauri-related crates exactly (brief Section 4.4 caveat).
2. **Core crate skeleton.** `cargo new --lib crates/reposync-core`. Add `lib.rs` declaring the modules from Section 4.3. Create each module file with a doc comment naming its owning effort and a `// TODO(E-0x)` marker. Create `git/` with `mod.rs` (empty `GitEngine` trait placeholder), `cli.rs`, `inspect.rs`. Create empty `migrations/`.
3. **Core has no Tauri.** Confirm `reposync-core/Cargo.toml` lists none of `tauri`/`tauri-*`. Add a trivial `#[test] fn skeleton_compiles() {}` so the test gate runs (AC6).
4. **Tauri shell skeleton.** Scaffold `src-tauri` (via `pnpm create tauri-app` output, trimmed) depending on `reposync-core`. Stub `main.rs` builder, empty `commands/`, `events.rs`, `tray.rs`, `windows/`. Set `tauri.conf.json`: `downloadBootstrapper` WebView2 strategy (AC3), bundle targets MSI/NSIS (Windows) and app/dmg (macOS), identifier and product name behind a single brand constant.
5. **Frontend skeleton.** Vite + React + TS + Tailwind + shadcn init, just enough that `pnpm typecheck` and `pnpm lint` pass. No screens; a single placeholder component is fine.
6. **Repo hygiene.** Write in-repo `.gitignore` (`_local/`, build artifacts, `node_modules/`, `target/`; explicitly NOT `docs/internal/`). Add `LICENSE` (MIT), `.github/` templates, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`.
7. **CI.** GitHub Actions workflow, `windows-latest` + `macos-latest` matrix: setup Rust + pnpm, pin `git`, run `cargo check`/`clippy -D warnings`/`cargo test`/`pnpm typecheck`/`pnpm lint`, run the `cargo tree -p reposync-core` hygiene gate (AC2), then `tauri build` (bundle) on each runner.
8. **Verify.** Run the full local gate on Windows; push the branch; confirm the matrix is green on both runners.

## Test strategy

- The only test here is the compile/lint/bundle pipeline itself plus the no-op core test. There is no behavior to unit-test yet.
- The dependency-hygiene gate is the most valuable assertion in this effort: it fails the build the instant anything drags `tauri` into core.

## Files / modules touched

- `Cargo.toml`, `crates/reposync-core/**`, `src-tauri/**`, frontend root (`package.json`, `vite.config.ts`, `src/**`), `.gitignore`, `LICENSE`, `.github/**`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, `.github/workflows/ci.yml`.

## Risks and mitigations

- **`libgit2-sys` / native build friction on Windows CI.** Mitigated downstream in E-03 (vendored, no-OpenSSL); for E-01, just adding `git2` as a dependency and getting a clean `cargo build` surfaces it early.
- **`tauri-specta` v2 is a release candidate.** Pin exact versions now; the actual codegen is E-06. Here, only lock the versions.
- **macOS bundle fails in CI for a non-code reason** (signing config). Keep macOS bundling unsigned; signing is human-only and a later job.

## Definition of done

All six acceptance criteria checked, CI green on both runners, and the branch ready for self-merge per the visibility-tiered policy in `EXECUTION.md`.
