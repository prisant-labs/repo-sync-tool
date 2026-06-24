---
effort: E-03
tracking-issue: 5
title: Git Engine
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-01]
source: docs/internal/v1-architecture-and-decisions.md (Section 4 / Architecture subsections 6 "Git engine" and 10d "Git executable discovery")
---

# E-03 - Git Engine

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** complete (pending orchestrator gate/review/commit). All 11 ACs implemented test-first. The full `GitEngine` trait (read seam + CLI ops), the five pure parsers (`fetch` classification, `rev-parse`, `status --porcelain=v2`, `for-each-ref`, `rev-list --left-right --count`), git2 reads incl. ahead/behind, discovery (explicit -> PATH -> well-known Windows), version probing with the >= 2.30 floor, the first-class `GitAvailability` state + `reprobe()`, infallible degraded construction (`SystemGitEngine::new`), and the internal `FetchClass` enum (BL-NI-05) wired into `repo.rs::check_now`. 67 reposync-core tests pass; clippy `-D warnings` clean; dependency-hygiene gate empty; git2 stays `vendored-libgit2` with no network transports. The frozen IPC contract and AppError wire shape are untouched.
- **Next:** orchestrator gate + review + commit. Downstream: E-04 ratifies the AC11 `None` ahead/behind contract via the git2-vs-CLI cross-check; E-07 consumes `FetchClass` for pause-vs-retry; E-08 drives `reprobe()` for auto-resume.
- **Blockers:** none for E-03 logic. NOTE: `cargo fmt --all -- --check` is red on PRE-EXISTING drift (verified red at HEAD 4fd0d55 across `db.rs`, `error.rs`, `ipc.rs`, `src-tauri/**`); E-03's own files are rustfmt-clean. Flagged as a follow-up rather than reformatting other efforts' files in this change.

## Context

RepoSync's git layer obeys one boundary rule, memorized in the brief: **if an operation hits the network or writes the working tree, it goes through the `git` CLI; if it is a cheap local read, it goes through `git2`.** This effort builds both sides of that hybrid and the trait that hides which side is in play.

The CLI side (`git/cli.rs`) shells out via `tokio::process::Command` and captures `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, and `duration_ms` for every invocation, because those captures are the audit trail E-09 writes into `activity_records`. The read side (`git/inspect.rs`) uses `git2` for HEAD SHA, active branch, dirty status, detached state, and ahead/behind counts, where a process spawn plus output parsing would be needless cost and fragility. Both sit behind a `GitEngine` trait in `git/mod.rs` so the `git2` read path - which is purely a performance optimization - can be abandoned for an all-CLI fallback as a localized change if `libgit2-sys` ever fights the Windows toolchain.

This effort also owns git discovery and the "git unavailable" first-class state. Windows frequently ships with no `git` at all, so a missing binary is a normal state with an actionable banner, not a crash. The parsers are pure functions over captured output; that is what makes them, and everything downstream (E-04, E-07), deterministically testable.

## In scope

- The `GitEngine` trait in `crates/reposync-core/src/git/mod.rs` abstracting both the network/mutation operations and the cheap reads, so an all-CLI fallback impl is a localized change.
- `git/cli.rs`: `tokio::process::Command` execution that captures `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, and `duration_ms` for every invocation, plus **pure parsers** for `fetch`, `rev-parse`, `status --porcelain=v2`, `for-each-ref`, and `rev-list --left-right --count` over captured output.
- `git/inspect.rs`: `git2`-backed reads for HEAD SHA, active branch, dirty status, detached state, and ahead/behind counts. Never touches the network, never mutates.
- `git2` / `libgit2-sys` pinned with the `vendored` feature and network transports disabled (**no OpenSSL, no libssh2**), since all network traffic goes through the CLI.
- Git executable discovery in the order: explicit `settings.git_executable_path`, then `PATH`, then well-known Windows install locations (`%ProgramFiles%\Git\cmd\git.exe`, Scoop/winget shims); resolved path cached.
- Version probing via `git --version` with a **minimum floor of git >= 2.30**; below-floor produces a clear, non-blocking warning.
- A "git not found" / "git unavailable" first-class state surfaced as data, so the shell can render an actionable banner and put repos into a distinct state; scheduled checks pause and auto-resume on detection.
- Degraded-state initialization: the `GitEngine` and its discovery must construct SUCCESSFULLY when no `git` is present, returning a usable engine pinned to the "git unavailable" state rather than an `Err`. App launch is never gated on git discovery (brief Section 4 / Architecture subsection 10d: "the app must launch and remain usable for browsing existing state even with no git").

## Out of scope

- The `activity_records` writer and retention sweep that consume the captured `raw_*`/`duration_ms` fields (E-09).
- The fixture harness and the git2-vs-CLI cross-check that exercises both engines against the 7 states (E-04).
- The update-policy engine that decides what action a repo state implies (E-07).
- The scheduler that drives checks, per-repo mutex, and pause/resume (E-08).
- The `AppError` variants that the engine's failures map to (E-05); this effort returns engine-level results that E-05 later wraps.
- Rendering the git-not-found banner or the "git unavailable" repo state (UI surface, out of these efforts).

## Contract / deliverables

1. A `GitEngine` trait in `git/mod.rs` covering network/mutation operations (CLI-backed) and cheap reads (git2-backed), with a clean seam for a future all-CLI read impl.
2. `git/cli.rs` runs `git` via `tokio::process::Command`, capturing `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms` on every call.
3. Pure parser functions for `fetch`, `rev-parse`, `status --porcelain=v2`, `for-each-ref`, and `rev-list --left-right --count`, each taking captured output and returning typed results with no I/O.
4. `git/inspect.rs` returns HEAD SHA, branch, dirty status, detached state, and ahead/behind counts via `git2`.
5. `git2`/`libgit2-sys` build with `vendored` and no network transports; `cargo build` on Windows pulls no OpenSSL.
6. Discovery resolves a usable `git` by the three-step order and caches it; version probing enforces the 2.30 floor.
7. "git not found" is representable as a distinct state value (not a generic error), and discovery exposes a re-probe path for auto-resume.
8. Engine construction succeeds in the degraded ("git unavailable") state when no `git` is present, so app launch is never gated on git discovery.

