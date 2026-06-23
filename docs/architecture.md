# RepoSync Architecture

> Living contributor-facing overview of the RepoSync system and its architecture. This is a seed document: accurate as of the current build, intentionally concise, and meant to grow as each effort lands. For the deep rationale behind every decision, follow the links into the source-of-truth docs rather than expecting full depth here.

## System overview

RepoSync is a cross-platform (Windows-first) desktop tray utility that keeps a personal library of cloned-but-not-actively-developed Git repositories fresh and visible. It runs as a resident tray application: a single native Rust host process owns the async runtime, a SQLite database, a scheduler, and the git engine, while a React/TypeScript UI rendered in the OS WebView talks to that host over a typed IPC bridge. The core loop is read-mostly and safe by default: it fetches on a schedule (fast-forward only, never destructive to a working tree), records every git invocation in an audit log, and surfaces repo state, errors, and a daily summary. RepoSync is an open-source (MIT) personal utility, local-first, with no telemetry and no required account.

## Component map

Three layers, one direction of dependency. The frontend depends on the IPC contract; the Tauri shell (`src-tauri`) depends on `reposync-core`; `reposync-core` depends only on the OS and third-party crates, never on Tauri.

```
+-------------------------------------------------------------+
|  Frontend  (OS WebView: WebView2 on Windows / WKWebView)    |
|  React 19 + TypeScript + shadcn/ui + Tailwind               |
|  TanStack Query (server state) | Zustand (UI state)         |
|  bindings.ts  <- tauri-specta generated typed invoke/listen |
+----------------------------+--------------------------------+
                             | Tauri IPC (invoke / event)
                             v
+-------------------------------------------------------------+
|  src-tauri  (Tauri v2 host process, Rust)                   |
|  - #[tauri::command] handlers (thin wrappers over core)     |
|  - typed event emit helpers (events.rs)                     |
|  - tray + native menu + window lifecycle (tray.rs)          |
|  - managed state (SqlitePool, scheduler handle, paths)      |
|  - tauri-specta codegen of bindings.ts                      |
+----------------------------+--------------------------------+
                             | plain Rust calls
                             v
+-------------------------------------------------------------+
|  reposync-core  (pure Rust, NO tauri dependency)            |
|  ipc (payload structs)  | error (AppError)                  |
|  repo | policy | scheduler | activity | summary | github    |
|  paths (platform seam)  | git/{mod,cli,inspect}            |
|  migrations/ (numbered .sql)                                |
+----------------------------+--------------------------------+
                             | OS + crates only
                             v
+-------------------------------------------------------------+
|  Operating system: git binary, filesystem / app-data dir,   |
|  native tray API, notification center, login items          |
+-------------------------------------------------------------+
```

- **`crates/reposync-core`** holds all product logic and is Tauri-free: the repo registry, update policy engine, scheduler, activity writer, summary engine, GitHub client, the git engine, the `paths` seam, the SQL migrations, and the IPC payload structs plus `AppError`. It compiles and tests with no running Tauri app, no WebView, and no display server.
- **`src-tauri`** is the thin Tauri v2 shell: a set of `#[tauri::command]` functions that call into `reposync-core`, typed event emit helpers, the tray and window lifecycle, and managed state (the SQLite pool, the scheduler handle, resolved paths). It is the only place that imports `tauri`.
- **The frontend** (Vite + React + TypeScript + Tailwind + shadcn/ui) renders inside the OS WebView and never reaches the filesystem or spawns processes directly. It calls the generated, typed `bindings.ts` and listens for typed events; TanStack Query caches command results and event arrival invalidates the relevant query keys (no JS-side polling).

Source: [v1-architecture-and-decisions.md Section 4 (System overview, workspace layout)](internal/v1-architecture-and-decisions.md).

## The IPC seam (E-06)

The IPC boundary is RepoSync's real API surface, and it is the single seam that lets the frontend and backend proceed independently. It is structured so the contract cannot silently drift:

- **Payload structs live in `reposync-core::ipc`** (Tauri-free). Every payload type (`RepoSummary`, `RepoDetail`, `CheckResult`, `UpdateResult`, `ScanResult`, `ActivityRecord`, `DailySummary`, `Settings`, and so on) and `AppError` derive `serde::Serialize`/`Deserialize` plus `specta::Type`.
- **Command signatures live in `src-tauri`.** The command layer is a thin set of `#[tauri::command]` / `#[specta::specta]` functions that accept and return those core-owned types.
- **`tauri-specta` generates `src/lib/bindings.ts`** from the Rust types. The canonical generator is the `export_bindings` integration test, so generation is headless and reproducible in CI; the committed `bindings.ts` is the frontend's typed surface (`commands.*` / `events.*`), used instead of raw `invoke`/`listen`.
- **A CI stale-bindings gate** regenerates `bindings.ts` and fails on any drift, so the committed bindings can never lie about the backend. As of E-06 the contract is frozen at **18 commands and 8 events over roughly 25 payload types**; commands whose backing efforts (E-07 through E-11) are not yet built return a typed `internal.unexpected` error, so the surface is complete and stable while the behavior fills in.

Why the seam exists: for an agent-driven build by a single developer, the largest avoidable failure is contract drift between a Rust signature and a hand-written TypeScript wrapper, which produces runtime `undefined`s the type checker never catches. Making the Rust types the single source of truth means renaming a field or changing a return type breaks the TypeScript build immediately. Freezing this contract early lets the two halves of the product be built and tested independently. tauri-specta v2 is pinned exactly because it is a release candidate; the fallback is a hand-maintained `bindings.ts` plus a CI round-trip test.

Source: [v1-architecture-and-decisions.md Section 4.4 (the IPC contract as the API)](internal/v1-architecture-and-decisions.md); effort [E-06 (IPC contract)](internal/program-roadmap.md).

## The "core never imports tauri" rule

`crates/reposync-core` must never depend on `tauri`, even transitively. This is the load-bearing architectural constraint and it buys two things:

- **Headless testability.** The scheduler, git parsing, policy engine, activity writer, and migrations are unit- and integration-testable in plain `cargo test` on any OS, including headless Linux CI, with no GUI host dragged in.
- **A thin macOS port.** All platform-specific behavior is confined to a small, enumerable set of files: the `paths` module, `tray.rs` plus the per-OS icon assets, and the CI bundling config. The crate that holds the actual product behavior is identical on both platforms and stays `#[cfg]`-free.

The rule is enforced by a CI gate: `cargo tree -p reposync-core` must show no `tauri` dependency (the workflow asserts an empty `cargo tree -p reposync-core | grep -i tauri`). `specta` itself is allowed in core because it is not a Tauri dependency; only `tauri-specta` (the glue) lives in the shell.

Source: [v1-architecture-and-decisions.md Section 4.3](internal/v1-architecture-and-decisions.md); [EXECUTION.md CI gates](../EXECUTION.md).

## Data flow: the core loop

The loop that justifies the app, with the owning efforts named:

| Step | What happens | Owning effort |
|---|---|---|
| Add / scan repo | "Add folder" or "scan parent" picks a path; the backend validates it is a git repo, detects duplicates, and registers it | E-02 (persistence), E-03 (git inspect), tracer in E-12 |
| Persist | The repo and its cached local state are written to SQLite | E-02 (persistence and paths) |
| Schedule | A Tokio interval computes which repos are due (`next_check_at <= now`, enabled, outside quiet hours) and fans work out under a bounded semaphore plus a per-repo mutex | E-08 (scheduler) |
| Fetch (ff-only) | Due repos run a git operation per their policy; network/mutation goes through the `git` CLI, fast-forward only, never destructive | E-03 (git engine), E-07 (policy engine) |
| Record activity | Every git invocation is appended to the audit log with command, exit code, stdout/stderr, and duration; retention is swept | E-09 (activity writer and retention) |
| Enrich (SHOULD) | Unauthenticated GitHub metadata (description, default branch, latest release) is fetched with ETag conditional requests and rate-limit backoff | E-10 (GitHub client) |
| Daily summary (SHOULD) | A nightly aggregation rolls up what changed into an in-app card | E-11 (summary engine) |

Safety properties baked into this loop: fetches are fast-forward-only and never mutate a working tree destructively; a repo that fails 3 consecutive times is auto-paused and requires manual acknowledgement; auth failures pause the repo rather than retry-looping. The error taxonomy (`AppError`, ~30 codes with remediation) is [E-05](internal/program-roadmap.md), and the typed contract carrying all of this across the seam is [E-06](internal/program-roadmap.md).

Source: [v1-architecture-and-decisions.md Sections 4.6, 4.7](internal/v1-architecture-and-decisions.md); [strategy-and-roadmap.md Sections 3.10, 5](internal/strategy-and-roadmap.md); effort breakdown in [docs/internal/program-roadmap.md](internal/program-roadmap.md).

## Persistence model

