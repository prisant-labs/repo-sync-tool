//! autostart (edge) - the OS launch-on-login actuation + start-minimized half of
//! E-15 (autostart / launch on login).
//!
//! Owning effort: E-15 (autostart), edge-wiring portion (AC1, AC3, AC4).
//!
//! The PURE decisions live in the Tauri-free [`reposync_core::autostart`] core:
//! [`reconcile`](reposync_core::autostart::reconcile) decides how to make the OS
//! launch-on-login registration match the persisted `autostart` setting on startup
//! (AC2, over a tri-state OS read where a failed query is non-actuating), and
//! [`is_autostart_launch`](reposync_core::autostart::is_autostart_launch) decides
//! whether THIS process was launched by the registration (AC3, a whole-argument
//! match so a repo path can never false-positive). This edge module is the thin
//! actuator the core cannot be: it queries + drives `tauri-plugin-autostart`
//! (`is_enabled` / `enable` / `disable`), owns the launch-argument string, and
//! performs the start-minimized window action. reposync-core stays Tauri-free: the
//! plugin calls and the window handle live only here.
//!
//! Two actuation sites:
//!   * startup reconciliation ([`reconcile_on_launch`]), best-effort and never
//!     fatal - a plugin hiccup logs and is swallowed. The OS registration is the
//!     source of truth here (BL-NI-18): a confirmed disagreement updates the
//!     SETTING rather than re-registering, so a launch-on-login entry removed
//!     outside RepoSync stays removed. A failed persist leaves the disagreement
//!     intact, so the next launch tries again; and
//!   * the live `settings_set` toggle ([`apply`]), which surfaces a real failure as
//!     a structured [`AppError`] so the UI can toast honestly instead of falsely
//!     reporting success (the same persist-then-apply contract the git-path swap
//!     uses).

use tauri::AppHandle;
use tauri_plugin_autostart::ManagerExt;

use reposync_core::autostart::{reconcile, AutostartAction, OsAutostartState};
use reposync_core::error::AppError;

/// The launch argument the autostart registration adds to RepoSync's command line
/// (configured on the plugin in `lib.rs::run`). Its presence in argv is how an
/// autostart launch is told from a normal one; the pure check is the core's
/// [`is_autostart_launch`](reposync_core::autostart::is_autostart_launch), and an
/// autostart launch starts hidden to the tray (AC3, [`launched_by_autostart`] +
/// the hide in `lib.rs` setup).
///
/// HANDOFF (P3-C, tray completion / close-to-tray): the flag is owned here so the
/// two never drift. P3-C can consume this SAME constant and the core detector
/// (via [`launched_by_autostart`]) if it needs to know whether the process was
/// autostarted - e.g. to seed close-to-tray state, or to flip the main window to
/// `visible: false` by default and show it explicitly on a non-autostart launch so
/// the autostart-launch hide has no startup flash. E-15 currently hides the
/// config-visible window post-setup, which is functional but can briefly flash;
/// removing that flash is a window-lifecycle refinement that belongs with P3-C's
/// window/tray ownership, not here.
pub const AUTOSTART_LAUNCH_FLAG: &str = "--autostart";

/// Whether THIS process was launched by the autostart registration (AC3), by asking
/// the Tauri-free core detector to look for [`AUTOSTART_LAUNCH_FLAG`] in the real
/// argv. One call site for the flag string so the edge (and P3-C) never reimplement
/// the detection or drift on the flag.
pub fn launched_by_autostart() -> bool {
    let args: Vec<String> = std::env::args().collect();
    reposync_core::autostart::is_autostart_launch(&args, AUTOSTART_LAUNCH_FLAG)
}

/// Map the plugin's `is_enabled()` result to the core's tri-state
/// [`OsAutostartState`]. The caller reduces the result to `Option<bool>` first
/// (`Some(true/false)` for a confirmed read, `None` for a failed/errored one). Pure,
/// so the mapping is unit-tested with no Tauri runtime. A `None` reads as `Unknown`,
/// which is exactly what makes a failed OS query non-actuating in [`reconcile`] (the
/// core never mutates OS state from an untrusted read).
fn os_state_from_is_enabled(is_enabled: Option<bool>) -> OsAutostartState {
    match is_enabled {
        Some(true) => OsAutostartState::Registered,
        Some(false) => OsAutostartState::Unregistered,
        None => OsAutostartState::Unknown,
    }
}