## Acceptance criteria

- [x] AC1: Network/mutation operations route through `git/cli.rs` (CLI) and cheap reads route through `git/inspect.rs` (git2), both behind the `GitEngine` trait in `git/mod.rs`. Source: brief Section 4 / Architecture subsection 6 (Git engine: the boundary rule and module split).
- [x] AC2: `git/cli.rs` captures `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, and `duration_ms` for every `tokio::process::Command` invocation. Source: brief Section 4 / Architecture subsection 6 (Git engine: module split / audit trail).
- [x] AC3: Pure parsers exist for `fetch`, `rev-parse`, `status --porcelain=v2`, `for-each-ref`, and `rev-list --left-right --count` as functions over captured output with no I/O. Source: brief Section 6 (Git engine `cli.rs` row: "Parsers are pure functions over captured output").
- [x] AC4: `git/inspect.rs` reports HEAD SHA, active branch, dirty status, detached state, and ahead/behind counts via `git2`. Source: brief Section 4 / Architecture subsection 6 (Git engine) and Section 6 (`inspect.rs` row).
- [x] AC5: `git2`/`libgit2-sys` are pinned with the `vendored` feature and network transports disabled (no OpenSSL, no libssh2); the trait makes an all-CLI read fallback a localized change. Source: brief Section 4 / Architecture subsection 6 (Git engine: the libgit2 sub-decision).
- [x] AC6: Git discovery follows the order explicit `settings.git_executable_path` -> `PATH` -> well-known Windows locations, and caches the result. "User-overridable" here means exactly that discovery honors `settings.git_executable_path` as the FIRST discovery candidate when it is set, and nothing more: writing or persisting that setting is a UI/settings concern out of scope for this effort. Source: brief Section 4 / Architecture subsection 10d (Discovery order).
- [x] AC7: Version probing via `git --version` enforces a git >= 2.30 floor and surfaces a clear, non-blocking warning below it. Source: brief Section 4 / Architecture subsection 10d (Minimum version floor).
- [x] AC8: "git not found" is a first-class state distinct from a generic error: the engine exposes a distinct "git unavailable" state value (the data a banner needs) plus a callable re-probe entry point that, on a later successful discovery, flips the state off "git unavailable". E-03 owns this signal and the re-probe; it does NOT assert the scheduled-check pause/resume behavior itself - that loop-control behavior is verified in E-08 (scheduler), which consumes this signal and re-probe. Source: brief Section 4 / Architecture subsection 10d ("git not found" behavior).
- [x] AC9: The `GitEngine` and its discovery initialize SUCCESSFULLY (return a usable engine, not an `Err`) when no `git` is present, landing in the "git unavailable" state so app launch is never gated on git discovery. Source: brief Section 4 / Architecture subsection 10d ("the app must launch and remain usable for browsing existing state even with no git").
- [x] AC10: The `fetch` parser classifies every invocation into at least these outcome classes: success, no-op (already up to date), auth-failure, network-failure, and unknown. "Unknown" is a valid conservative result when the captured exit code + stderr do not match a known signature. This minimum set exists because the update-policy engine (E-07) maps auth-failure to pause and network-failure to retry, so the parser must distinguish them. Source: brief Section 4 / Architecture subsection 6 (Git engine: CLI owns network ops and the capture the parser reads); the auth-vs-network pause/retry mapping that motivates the split is owned by E-07.
- [x] AC11: For ahead/behind counts, E-03 owns a provisional contract (E-04's cross-check ratifies or flags it, rather than both docs deferring to each other): in the no-upstream state, ahead/behind counts are `None` (not `(0, 0)`), since "no comparison base" is distinct from "equal to upstream"; the deleted-upstream state, where the configured upstream ref no longer resolves, reports ahead/behind as `None` for the same reason. Source: brief Section 4 / Architecture subsection 6 (Git engine: ahead/behind reads) and the no-upstream/deleted-upstream states defined in E-04.

## Dependencies

- Upstream: E-01 (the workspace, the `reposync-core` crate, and the empty `git/{mod,cli,inspect}.rs` stubs).
- Downstream: E-04 (fixture cross-check exercises both engines), E-07 (policy consumes git state), E-08 (scheduler drives the engine and the pause/resume), E-09 (activity writer persists the captured `raw_*`/`duration_ms`), E-12 (tracer bullet wires a real fetch + rev-list through to SQLite).

## V1.1 extension points

- The all-CLI read impl behind `GitEngine` becomes the live path if `libgit2-sys` ever proves unmaintainable on a target toolchain; the trait keeps this a single-file swap.
- `pull --ff-only` and other working-tree mutations beyond `fetch` extend the CLI side as new policy modes ship (E-07 enumerates them).
- Credential-helper-aware error classification can deepen once real auth-failure captures are observed in the field.

## Open questions

- Whether discovery should also probe macOS/Unix well-known locations now or defer until Mac access exists. Default: implement the discovery seam generically but only populate Windows well-known paths in V1; flag for jp at the macOS port.
- Exact `git2` ahead/behind semantics versus `rev-list --left-right --count` when no upstream is configured. E-03 now states a provisional contract (AC11: `None` for both the no-upstream and deleted-upstream states); the E-04 cross-check ratifies that contract or flags a divergence back to this spec, rather than the two docs deferring to each other.
