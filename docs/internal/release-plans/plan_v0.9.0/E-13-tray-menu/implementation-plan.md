---
effort: E-13
title: Tray Native Menu - implementation plan
status: ready
---

# E-13 - Tray Native Menu - Implementation Plan

## Ordered steps

1. **Menu skeleton.** In `src-tauri/src/tray.rs`, build a `TrayIconBuilder` with a `Menu` containing the six items (Show, Check All, Pause/Resume, Open recent submenu, Settings, Quit). Register it in the app builder (`lib.rs`). Static labels first.
2. **Window actions.** Wire Show and Settings to show + focus the main window (and emit a "navigate to settings" signal for Settings). Implement close-to-tray: intercept the main window close-requested event, hide instead of destroy.
3. **Quit.** Wire Quit to a clean shutdown: stop the scheduler, close the sqlx pool, exit.
4. **Scheduler controls.** Wire Pause/Resume to the E-08 scheduler's pause/resume; read current state to set the item label. Wire Check All Now to the scheduler's check-all (or iterate `repo_check_now` over enabled repos).
5. **Recent submenu.** Populate "Open recent repo" from E-02's repo list (most-recently-checked N); each entry calls `repo_open_folder` (or shows detail). Rebuild the submenu when the list changes.
6. **Tooltip.** Set the tray tooltip from the daily summary / attention count (best-effort; static "RepoSync" until E-11 wires the count).
7. **Left-click.** On Windows, map the tray left-click to show + focus the window.
8. **Verify.** Manual: launch, exercise every item; confirm close-to-tray keeps the process resident and Quit exits cleanly. Automated where feasible: a smoke test that the menu builds without panicking.

## Test strategy

- The tray is native chrome, so coverage is mostly manual (launch + click). Add a headless unit test that constructs the menu definition (item ids/labels) without a running event loop, asserting the six items + submenu exist. Pause/Resume and Check-All logic that lives in the scheduler (E-08) is unit-tested there; this effort tests the wiring is connected.

## Files touched

- `src-tauri/src/tray.rs` (the menu; primary).
- `src-tauri/src/lib.rs` (register the tray; close-to-tray handler).
- `src-tauri/src/windows/mod.rs` (show/focus/select-view helpers).

## Risks

- macOS menu semantics differ (template images, click behavior); per the brief, macOS is build/bundle-only, so macOS menu QA is deferred to the staged Mac pass. Keep the menu definition platform-neutral; isolate any per-OS bits.
- Close-to-tray must not leave a zombie process on Quit; verify the scheduler task and pool are torn down.

## Definition of done

- All five ACs met; the menu is wired to real actions; close-to-tray and clean Quit verified on Windows; `tray.rs` holds no product logic; local gate green.