/// Reconcile the persisted `autostart` setting with the OS launch-on-login
/// registration on startup (AC2, policy amended by BL-NI-18).
///
/// **The OS wins.** A confirmed registration state that disagrees with the setting
/// updates the SETTING; the registration is not touched. RepoSync used to do the
/// opposite and silently re-register at every launch, which meant a user who
/// removed the entry in Task Manager found it back the next morning with nothing
/// said. See [`reconcile`] for the full rationale and the accepted risk.
///
/// Best-effort and NEVER fatal, in three separate ways:
///   * a failed OS query reads as `Unknown`, and the core declines to adopt from an
///     observation it does not trust;
///   * settings that could not be read at startup skip reconciliation entirely,
///     because there is no trustworthy value to compare against and writing the
///     row back would persist defaults over the user's real configuration; and
///   * a failed persist is logged and swallowed. The setting still disagrees, so
///     the next launch reaches the same conclusion and tries again.
///
/// Takes the settings snapshot the caller already read at startup rather than
/// re-reading, so the value compared is the same one the rest of startup used.
/// No lock is taken: this runs inside the Tauri setup closure, before the webview
/// exists, so no IPC settings write can be in flight.
pub async fn reconcile_on_launch(
    app: &AppHandle,
    pool: &sqlx::SqlitePool,
    startup_settings: Option<&reposync_core::ipc::Settings>,
) {
    let Some(settings) = startup_settings else {
        tracing::warn!(
            "autostart: skipping launch reconciliation - settings could not be read,              so there is no trustworthy value to reconcile against"
        );
        return;
    };

    let manager = app.autolaunch();
    let os = os_state_from_is_enabled(manager.is_enabled().ok());
    let AutostartAction::AdoptOsState(os_on) = reconcile(os, settings.autostart) else {
        // Already aligned, or an Unknown (untrusted) read the core refused to
        // act on.
        return;
    };

    // A one-column UPDATE, not a full-row write of the startup snapshot: another
    // process sharing this database (BL-NI-73) could have saved newer settings
    // since the snapshot was taken, and rewriting the row would silently restore
    // the stale values along with the adopted one.
    match reposync_core::store::settings_set_autostart(pool, os_on).await {
        Ok(()) => tracing::info!(
            event = reposync_core::logging::event::AUTOSTART_ADOPTED_OS_STATE,
            autostart = os_on,
            "autostart: the OS launch-on-login registration disagreed with the              setting, so the setting was updated to match the OS (the OS is              authoritative on startup). Something outside RepoSync changed the              registration while it was not running."
        ),
        Err(e) => tracing::warn!(
            event = reposync_core::logging::event::AUTOSTART_ADOPT_PERSIST_FAILED,
            autostart = os_on,
            "autostart: could not persist the adopted OS state, so the setting              still disagrees with the OS and the next launch will retry: {e}"
        ),
    }
}

/// What the live-apply path should do to the OS registration for an explicit user
/// toggle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ApplyAction {
    /// The OS already matches the desired setting: nothing to do.
    Noop,
    /// Register launch-on-login.
    Enable,
    /// Remove launch-on-login.
    Disable,
}

/// The live-apply decision for `settings_set` when the user toggles `autostart`:
/// given the current OS registration (`Some(bool)` for a confirmed read, `None` when
/// the query failed) and the DESIRED setting, decide whether to enable, disable, or
/// do nothing. Pure, so it is unit-tested.
///
/// This DIFFERS from [`reconcile`] on the failed-read (`None`) case ON PURPOSE:
/// startup reconciliation must not actuate on an untrusted read (safety - a bad
/// query must never mutate OS state), but a live toggle is the user's EXPLICIT
/// intent, so a failed read still attempts the actuation toward the setting and lets
/// the real plugin error surface. The `Some(c) if c == desired -> Noop` arm keeps
/// the apply idempotent and, on Windows, avoids a spurious "value not found" error
/// from `disable()` when the Run key is already absent (e.g. after external
/// tampering that the changed-guard alone would not catch).
fn apply_action(current: Option<bool>, desired: bool) -> ApplyAction {
    match current {
        Some(c) if c == desired => ApplyAction::Noop,
        _ if desired => ApplyAction::Enable,
        _ => ApplyAction::Disable,
    }
}

