---
effort: E-15
title: Autostart (Launch on Login)
tracking-issue: 17
status: ready
tier: SHOULD
scope: V1 (integration)
depends_on: [E-02, E-01]
source: docs/internal/v1-architecture-and-decisions.md (cross-platform table row "autostart behind abstraction"; settings column `autostart`)
---

# E-15 - Autostart (Launch on Login)

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** not started (spec drafted 2026-06-23 to close a category-C gap: the `autostart` setting column exists, but no effort owned the OS registration mechanism).
- **Next:** wire the `autostart` setting to `tauri-plugin-autostart` so toggling it registers/unregisters launch-on-login.
- **Blockers:** needs E-02's `autostart` setting persistence.

## Context

A resident tray utility is most useful when it is already running. This effort makes RepoSync optionally launch on login, driven by the `autostart` setting (default **off**, opt-in). Launch-on-login registration is platform-specific (Windows: a Run key / Startup entry; macOS: a login item), abstracted by `tauri-plugin-autostart` (one API, per-OS mechanism). The architecture already names autostart as one of the thin platform seams; this effort fills it in. It is UI-independent: the toggle UI is the settings screen's job, but the registration logic and its reconciliation are headless.

## In scope

- Wire the `autostart` setting to the OS: enabling it registers launch-on-login; disabling it removes the registration.
- **Startup reconciliation**: on app start, make the OS autostart state match the persisted setting (correct drift if a user changed it via the OS, or if a prior run failed to register).
- **Start minimized to tray** when the process was launched by autostart (detected via a launch argument the registration adds), so an autostart launch does not pop a window in the user's face.

## Out of scope

- The settings **toggle UI** (settings screen renders `autostart`; storage is E-02).
- The window/tray lifecycle itself (E-01 / E-13); this effort only requests "start minimized" on an autostart launch.
- Any elevation / admin install behavior; autostart here is per-user, no elevation.

## Contract / deliverables

1. A small autostart module that enables/disables launch-on-login via `tauri-plugin-autostart`, keyed off the `autostart` setting.
2. Startup reconciliation that aligns the OS state with the setting on every launch.
3. Autostart launches start minimized to the tray (no focused window).

## Acceptance criteria

- [ ] AC1: Enabling the `autostart` setting registers RepoSync to launch on login; disabling it removes the registration. Source: settings `autostart`; brief platform-seam row.
- [ ] AC2: On startup, the OS autostart state is reconciled to match the persisted setting. Source: derived robustness requirement (drift correction).
- [ ] AC3: When launched by autostart, the app starts minimized to the tray and does not focus a window. Source: resident-utility UX (brief Section 8 + autostart row).
- [ ] AC4: Autostart is per-user and requires no elevation. Source: brief security model (no admin install in V1).

## Dependencies

- Upstream: E-02 (the `autostart` setting persistence), E-01 (window/tray lifecycle for the minimized start). The plugin call sits in `src-tauri`.
- Downstream: the settings screen renders the toggle (out of scope here).

## V1.1 extension points

- "Start minimized" as an independent preference (separate from autostart).
- Delayed / on-network autostart conditions.

## Open questions

- **Tier (flag for jp):** marked SHOULD - autostart is opt-in (default off) and not required to use the app. Keep SHOULD.
- `tauri-plugin-autostart` vs a hand-rolled registry/login-item writer. Default: the plugin, for the cross-platform abstraction; fall back to manual only if the plugin is unreliable on Windows.
- The exact launch-argument used to detect an autostart start (the plugin can add one); confirm during wiring.
