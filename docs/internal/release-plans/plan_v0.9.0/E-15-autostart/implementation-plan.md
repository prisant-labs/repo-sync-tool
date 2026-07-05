---
effort: E-15
title: Autostart (Launch on Login) - implementation plan
status: ready
---

# E-15 - Autostart (Launch on Login) - Implementation Plan

## Ordered steps

1. **Plugin.** Add `tauri-plugin-autostart` to `src-tauri` (Cargo + the builder + capabilities). Configure it with the launch argument that marks an autostart start (e.g. `--minimized`).
2. **Setting bridge.** On `settings_set`, if `autostart` changed, call the plugin to enable/disable launch-on-login accordingly.
3. **Reconcile on startup.** On app start, read the persisted `autostart` setting and the plugin's current OS state; if they differ, set the OS state to match the setting.
4. **Minimized start.** Detect the autostart launch argument; when present, skip showing/focusing the main window and start resident in the tray (coordinate with E-13's tray + E-01's window lifecycle).
5. **Verify.** Manual on Windows: toggle on -> confirm a Run entry exists and the app launches on login minimized; toggle off -> entry removed; flip the OS entry manually and confirm reconciliation on next start.

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

- All four ACs met; toggling the setting registers/unregisters launch-on-login on Windows; reconciliation works; autostart launches start minimized; the setting->action bridge is unit-tested; local gate green.