- **SQLite via `sqlx`** in **WAL mode**, accessed through a single connection pool. The scheduler holds only short transactions and never holds a lock across a network call.
- **Numbered migrations** under `crates/reposync-core/migrations/`, embedded at compile time and applied at startup via `sqlx::migrate!`. Post-V1 the schema is additive-only: new columns with defaults, new tables, never destructive renames or drops. Pre-V1 it may be reset freely.
- **Startup recovery:** on a migration error the app does not crash or silently delete data; it moves the existing DB aside to a timestamped backup, creates a fresh DB, and surfaces a one-time notice.
- **OS-specific data directory (the `paths` seam).** The DB and logs live in `%LOCALAPPDATA%\RepoSync` on Windows (Local, never Roaming, never a OneDrive-synced folder, to avoid WAL/SHM corruption) and `~/Library/Application Support/RepoSync` on macOS. The `paths` module is the single place that computes a data path; nothing else does.
- **The schema** spans six logical areas: `repos` (registry), `repo_local_state` (cached git state, with `consecutive_failures` and `auto_paused` for the 3-strikes pause), `repo_remote_meta` (host metadata, with `etag`), `activity_records` (audit trail), `groups` + `repo_groups` (tagging), and `settings` (singleton). A nullable `scoped_bookmark_blob` column on `repos` is reserved now for a possible future macOS App Store path.

For the authoritative DDL, see [strategy-and-roadmap.md Section 4.2](internal/strategy-and-roadmap.md). Persistence and the `paths` seam are effort [E-02](internal/program-roadmap.md); the activity writer and retention are [E-09](internal/program-roadmap.md).

## Tech stack

Versions are pinned. Tauri-related crates plus `specta`/`tauri-specta` are exact-pinned because tauri-specta v2 is a release candidate.

| Layer | Component | Version |
|---|---|---|
| Shell | tauri | 2.11.3 |
| Shell | tauri-build | 2.6.3 |
| IPC codegen | specta / tauri-specta | 2.0.0-rc.25 (exact-pinned) |
| Language | Rust | stable (`rust-toolchain.toml`) |
| Persistence | sqlx | 0.9.0 |
| Git | git2 (libgit2, vendored, no network transports) | 0.21.0 |
| Serialization | serde | 1 |
| Errors | thiserror | 2 |
| Async | tokio | 1.52 |
| Frontend | React | 19.2 |
| Frontend | Vite | 8 |
| Frontend | TypeScript | 6 |
| Frontend | Tailwind | 4.3 |
| Frontend | ESLint | 10 |
| Frontend | @tauri-apps/api + cli | 2.11 |

The git engine boundary rule: anything that hits the network or writes the working tree goes through the `git` CLI (for credential-helper inheritance and behavior parity with the user's terminal); cheap local reads (HEAD SHA, branch list, dirty status, ahead/behind) go through `git2`. The GitHub client uses `reqwest` with **rustls**, not OpenSSL, to avoid platform TLS divergence.

Source: build state for this session; [v1-architecture-and-decisions.md Sections 3.5, 4.6](internal/v1-architecture-and-decisions.md).

## Platform posture and CI gates

True dual-platform, **Windows-first**. Windows is the real GA bar: launches, human-validated, signed-or-documented. macOS is "compiles + bundles in CI only" - no human-validated or signed clause - until real Mac access exists, then a staged later GA. This split exists because the sole developer has only a Windows machine and cannot run, click, render-verify, sign, or notarize a macOS build today. WebView2 (Chromium) on Windows is the source of visual truth; WKWebView (WebKit) rendering of the same UI is unverified by a human until Mac access exists.

CI gates (the boundary checkpoints), required green before any merge:

- `cargo check`, `cargo clippy --all -- -D warnings`, `cargo test`
- `pnpm typecheck`, `pnpm lint`
- Dependency-hygiene gate: `cargo tree -p reposync-core` shows no `tauri`
- Build matrix: Windows + macOS both build and bundle
- `git` pinned in CI so porcelain output stays byte-stable for the fixture harness

Source: [EXECUTION.md](../EXECUTION.md); [v1-architecture-and-decisions.md Section 2](internal/v1-architecture-and-decisions.md).

## Build status

RepoSync V1 is built as a sequence of numbered efforts (E-01 through E-12), each with its own spec and plan. The current effort table, dependency graph, sequencing, scope ledger (MUST / SHOULD / CUT), and pre-committed descope triggers live in [docs/internal/program-roadmap.md](internal/program-roadmap.md) - that file is the single source of truth for build status and is not duplicated here.

This architecture document is living: it will be updated as each effort lands and as decisions are ratified.