/// Apply the `autostart` setting live (the `settings_set` path, AC1): register or
/// remove launch-on-login to match `enabled`, idempotently.
///
/// FAILURE CONTRACT (matches the git-path swap in `settings_set`, commit 71a0f7b):
/// this is PERSIST-THEN-APPLY. The caller has already persisted the new `autostart`
/// value before calling this, so on a plugin failure we return a structured
/// [`AppError::InvalidSetting`] on the `autostart` field (the UI toasts an honest
/// failure instead of a false "Settings saved") WITHOUT rolling back the stored
/// value - unrelated settings saved in the same write are not lost, and
/// [`reconcile_on_launch`] retries the actuation on the next launch, so the stored
/// setting self-heals. `field: "autostart"` is reused rather than a new error
/// variant because the [`AppError`] taxonomy is frozen at 30 variants; the value
/// itself is valid (both true/false are legal), so `InvalidSetting` reads as "could
/// not activate this setting", exactly as the git-path precedent uses it.
pub fn apply(app: &AppHandle, enabled: bool) -> Result<(), AppError> {
    let manager = app.autolaunch();
    match apply_action(manager.is_enabled().ok(), enabled) {
        ApplyAction::Noop => Ok(()),
        ApplyAction::Enable => manager.enable().map_err(|e| apply_error("register", e)),
        ApplyAction::Disable => manager.disable().map_err(|e| apply_error("remove", e)),
    }
}

/// Log a live-apply plugin failure and reduce it to the structured, UI-honest
/// [`AppError::InvalidSetting`] on the `autostart` field (see [`apply`]).
fn apply_error(verb: &str, e: impl std::fmt::Display) -> AppError {
    tracing::warn!("autostart: failed to {verb} launch-on-login: {e}");
    AppError::InvalidSetting {
        field: "autostart".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_state_maps_the_is_enabled_query_to_the_core_tristate() {
        // A confirmed read maps to the confirmed core states; a failed query (None)
        // maps to Unknown, which `reconcile` treats as non-actuating.
        assert_eq!(
            os_state_from_is_enabled(Some(true)),
            OsAutostartState::Registered
        );
        assert_eq!(
            os_state_from_is_enabled(Some(false)),
            OsAutostartState::Unregistered
        );
        assert_eq!(os_state_from_is_enabled(None), OsAutostartState::Unknown);
    }

    #[test]
    fn apply_action_is_a_noop_when_the_os_already_matches() {
        // Idempotent: re-applying a setting the OS already reflects does nothing
        // (and so never trips a spurious disable-of-an-absent-key error).
        assert_eq!(apply_action(Some(true), true), ApplyAction::Noop);
        assert_eq!(apply_action(Some(false), false), ApplyAction::Noop);
    }

    #[test]
    fn apply_action_actuates_toward_the_desired_setting_on_a_mismatch() {
        assert_eq!(apply_action(Some(false), true), ApplyAction::Enable);
        assert_eq!(apply_action(Some(true), false), ApplyAction::Disable);
    }

    #[test]
    fn apply_action_attempts_actuation_on_a_failed_read() {
        // The live-apply path honors the user's EXPLICIT toggle even when the OS
        // query failed (None): it tries the actuation toward the setting (and lets a
        // real plugin error surface), unlike startup `reconcile`, which declines to
        // actuate on an untrusted read.
        assert_eq!(apply_action(None, true), ApplyAction::Enable);
        assert_eq!(apply_action(None, false), ApplyAction::Disable);
    }
}
