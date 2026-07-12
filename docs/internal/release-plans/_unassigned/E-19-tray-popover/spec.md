---
effort: E-19
title: Tray Popover (menu bar extra)
status: deferred
tier: SHOULD
scope: V1.1 (a lightweight webview popover + native tray wiring; layers on E-13)
depends_on: [E-13, E-08, E-10, E-11, E-06]
source: BL-V11-01 (tray popup window); audit recommendation R-INNOV-1 (the top native-feel bet); design brief promoted from _local/audit/2026-07-10_fable-audit/07_DESIGN_tray-popover.md
---

# E-19 - Tray Popover (menu bar extra)

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** DEFERRED (V1.1). PROMOTED 2026-07-11 from the 2026-07-10 audit design brief into a
  tracked spec (BL-V11-01). This is the design contract; it is not yet an implementation plan and no
  code exists. Approach A (a dedicated frameless popover webview window) is recommended and the
  boundaries are set, but five decisions (see Open questions) are the maintainer's to make before an
  implementation plan is written.
- **Effort id:** E-19 is provisional, assigned on promotion from BL-V11-01 and parked in
  `_unassigned/` pending release slotting; renumber freely if the roadmap wants a different id.
- **Next:** maintainer resolves the five open decisions, then the writing-plans step produces an
  implementation plan. The popover and the paired tray badge (AC9) are staged to land
  independently, both behind the existing-behavior fallback.
- **Blockers:** none technical. Depends on shipped surfaces (E-13 tray, E-08 scheduler control,
  E-10/E-11 status + summary, E-06 events); adds essentially no new backend IPC surface.

## Context

RepoSync's premise is ambient awareness of a repo library, yet today the tray is a right-click text
menu and a left-click just opens the full 900x600 window (`src-tauri/src/tray.rs`,
`show_main_window`). The frameless popover was deliberately cut to V1.1 (BL-V11-01). That cut is the
single biggest gap between what the app *is* (a glanceable resident utility) and what it *does* (a
thing you open). On macOS especially, a left-click popover anchored to the menu bar icon is the
canonical menu bar extra interaction; the current right-click-only menu reads as non-native there.

This effort adds a left-click, frameless, auto-dismissing popover that answers "is everything fine?"
in one glance and offers the three or four actions the user actually takes, without ever opening the
full window. It builds directly on E-13 (the native tray menu, which stays as the full control
surface and the fallback) and reuses the existing IPC + events, so it adds no product logic to the
shell and almost no backend surface.

## Approach (recommended: a dedicated popover webview window)

**Approach A - a dedicated frameless popover webview window (RECOMMENDED).** A second Tauri window
(`label: "popover"`), created hidden, frameless, transparent, always-on-top, skip-taskbar, rendered
by a small separate frontend entry. Left-click on the tray positions it near the icon and shows it;
it hides on blur. Pros: a real webview, so it reuses the React design system and the typed
`bindings.ts`; full styling control; the canonical menu bar extra look; isolated from the main app.
Cons: a second window to manage, anchor-positioning math across DPI and multi-monitor, and
blur-to-hide semantics that differ slightly by OS - all known, bounded Tauri problems.

Rejected alternatives: **B - reuse the main window in a compact mode** (conflates the full app with
the popover, fights the `visible:false` + close-to-tray lifecycle, no clean compact-vs-full
boundary); **C - a richer native menu** (native tray menus are text-only items and cannot render a
status dashboard, a colored attention list, or a live-updating header - which is exactly why a
webview popover exists).

## In scope

- A frameless, transparent, always-on-top, skip-taskbar `popover` webview window, created hidden and
  shown on tray left-click, anchored near the tray icon, auto-dismissed on blur.
- A lightweight, isolated frontend entry (`popover.html` + `src/popover/main.tsx`) rendering one
  `<Popover>` component that reuses `src/index.css` tokens and `src/lib/bindings.ts` + `ipc.ts`, but
  NOT the full app shell/screens.
