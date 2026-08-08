---
effort: E-15
title: Autostart (Launch on Login) - implementation plan
status: ready
---

# E-15 - Autostart (Launch on Login) - Implementation Plan

## Ordered steps

1. **Plugin.** Add `tauri-plugin-autostart` to `src-tauri` (Cargo + the builder + capabilities). Configure it with the launch argument that marks an autostart start (e.g. `--minimized`).
2. **Setting bridge.** On `settings_set`, if `autostart` changed, call the plugin to enable/disable launch-on-login accordingly.
3. **Reconcile on startup.** On app start, read the persisted `autostart` setting and the plugin's current OS state; if they differ and the OS state could actually be read, set the SETTING to match the OS (amended 2026-08-07, BL-NI-18 - originally the reverse). The registration is never touched here; only the explicit Settings toggle actuates it.
4. **Minimized start.** Detect the autostart launch argument; when present, skip showing/focusing the main window and start resident in the tray (coordinate with E-13's tray + E-01's window lifecycle).
5. **Verify.** Manual on Windows: toggle on -> confirm a Run entry exists and the app launches on login minimized; toggle off -> entry removed; **remove the Run entry via Task Manager's Startup tab while the setting reads on, restart, and confirm the SETTING now reads off** (the BL-NI-18 direction - the old expectation was the entry reappearing) with an `autostart.adopted_os_state` line in the log; add a Run entry externally while the setting reads off and confirm the setting flips on.

## Test strategy

- The OS registration is platform side-effect, so coverage is mostly a Windows manual smoke (toggle on/off, reboot-or-relogin check, reconciliation). Keep the setting-to-action bridge thin and assertable: a unit/integration check that "autostart on -> plugin enable called, off -> disable called" via a small trait wrapper so the decision is testable without touching the registry. macOS login-item behavior is deferred to the staged Mac pass.

## Files touched

- `src-tauri/src/` (a small `autostart` wiring module + builder registration + the minimized-start branch in the window lifecycle).
- `src-tauri/Cargo.toml` + capabilities (add `tauri-plugin-autostart`).
- Hook in the `settings_set` command path (E-02) to call the bridge on change.

## Risks

- Antivirus / enterprise policy can block Run-key writes; surface a clear `AppError` (config domain) rather than failing silently, so the settings UI can report it.
- "Start minimized" detection must be reliable, or autostart launches pop a window; gate strictly on the launch argument.

## Definition of done

- All four ACs met; toggling the setting registers/unregisters launch-on-login on Windows; reconciliation adopts an externally-changed OS state into the setting (BL-NI-18) rather than re-forcing the registration; autostart launches start minimized; the setting->action bridge is unit-tested; local gate green.
