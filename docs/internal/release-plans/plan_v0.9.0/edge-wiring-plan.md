# Edge-Wiring Plan (v0.9.0)

- **Date:** 2026-06-30 (last updated 2026-07-04)
- **Status:** most slices landed 2026-07-03; the remainder is sequenced into [execution-plan.md](execution-plan.md) Phase 3
- **Purpose:** The governing plan for the **edge-wiring effort** - the consolidated work that connects the headlessly-tested `reposync-core` to the live Tauri runtime. It is not a new spec; it sequences the deferred edge portions of several efforts into one coherent unit and defines how that (mostly non-unit-testable) work is verified.
- **Companion:** [feature-inventory.md](feature-inventory.md) (the feature view), [plan_v0.9.0.md](plan_v0.9.0.md) (the release plan), [execution-plan.md](execution-plan.md) (the phased path to the tag), [../../program-roadmap.md](../../program-roadmap.md) (per-effort links).

> **2026-07-04 update:** the 2026-07-03 build session landed most of this plan's inventory: the scheduler spawn, the tray (partially), the `repo_open_*` commands, and Groups (not originally part of this plan, but built alongside it). What is left, plus the correctness defects the 2026-07-04 audit found in the landed work, is sequenced in [execution-plan.md](execution-plan.md). This document is kept as the historical inventory and rationale; treat `execution-plan.md` as the current source for what remains.

## Why this effort exists

The 15 efforts deliberately built the product logic **behind the IPC seam**, where it is unit-testable with no GUI. What is left is the **edge**: the thin `src-tauri` shell that actuates that logic against the OS and the webview. Three integration efforts (E-13 tray, E-14 notifications, E-15 autostart) and parts of E-10/E-11 each left an "edge" remainder that needs the live runtime. Rather than scatter those remainders, they fold into one effort with one verification model.

## The verification reality (the methodology shift)

Every core effort to date was verified test-first: a failing unit test, then minimal code. **Most edge-wiring cannot be unit-tested** - there is no headless harness for the Tauri runtime, the tray, the OS notification center, the autostart registry, or a real webview. So this effort is verified by a two-part gate:

1. **Headless gate (automated, every change):** `cargo fmt` + `cargo clippy --workspace --all-targets -D warnings` + `cargo clippy -p reposync-core --features test-support -D warnings` + `cargo test --workspace` + `cargo tree -p reposync-core` (still tauri/chrono/openssl-free). This proves the glue compiles, is lint-clean, and that the cores it calls remain green. Any NEW core logic an edge task needs (e.g. a read query, a path resolver) is still built **test-first** in `reposync-core`.
2. **Manual smoke-test (jp, on a real Windows launch):** a per-feature checklist of observable behaviors (tray appears, a check fires a toast, autostart survives a reboot). This is the only place launch-only behavior is confirmed. The checklist lives in this doc and is filled in per slice.

> Honest boundary: between the two gates, edge glue is "compiles + the core it calls is tested" - not "behavior verified." That gap is real and is closed only by the smoke-test. Slices are kept thin so the unverified surface per slice stays small.

> Update (2026-06-30): `src-tauri` **library** unit tests DO run on Windows - only full-runtime integration tests need the comctl32 manifest (`build.rs`). So pure edge helpers (timezone math, path resolution, arg parsing) are unit-testable in `src-tauri` after all, and should be built test-first like core. Only genuinely launch-only behavior (tray, toasts, autostart registration, the webview) falls to the smoke-test. This shrinks the unverified surface meaningfully - e.g. the `localtime` day-window math landed with 3 unit tests.

## Inventory (grounded in `src-tauri`, last regrounded 2026-07-04)

### Commands - 18 of 19 real; only `summary_week` remains an intentional stub:

