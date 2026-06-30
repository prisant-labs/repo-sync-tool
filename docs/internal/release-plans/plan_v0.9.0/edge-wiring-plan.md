# Edge-Wiring Plan (v0.9.0)

- **Date:** 2026-06-30
- **Status:** in progress (first slice landed)
- **Purpose:** The governing plan for the **edge-wiring effort** - the consolidated work that connects the headlessly-tested `reposync-core` to the live Tauri runtime. It is not a new spec; it sequences the deferred edge portions of several efforts into one coherent unit and defines how that (mostly non-unit-testable) work is verified.
- **Companion:** [feature-inventory.md](feature-inventory.md) (the feature view), [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), [../../program-roadmap.md](../../program-roadmap.md) (per-effort links).

## Why this effort exists

The 15 efforts deliberately built the product logic **behind the IPC seam**, where it is unit-testable with no GUI. What is left is the **edge**: the thin `src-tauri` shell that actuates that logic against the OS and the webview. Three integration efforts (E-13 tray, E-14 notifications, E-15 autostart) and parts of E-10/E-11 each left an "edge" remainder that needs the live runtime. Rather than scatter those remainders, they fold into one effort with one verification model.

## The verification reality (the methodology shift)

Every core effort to date was verified test-first: a failing unit test, then minimal code. **Most edge-wiring cannot be unit-tested** - there is no headless harness for the Tauri runtime, the tray, the OS notification center, the autostart registry, or a real webview. So this effort is verified by a two-part gate:

1. **Headless gate (automated, every change):** `cargo fmt` + `cargo clippy --workspace --all-targets -D warnings` + `cargo clippy -p reposync-core --features test-support -D warnings` + `cargo test --workspace` + `cargo tree -p reposync-core` (still tauri/chrono/openssl-free). This proves the glue compiles, is lint-clean, and that the cores it calls remain green. Any NEW core logic an edge task needs (e.g. a read query, a path resolver) is still built **test-first** in `reposync-core`.
2. **Manual smoke-test (jp, on a real Windows launch):** a per-feature checklist of observable behaviors (tray appears, a check fires a toast, autostart survives a reboot). This is the only place launch-only behavior is confirmed. The checklist lives in this doc and is filled in per slice.

> Honest boundary: between the two gates, edge glue is "compiles + the core it calls is tested" - not "behavior verified." That gap is real and is closed only by the smoke-test. Slices are kept thin so the unverified surface per slice stays small.

> Update (2026-06-30): `src-tauri` **library** unit tests DO run on Windows - only full-runtime integration tests need the comctl32 manifest (`build.rs`). So pure edge helpers (timezone math, path resolution, arg parsing) are unit-testable in `src-tauri` after all, and should be built test-first like core. Only genuinely launch-only behavior (tray, toasts, autostart registration, the webview) falls to the smoke-test. This shrinks the unverified surface meaningfully - e.g. the `localtime` day-window math landed with 3 unit tests.

## Inventory (grounded in `src-tauri`, 2026-06-30)

### Commands - 11 of 19 already real; the rest:

| Command | State | Edge-wiring task |
|---------|-------|------------------|
| `activity_list` | **DONE 2026-06-30** | wired to new `activity::list` core read (test-first) |
| `summary_today` | **DONE 2026-06-30** | wired via a new edge `localtime` helper (the `time` crate, jp's call); the local-day window math is unit-tested |
| `repo_refresh_metadata` | stub; core ready (E-10, a/c-hardened) | construct `ReqwestTransport` + `NoToken`, call `refresh_one(now)`, map the engine outcome to `AppError` (the `NetworkLost`/`RateLimited`/`NotFound` outcomes are returned as values, not errors), re-read `RepoDetail`. **Coupling found 2026-06-30:** `RateLimit` carries only `remaining`/`limit`, so an honest `AppError::RateLimited { reset_at }` needs the engine to also parse `X-RateLimit-Reset` (small test-first core change) - else the error's `reset_at` is a guess. Manual refresh is otherwise unaffected by the BL-NI-15b cadence caveat |
| `repo_open_folder/terminal/editor/remote` | stub; **no core** | resolve the repo's local path / remote URL / configured editor+terminal (test-first core helper), then OS shell-out (launch-only) |
| `summary_week` | inert stub | stays a V1.1 stub |

### Launch-dependent chrome (no unit test possible; smoke-test only):

| Piece | Owner | Task |
|-------|-------|------|
| Scheduler spawn | E-08 core ready | spawn the tokio scheduler in `lib.rs` setup; attach the daily activity-sweep + summary cadence; route **manual** `check_now`/`update_now` through the **same per-repo mutex** so manual + scheduled never collide on one repo |
| Tray + menu | E-13 (deferred) | enable the `tray-icon` feature; build `tray.rs`; wire into setup; popover window |
| Notification emit | E-14 core ready | add `tauri-plugin-notification`; call `notify::decide`/`coalesce` at the scheduler's check-completion; emit `notification:fired` |
| Autostart | E-15 core ready | add `tauri-plugin-autostart`; `reconcile` on startup against the OS state; start-minimized on the autostart launch arg |

### Backlog couplings this effort resolves or carries:
- **BL-NI-15b** - release ETag / separate cadence / durable release-staleness (carried into the scheduler refresh cadence).
- **BL-NI-16** - summary release-event fidelity (carried into `repo_refresh_metadata` + the cadence).
- **BL-NI-18** - autostart "setting wins vs adopt the OS change" policy (resolved when reconcile is wired).
- **BL-NI-17** - notification auth-toggle policy (resolved when the emit-site is wired).

## The foundational decision: local time at the edge

`reposync-core` is deliberately timezone-free (no `chrono`/`time`). Three features need the **edge** to supply local-time values the core refuses to compute:
- `summary_today` needs local-midnight day bounds + a `YYYY-MM-DD` label (`DayWindow`).
- Quiet-hours (`notify.rs`) needs the current **local minute-of-day** (`LocalMinute`).
- The scheduler's daily cadence needs "is it a new local day."

Rust's std has no local-time API, so the edge needs a mechanism. This is the first decision because it gates `summary_today`, notifications, and the daily cadence. Options are recorded with the decision in this session; the dependency-hygiene rule (`no chrono/time`) is **core-scoped**, so a vetted crate in `src-tauri` does not break the core's tree-clean guarantee.

## Proposed sequence

1. **Headless command wiring first** (compile-verifiable, cores already tested): `activity_list` (done) -> resolve the local-time decision -> `summary_today` -> `repo_refresh_metadata` -> the `repo_open_*` path-resolution core + shell-out.
2. **Scheduler spawn + manual-command locks** (the behavioral spine; needs smoke-test).
3. **The plugin chrome** (notification emit, autostart) - each a dep add + a wire + a smoke-test line.
4. **Tray** (E-13) last - it depends on a window + the scheduler control surface.

Slices land independently behind the headless gate; the smoke-test checklist accumulates for one batched Windows launch.

## Smoke-test checklist (filled per slice; run on a real Windows launch)

- [ ] App launches; tray icon appears; popover opens.
- [ ] A manual "check now" updates the UI and (on a real change) fires one toast.
- [ ] A scheduled cycle fires; quiet-hours suppresses toasts in-window.
- [ ] `summary_today` shows the correct local day.
- [ ] `repo_refresh_metadata` populates release/topics/archived.
- [ ] `repo_open_*` open the folder / terminal / editor / remote.
- [ ] Autostart toggle registers; app starts minimized on a reboot launch.
