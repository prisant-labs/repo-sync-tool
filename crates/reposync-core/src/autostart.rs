//! autostart - owned by E-15 (launch-on-login reconciliation).
//!
//! The PURE, Tauri-free half of autostart: given the OS launch-on-login registration
//! state and the persisted `autostart` setting, decide what to do so the two agree (AC2),
//! and decide whether THIS launch was an autostart launch (AC3 detection). The actual
//! `tauri-plugin-autostart` enable/disable call, the launch argument it adds, and the
//! start-minimized-to-tray behavior live in `src-tauri` (the thin edge); this module is
//! just the decisions, so it is unit-testable with no plugin or UI dependency.

/// What startup reconciliation should do to make the OS launch-on-login state match the
/// persisted `autostart` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutostartAction {
    /// The setting is on but the OS has no registration: register launch-on-login.
    Register,
    /// The setting is off but the OS still has a registration: remove it.
    Unregister,
    /// The OS state already matches the setting: do nothing.
    NoChange,
}

/// Decide how to reconcile the OS autostart registration with the persisted setting on
/// startup (AC2). The persisted setting is the source of truth: if the two disagree, move
/// the OS to match it; if they agree, do nothing. This is the whole drift-correction
/// policy - it covers a user who toggled launch-on-login via the OS directly, and a prior
/// run that failed to (un)register.
pub fn reconcile(os_registered: bool, setting_on: bool) -> AutostartAction {
    match (os_registered, setting_on) {
        (false, true) => AutostartAction::Register,
        (true, false) => AutostartAction::Unregister,
        (true, true) | (false, false) => AutostartAction::NoChange,
    }
}

/// Whether this process was launched by the autostart registration, detected by the
/// launch argument the registration adds (AC3). An autostart launch starts minimized to
/// the tray; a normal launch shows the window. The flag string is the plugin's (confirmed
/// at wiring); the decision - "is exactly that flag present in argv?" - is pure and lives
/// here so the edge does not reimplement it (and cannot drift into substring matching that
/// a repo path could trip).
pub fn is_autostart_launch(args: &[String], flag: &str) -> bool {
    args.iter().any(|arg| arg == flag)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reconcile_registers_when_setting_on_but_os_unregistered() {
        // AC2: the setting is the source of truth. Setting on + OS not registered (first
        // run, or a prior registration that failed / was removed via the OS) -> register.
        assert_eq!(reconcile(false, true), AutostartAction::Register);
    }

    #[test]
    fn reconcile_unregisters_when_setting_off_but_os_registered() {
        // AC2: setting off + OS still registered (the user disabled it in-app while a
        // stale registration lingered, or enabled it via the OS) -> remove it.
        assert_eq!(reconcile(true, false), AutostartAction::Unregister);
    }

    #[test]
    fn reconcile_no_change_when_already_aligned() {
        // AC2: the OS already matches the setting either way -> do nothing (idempotent).
        assert_eq!(reconcile(true, true), AutostartAction::NoChange);
        assert_eq!(reconcile(false, false), AutostartAction::NoChange);
    }

    #[test]
    fn is_autostart_launch_matches_the_flag_exactly() {
        // AC3: an exact argv match means autostart launched us.
        let flag = "--autostart";
        assert!(is_autostart_launch(
            &["reposync.exe".to_string(), "--autostart".to_string()],
            flag
        ));
        // A normal launch (no flag) is not an autostart launch.
        assert!(!is_autostart_launch(&["reposync.exe".to_string()], flag));
        // A mere substring (e.g. a repo path that contains the flag text) must NOT
        // false-positive - detection is whole-argument equality, not `contains`.
        assert!(!is_autostart_launch(
            &[
                "reposync.exe".to_string(),
                "C:/repos/--autostart-notes".to_string()
            ],
            flag
        ));
    }
}
