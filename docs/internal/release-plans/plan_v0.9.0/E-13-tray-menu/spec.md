---
effort: E-13
title: Tray Native Menu
tracking-issue: 15
status: ready
tier: MUST
scope: V1 (integration; native chrome, not webview)
depends_on: [E-01, E-08, E-02]
source: docs/internal/v1-architecture-and-decisions.md (Section 8 "Tray architecture"; Section 4 / Architecture subsection on tray + window lifecycle)
---

# E-13 - Tray Native Menu

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started (spec drafted 2026-06-23 to close a category-C gap: the tray menu was identified in the brief but owned by no effort; `tray.rs` is an empty E-01 placeholder).
- **Next:** build the `TrayIconBuilder` + `Menu` in `src-tauri/src/tray.rs` and wire the menu items to existing commands.
- **Blockers:** Pause/Resume and Check-All need the E-08 scheduler control surface; "Open recent" needs E-02's repo list.

## Context

RepoSync is a resident tray utility. The native right-click tray menu is its always-available control surface, and the brief (Section 8) deliberately keeps the **native menu** for V1 while cutting the frameless popup window to V1.1. This effort wires that menu. It is platform-specific **native chrome** rendered by the OS, not a webview screen, so it is UI-independent of the unbuilt React surfaces and can be built and exercised now.

The work lives entirely in `src-tauri/src/tray.rs` (a Tauri seam, per the architecture's "platform-specific code is a thin edge" rule). Each menu item is a thin trigger that calls an existing IPC command or core entry point; no product logic is added here.

## In scope

- A `tauri::tray::TrayIconBuilder` with a native `Menu` carrying the brief's items: **Show RepoSync** (show + focus the main window), **Check All Now** (trigger a scheduler check-all over enabled repos), **Pause / Resume** (toggle the scheduler; the item label reflects current state), **Open recent repo** (a submenu of recent repos, each opening its folder or detail), **Settings** (show the main window on the settings view), **Quit** (clean shutdown).
- Left-click behavior on Windows: show + focus the main window (the popup window is cut, so left-click is not a popup).
- Window-close-to-tray: closing the main window hides it and the app keeps running in the tray; Quit is the only full exit.
- A tray tooltip / icon-state reflection of the summary (e.g. "3 need attention") is in scope at a minimal level (tooltip text); richer icon-state art is optional.

## Out of scope

- The frameless left-click **popup window** (CUT to V1.1; brief Section 8.2).
- The webview screens themselves (dashboard, settings rendering); this effort only opens/focuses the window and selects a view.
- The tray **icon assets** (`.ico` / template png), owned by E-01.
- **Notification toasts** (E-14) and **autostart** (E-15), which are separate integration efforts.

## Contract / deliverables

1. A native tray menu with the six items above, each wired to its action.
2. Pause/Resume reads and toggles the live scheduler state, with the label reflecting it.
3. Check All Now triggers a check over enabled repos via the scheduler/`repo_check_now` path.
4. Close-to-tray: the app survives main-window close and stays resident; Quit exits cleanly (scheduler stopped, pool closed).
5. Show / Settings bring up and focus the main window (Settings selects the settings view).

## Acceptance criteria

- [ ] AC1: The tray icon shows a native menu with Show RepoSync, Check All Now, Pause/Resume, Open recent (submenu), Settings, and Quit. Source: brief Section 8.1.
- [ ] AC2: Each item performs its action: Show focuses the window; Check All triggers checks; Pause/Resume toggles and reflects scheduler state; Open recent opens a repo; Settings shows the settings view; Quit exits cleanly. Source: brief Section 8.1.
- [ ] AC3: Closing the main window hides it and the app keeps running in the tray; only Quit fully exits. Source: brief Section 8 (resident utility) + Section 4 window-lifecycle.
- [ ] AC4: The menu wiring lives in `src-tauri/src/tray.rs` and adds no product logic to the shell; items call existing commands / core entry points. Source: brief "platform-specific code is a thin edge."
- [ ] AC5: On Windows, left-clicking the tray icon shows + focuses the main window (no popup). Source: brief Section 8 (popup cut to V1.1).

## Dependencies

- Upstream: E-01 (shell, `tray.rs` placeholder, window lifecycle, tray icon assets), E-08 (scheduler control: pause/resume + check-all), E-02 (recent-repos list for the submenu).
- Downstream: none hard; the GUI later coexists with the menu.

## V1.1 extension points

- The frameless left-click popup window (the cut item) layers on top of this menu.
- Richer tray icon state art (idle / syncing / attention) driven by `repo:state-changed`.

## Open questions

- **Tier (flag for jp):** marked MUST as the product's primary surface, but a first beta could ship window-only with a minimal Show/Check/Quit menu and defer the submenu. Recommend MUST with a minimal-menu fallback if it slips.
- Whether "Pause" pauses globally or also exposes per-repo pause (per-repo lives in the repo detail UI; the tray pause is global here).
