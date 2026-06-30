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

/// The observed OS launch-on-login registration state, as the edge's plugin query reports
/// it. `Unknown` models a query that failed, timed out, or is unsupported: the core must
/// NOT actuate (register/unregister) from an untrusted observation, so it maps to
/// `NoChange` (Codex review finding 2). Only a CONFIRMED `Registered` / `Unregistered` can
/// drive an OS mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsAutostartState {
    /// The OS query confirmed a launch-on-login registration exists.
    Registered,
    /// The OS query confirmed no launch-on-login registration exists.
    Unregistered,
    /// The OS state could not be determined (query failed / timed out / unsupported).
    Unknown,
}

/// Decide how to reconcile the OS autostart registration with the persisted setting on
/// startup (AC2).
///
/// V1 policy: the persisted `autostart` setting is authoritative. When a CONFIRMED OS
/// state disagrees with it, move the OS to match (Register / Unregister); when they agree,
/// do nothing. A failed/unknown OS query (`OsAutostartState::Unknown`) is non-actuating -
/// the core never mutates OS state from an untrusted read (finding 2); the edge can
/// log/retry it.
///
/// The reviewer flagged (finding 1) that "setting wins" also re-forces a user's direct
/// OS-level change on every launch. Treating an OS-originated change as an intent to ADOPT
/// (update the setting) or surfacing a conflict needs a persisted last-observed-OS state
/// and a settings write - both edge-owned - so it is deferred to the edge-wiring effort
/// and tracked as BL-NI-18. V1 keeps the simple authoritative-setting policy.
pub fn reconcile(os: OsAutostartState, setting_on: bool) -> AutostartAction {
    match (os, setting_on) {
        (OsAutostartState::Unregistered, true) => AutostartAction::Register,
        (OsAutostartState::Registered, false) => AutostartAction::Unregister,
        // Aligned (Registered+on, Unregistered+off), or an Unknown read that must never
        // actuate from an untrusted observation (finding 2): do nothing.
        _ => AutostartAction::NoChange,
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
        // AC2: the setting is the source of truth. Setting on + OS confirmed not
        // registered (first run, or a registration that failed / was removed) -> register.
        assert_eq!(
            reconcile(OsAutostartState::Unregistered, true),
            AutostartAction::Register
        );
    }

    #[test]
    fn reconcile_unregisters_when_setting_off_but_os_registered() {
        // AC2: setting off + OS confirmed registered (the user disabled it in-app while a
        // stale registration lingered, or enabled it via the OS) -> remove it.
        assert_eq!(
            reconcile(OsAutostartState::Registered, false),
            AutostartAction::Unregister
        );
    }

    #[test]
    fn reconcile_no_change_when_already_aligned() {
        // AC2: the OS already matches the setting either way -> do nothing (idempotent).
        assert_eq!(
            reconcile(OsAutostartState::Registered, true),
            AutostartAction::NoChange
        );
        assert_eq!(
            reconcile(OsAutostartState::Unregistered, false),
            AutostartAction::NoChange
        );
    }

    #[test]
    fn reconcile_does_not_actuate_on_unknown_os_state() {
        // Codex review finding 2: a failed/unknown OS query must NOT drive a register or
        // unregister - the core never mutates OS state from an untrusted observation. The
        // edge logs/retries the failed query instead.
        assert_eq!(
            reconcile(OsAutostartState::Unknown, true),
            AutostartAction::NoChange
        );
        assert_eq!(
            reconcile(OsAutostartState::Unknown, false),
            AutostartAction::NoChange
        );
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
