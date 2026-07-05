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

- **State:** core done (2026-06-29); OS wiring built 2026-07-05 (Phase 3 / P3-B), pending Windows dogfood smoke verification. `reconcile(os, setting_on)` (the AC2 startup drift-correction decision over a tri-state OS read, with a non-actuating `Unknown`) and `is_autostart_launch(args, flag)` (the AC3 launch detection) live in `reposync-core/src/autostart.rs`, built test-first (5 tests) and adversarially reviewed - the tri-state `Unknown` fix landed test-first; the "setting wins vs adopt the OS change" policy is filed as BL-NI-18 (V1 ships the authoritative-setting policy). The edge is now wired in `src-tauri`: `tauri-plugin-autostart` 2.5.1 (LaunchAgent macOS launcher, `--autostart` launch arg) is registered in `lib.rs`; a new edge module `src-tauri/src/autostart.rs` reconciles OS state against the setting on startup (`reconcile_on_launch`, best-effort / never fatal) and applies the setting live on toggle (`apply`, persist-then-apply with an honest `InvalidSetting{autostart}` on plugin failure, mirroring the git-path swap contract from 71a0f7b); an autostart launch hides the main window to the tray in `lib.rs` setup. Four pure-glue unit tests in the shell (`os_state_from_is_enabled`, `apply_action`); the registry effect itself is dogfood-only.
- **Next:** dogfood smoke on Windows (per the checklist below): toggle registers/removes the `HKCU\...\Run\RepoSync` value, autostart launch starts hidden to the tray, and reconcile corrects external tampering. P3-C (tray completion) owns close-to-tray + the optional `visible:false`-by-default refinement that removes the autostart-launch hide flash; the `AUTOSTART_LAUNCH_FLAG` const + `launched_by_autostart()` detector are the handoff seam.
- **Blockers (edge only):** none. Wiring is built behind the headless gate; the only remaining gate is the Windows dogfood smoke (the launch-only registry/login-item + start-minimized behavior that no unit test can cover).

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

- [ ] AC1: Enabling the `autostart` setting registers RepoSync to launch on login; disabling it removes the registration. Source: settings `autostart`; brief platform-seam row. **Deferred edge** - the core provides the register/unregister DECISION (`reconcile`); the `tauri-plugin-autostart` enable/disable call is the edge.
- [x] AC2: On startup, the OS autostart state is reconciled to match the persisted setting. Source: derived robustness requirement (drift correction). **Done in core** (`reconcile`: the startup drift-correction decision over a tri-state OS read; `Unknown` is non-actuating so a failed OS query never mutates state; the edge supplies the OS state + actuates).
- [ ] AC3: When launched by autostart, the app starts minimized to the tray and does not focus a window. Source: resident-utility UX (brief Section 8 + autostart row). **Detection done in core** (`is_autostart_launch`, whole-argument match so a repo path cannot false-positive); the start-minimized / no-focus window behavior is the deferred edge.
- [ ] AC4: Autostart is per-user and requires no elevation. Source: brief security model (no admin install in V1). **Deferred edge** - a property of the per-user `tauri-plugin-autostart` registration; no core logic.

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
