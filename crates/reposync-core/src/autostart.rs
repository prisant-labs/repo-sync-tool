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
    /// The confirmed OS state disagrees with the setting: adopt the OS state by
    /// writing this value into `settings.autostart`. The OS registration is NOT
    /// touched.
    AdoptOsState(bool),
    /// The OS state already matches the setting, or could not be determined: do
    /// nothing.
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

/// Decide how to reconcile the OS autostart registration with the persisted
/// setting on startup (AC2, amended by BL-NI-18).
///
/// **The OS wins.** When a CONFIRMED OS state disagrees with the setting,
/// RepoSync updates its OWN setting to match rather than re-registering. A user
/// who turns launch-on-login off in Task Manager expects it to stay off, and the
/// previous policy quietly turned it back on at the next launch, which is the
/// app arguing with a deliberate choice it cannot see the reason for.
///
/// This is a startup-only policy and does not touch the user's explicit toggle:
/// changing `autostart` in Settings still actuates the OS immediately, through
/// the separate live-apply path. So "the OS wins" means "the OS wins about what
/// happened while RepoSync was not running", which is the only thing startup
/// reconciliation can actually observe.
///
/// KNOWN RISK, accepted deliberately when this policy was chosen: a security
/// tool or policy that strips autostart entries looks identical to a user who
/// removed one, so RepoSync will agree with it and the feature turns itself off.
/// The mitigation is that it is not silent. The setting visibly changes, so the
/// Settings screen shows it off rather than showing on while nothing happens,
/// which is the state the old policy could produce when registration kept
/// failing. The edge also logs the adoption under a stable event name.
///
/// An `Unknown` OS read remains non-actuating (Codex review finding 2, unchanged
/// and now doubly important): adopting from an untrusted observation would let a
/// transient query failure silently disable the feature.
pub fn reconcile(os: OsAutostartState, setting_on: bool) -> AutostartAction {
    match (os, setting_on) {
        // Confirmed disagreement: the OS is the source of truth, so the SETTING
        // moves, not the registration.
        (OsAutostartState::Unregistered, true) => AutostartAction::AdoptOsState(false),
        (OsAutostartState::Registered, false) => AutostartAction::AdoptOsState(true),
        // Aligned (Registered+on, Unregistered+off), or an Unknown read that must
        // never actuate from an untrusted observation (finding 2): do nothing.
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
    fn reconcile_adopts_os_off_when_setting_on_but_os_unregistered() {
        // BL-NI-18: the registration is gone but the setting says on. Something
        // outside RepoSync removed it while RepoSync was not running, and the OS
        // is authoritative about that, so the SETTING moves to off. The old
        // policy re-registered here, which is how a deliberate removal came back
        // by itself at the next launch.
        assert_eq!(
            reconcile(OsAutostartState::Unregistered, true),
            AutostartAction::AdoptOsState(false)
        );
    }

    #[test]
    fn reconcile_adopts_os_on_when_setting_off_but_os_registered() {
        // BL-NI-18, the mirror case: the user (or an installer) added the entry
        // outside the app. Adopt it rather than deleting a registration the user
        // may have just created on purpose.
        assert_eq!(
            reconcile(OsAutostartState::Registered, false),
            AutostartAction::AdoptOsState(true)
        );
    }

    #[test]
    fn reconcile_never_actuates_the_os_registration() {
        // The load-bearing half of BL-NI-18, asserted as its own property: NO
        // input to startup reconciliation can produce a change to the OS
        // registration. Adopting is a settings write; the registration is only
        // ever touched by the user's explicit toggle, through `ApplyAction`.
        // Written as an exhaustive sweep so a future variant that DOES actuate
        // cannot be added without this test being confronted.
        for os in [
            OsAutostartState::Registered,
            OsAutostartState::Unregistered,
            OsAutostartState::Unknown,
        ] {
            for setting_on in [true, false] {
                match reconcile(os, setting_on) {
                    AutostartAction::AdoptOsState(_) | AutostartAction::NoChange => {}
                }
            }
        }
    }

    #[test]
    fn reconcile_adopted_value_always_matches_the_observed_os_state() {
        // The direction is the whole point and a swapped boolean would be
        // invisible: adopting must write what the OS actually reported, never its
        // negation. Both confirmed states, asserted against the observation.
        assert_eq!(
            reconcile(OsAutostartState::Registered, false),
            AutostartAction::AdoptOsState(true),
            "a registered OS entry must adopt to autostart = on"
        );
        assert_eq!(
            reconcile(OsAutostartState::Unregistered, true),
            AutostartAction::AdoptOsState(false),
            "a missing OS entry must adopt to autostart = off"
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
        // Codex review finding 2, and now doubly load-bearing under BL-NI-18: a
        // failed/unknown OS query must NOT be adopted. Under the old policy an
        // untrusted read could have mutated OS state; under this one it could
        // silently switch the user's setting off on the strength of a query that
        // simply errored. Neither is acceptable, so Unknown stays inert and the
        // next launch reads again.
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