- The popover content: an at-a-glance status header (overall status pill + one-line summary), a
  needs-attention list (top N with an overflow row), and a small quick-actions footer.
- A new `src-tauri/src/popover.rs` edge that owns the window lifecycle and a pure, testable
  `compute_popover_position`, with `tray.rs`'s left-click handler calling `popover::toggle`.
- Progressive-enhancement fallback: if the popover cannot build or position, left-click falls back
  to `show_main_window` (today's exact behavior).
- The paired minimal tray badge (R-INNOV-5): reflect the needs-attention count on the tray tooltip
  (and macOS `set_title`) so the user does not even have to click. Splittable: may land after the
  popover.

## Out of scope

- Inline commit/diff preview inside the popover (that is a separate feature, R-INNOV-6).
- Editing settings in the popover, drag-reordering, rich animation, or a full repo list.
- Replacing the right-click native menu (E-13), which stays as the full control surface and fallback.
- The tray icon assets themselves (owned by E-01); richer colored overlay-icon art is optional
  beyond the tooltip/title badge.

## Contract / deliverables

1. A frameless popover webview window that opens on tray left-click, anchored near the icon, and
   auto-dismisses on blur, without opening the full window.
2. A status header (pill + one-line summary) derived from the SAME status logic the dashboard uses
   (`src/lib/status.ts`), reused not reimplemented.
3. A needs-attention list (top N via `repo_list` with a needs-attention filter), each row with one
   primary action (Update / Resume / Fix) plus click-to-open-detail, and an "N more in RepoSync"
   overflow row.
4. A quick-actions footer: Check All Now, Pause/Resume, Open RepoSync, Settings, Quit.
5. A pure `compute_popover_position(tray_rect, popover_size, monitor) -> (x, y)` with unit tests, and
   a new `src-tauri/src/popover.rs` edge; `tray.rs` left-click calls `popover::toggle`.
6. A safe fallback to `show_main_window` when the popover cannot build/position, mirroring the
   `tray_available` gate.
7. The paired tray badge (tooltip + macOS title) reflecting the needs-attention count.

## Acceptance criteria

- [ ] AC1: Left-clicking the tray icon opens the frameless, transparent, always-on-top popover
  anchored near the icon; it does NOT open the full 900x600 window. Source: this spec, Approach A;
  supersedes E-13 AC5's left-click-opens-window behavior when the popover is enabled.
- [ ] AC2: The popover header shows an overall status pill (All fresh / N need attention / Paused /
  Offline / Git missing) and a one-line summary, both derived from the shared `src/lib/status.ts`
  logic (not reimplemented). Source: design brief Section 5.2.
- [ ] AC3: The needs-attention list shows the top 5 repos (behind / auth-failed / auto-paused /
  dirty) via `repo_list` with a needs-attention filter, each row's primary action mapping to an
  existing command (`repo_update_now`, `repo_set_enabled` to resume) or opening detail via a
  `navigate:requested` event, with an "N more in RepoSync" overflow row when more than 5. Source:
  design brief Section 5.2.
- [ ] AC4: The popover auto-dismisses on blur (`Focused(false)`), and a re-click while open toggles
  it closed with no hide-then-immediately-reshow race on the same click. Source: design brief
  Section 5.4.
- [ ] AC5: Positioning is computed by a PURE, unit-tested `compute_popover_position` that clamps to
  the monitor work-area and applies the monitor DPI scale, anchoring above the icon on Windows and
  below the menu bar on macOS. Source: design brief Section 5.4; mirrors the `decide_window_lifecycle`
  pattern.
- [ ] AC6: If the popover window fails to build or a monitor cannot be resolved, the left-click FALLS
  BACK to `show_main_window` (today's behavior); the right-click native menu remains available
  regardless. Source: design brief Section 5.5; mirrors the `tray_available` gate.
- [ ] AC7: The popover is a live subscriber - on show it fetches `repo_list(needs)` + `summary_today`
  and listens for `CheckCompleted`, `SchedulerTick` (gated on `checked > 0`, reusing the existing
  gate), `MetadataRefreshed`, and `StateChanged` to refetch, so a background check reflects without
  reopening. No new event types; no N+1. Source: design brief Section 5.3.
- [ ] AC8: The frontend entry is a separate `popover.html` + `src/popover/main.tsx` reusing
  `src/index.css` tokens and `bindings.ts`/`ipc.ts`, NOT the full app shell (a tiny, isolated
  bundle). Source: design brief Section 5.1.
- [ ] AC9: The edge lives in a new `src-tauri/src/popover.rs`; `tray.rs`'s left-click calls
  `popover::toggle`, and no product logic is added to the shell (the thin-edge rule). Source: design
  brief Section 5.1; AGENTS.md shell-crate rule.
- [ ] AC10: The paired tray badge reflects the needs-attention count via the tray tooltip (and macOS
  `set_title`), driven by the same status and updated on the same events. Splittable from the popover.
  Source: design brief Section 5.6 (R-INNOV-5).
- [ ] AC11: No new backend IPC surface is required; the popover reuses `repo_list`, `summary_today`,
  `repo_check_all`, the pause flag, `repo_update_now`, `repo_set_enabled`, and `navigate:requested`.
  Source: design brief Section 2.

## Dependencies

- Upstream: E-13 (the native tray menu + left-click handler this modifies; the fallback surface),
  E-08 (scheduler control: pause/resume + check-all), E-10 / E-11 (repo status + the daily summary
  the header reads), E-06 (the `repo:*` events the popover subscribes to and `navigate:requested`).
- Downstream: none hard. The popover coexists with the native menu and the full window.
- Cross-cutting: touches `src-tauri` (serialize per the shell-crate chokepoint) and adds a frontend
  entry, but adds essentially no new backend IPC surface, which contains the risk.

## Testing

- Pure `compute_popover_position` unit tests (anchor above/below, monitor-work-area clamping, DPI
  scale, multi-monitor) - harness-free, like `decide_window_lifecycle`.
- Frontend: the `<Popover>` status-derivation and needs-attention rendering via the vitest harness
  proposed in SPEC-03 (test-infrastructure), reusing the `src/lib/status.ts` tests.
- Manual: the macOS anchor/blur behavior belongs on the macOS first-run hardware-validation checklist
  (SPEC-05); positioning across DPI and multi-monitor is the part most likely to need real-hardware
  tuning.

## V1.1 extension points

- Inline commit/diff preview in a row (R-INNOV-6), layered on the popover.
- Richer tray icon-state art (idle / syncing / attention) beyond the tooltip/title badge, driven by
  `repo:state-changed`.

## Open questions

These five decisions are the maintainer's to make before this becomes an implementation plan
(carried from the design brief, Section 8):

1. **Left-click behavior:** the popover replaces the current left-click-opens-full-window entirely
   (recommended - it is the point), with the full window reachable via the popover footer and the
   right-click "Show RepoSync". Agree, or keep an option to disable the popover and restore
   left-click-opens-window?
2. **Frontend entry:** a separate lightweight `popover.html` entry (recommended, for isolation and a
   tiny bundle) vs a route inside the existing app.
3. **Badge now or later:** pair the minimal tray badge (AC10 / R-INNOV-5) with this, or ship the
   popover alone first?
4. **macOS timing:** build cross-platform now and tune macOS positioning in the macOS hardware pass
   (SPEC-05), or defer the whole feature until Mac access exists?
5. **Row actions depth:** V1 rows offer one primary action each (Update / Resume / Fix) plus
   click-to-open-detail (recommended), or start with click-to-open-detail only and add inline actions
   later?

## Provenance

Promoted 2026-07-11 from the design brief at
`_local/audit/2026-07-10_fable-audit/07_DESIGN_tray-popover.md` (the exploratory brief that explored
the design space and the rejected alternatives in full). This spec is now the tracked authority;
the brief is retained under `_local` for its longer rationale but is not required reading. The
feature is BL-V11-01 and audit recommendation R-INNOV-1.