| Command | State | Edge-wiring task |
|---------|-------|------------------|
| `activity_list` | **DONE 2026-06-30** | wired to new `activity::list` core read (test-first) |
| `summary_today` | **DONE 2026-06-30** | wired via a new edge `localtime` helper (the `time` crate, jp's call); the local-day window math is unit-tested |
| `repo_refresh_metadata` | **DONE 2026-06-30** | wired over `refresh_one` (NoToken path); engine outcome -> `AppError` via a pure **unit-tested** mapper (Offline / NotFound / RateLimited{reset_at}; cache/200/Skipped = success -> re-read detail). Prerequisite landed test-first: the engine now captures `X-RateLimit-Reset` into `RateLimit.reset_at` and `FetchOutcome::RateLimited` carries the budget, so the rate-limited error is honest (not a guessed reset). Manual refresh is unaffected by the BL-NI-15b cadence caveat |
| `repo_open_folder/terminal/editor/remote` | **DONE 2026-07-03 (commit 8fc806c), with open defects** | implemented, but shipped broken on Windows: `local_path` is stored canonicalized (`std::fs::canonicalize`), so `explorer`, the configured editor, and the terminal all receive an unusable `\\?\C:\...` path and fail (HIGH, audit finding 1); `repo_open_remote` passes the raw remote URL to `explorer` with no scheme validation, so a crafted remote can be executed on click (HIGH, security, audit finding 2); `open_editor`'s `cmd /C` is metacharacter-injectable (finding 8) and always reports success even when the launch fails (finding 9). Fix is Phase 1 (Correctness) of [execution-plan.md](execution-plan.md) |
| `summary_week` | inert stub | stays a V1.1 stub, by design |

### Launch-dependent chrome (no unit test possible; smoke-test only):

| Piece | Owner | Task |
|-------|-------|------|
| Scheduler spawn | **DONE 2026-07-03 (commit 81c96af)** | the tokio scheduler spawns resident in `lib.rs` setup, with manual `check_now`/`update_now` routed through the same per-repo mutex as the scheduled path, so manual and scheduled work never collide on one repo. Known gap (audit finding 6): if git is unavailable at startup, no scheduler is spawned at all, so even a later live git re-probe (BL-NI-19) has no running loop to pick it up, a gap wider than BL-NI-23's backlog description assumes. The daily activity-sweep also only runs at startup, not attached to the tick. Both are Phase 1 fixes |
| Tray + menu | E-13 (**PARTIAL**, commit bb353f9, 2026-07-03) | Show RepoSync, Quit, and left-click-show are wired. Check All Now, Pause/Resume, Open recent (submenu), the Settings menu item, and close-to-tray are NOT built (AC1/AC2 mostly unmet, AC3 entirely unmet per [E-13-tray-menu/spec.md](E-13-tray-menu/spec.md)). The frameless popover window stays V1.1 as planned. Completion is Phase 3 of [execution-plan.md](execution-plan.md) |
| Notification emit | E-14 core ready | add `tauri-plugin-notification`; call `notify::decide`/`coalesce` at the scheduler's check-completion; emit `notification:fired`. Not yet built; Phase 3 |
| Autostart | E-15 core ready | add `tauri-plugin-autostart`; `reconcile` on startup against the OS state; start-minimized on the autostart launch arg. Not yet built; Phase 3 |

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

## Proposed sequence (historical; see status per step)

1. **Headless command wiring first** (compile-verifiable, cores already tested): `activity_list` (done) -> resolve the local-time decision -> `summary_today` -> `repo_refresh_metadata` -> the `repo_open_*` path-resolution core + shell-out. **DONE** (2026-06-30 through 2026-07-03), though `repo_open_*` shipped with the defects noted above.
2. **Scheduler spawn + manual-command locks** (the behavioral spine; needs smoke-test). **DONE 2026-07-03** (commit 81c96af); the smoke-test itself has not yet run (Phase 2 of the execution plan).
3. **The plugin chrome** (notification emit, autostart) - each a dep add + a wire + a smoke-test line. **NOT STARTED**; Phase 3 of the execution plan.
4. **Tray** (E-13) last - it depends on a window + the scheduler control surface. **PARTIAL 2026-07-03** (commit bb353f9): Show/Quit and left-click-show only; completion is Phase 3.

Slices land independently behind the headless gate; the smoke-test checklist accumulates for one batched Windows launch, which has not yet happened (Phase 2, Dogfood, in [execution-plan.md](execution-plan.md)).

## Smoke-test checklist (not yet run; execute in Phase 2, Dogfood, of the execution plan)

> Note: the frameless left-click popover window is cut to V1.1 (see `features-and-outcomes.md` Section 9). The tray surface to verify is the native right-click menu, not a popover.

- [ ] App launches; tray icon appears; the native right-click menu opens with all six items (Show, Check All Now, Pause/Resume, Open recent, Settings, Quit).
- [ ] A manual "check now" updates the UI and (on a real change) fires one toast.
- [ ] A scheduled cycle fires; quiet-hours suppresses toasts in-window.
- [ ] `summary_today` shows the correct local day.
- [ ] `repo_refresh_metadata` populates release/topics/archived.
- [ ] `repo_open_*` open the folder / terminal / editor / remote, on their real (non-canonicalized) paths.
- [ ] Autostart toggle registers; app starts minimized on a reboot launch.
- [ ] Closing the main window hides it to the tray; only Quit fully exits.
