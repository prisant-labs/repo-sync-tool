//! updates (edge) - the app self-update half of E-18 (auto-update and distribution).
//!
//! Owning effort: E-18 (auto-update and distribution).
//!
//! This is the thin Tauri edge over `tauri-plugin-updater`: the check/download/
//! verify/install loop the Tauri-free core cannot own. reposync-core stays
//! Tauri-free; only this module (plus the two `app_*` command wrappers and the
//! on-launch check spawned in `lib.rs`) touches the plugin. The integrity boundary
//! is the plugin's minisign signature verification against the public key baked
//! into the binary (`plugins.updater.pubkey` in `tauri.conf.json`): a downloaded
//! artifact is verified BEFORE it replaces the running binary, so a bad signature
//! never installs.
//!
//! Two callers, one typed path: the Settings "Check for updates" button and the
//! on-launch check both route through [`check`] / [`install`], so the
//! `auto_update_check` toggle gate and the ship-dark gate live in one place.
//!
//! Ship-dark: the production signing keypair is human-only (jp generates it once;
//! the private key lives only in CI secrets). Until a real production pubkey is
//! configured, the updater ships DARK - fully wired but disabled - and every check
//! reports the gentle "could not reach the update server" outcome without touching
//! the network. See [`updater_is_live`] and the E-18 spec.

use tauri::AppHandle;
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_updater::UpdaterExt;

use reposync_core::error::AppError;
use reposync_core::ipc::UpdateAvailability;

/// The ship-dark sentinel committed in `tauri.conf.json` `plugins.updater.pubkey`.
///
/// While this exact value (or an empty string) is the configured pubkey, the
/// updater is DARK: jp has not yet generated the production keypair, so there is no
/// real signature key and update checks are disabled. jp replaces this with the
/// production public key at activation (see the E-18 spec's ship-dark fallback and
/// the runbook's public-flip checklist); [`updater_is_live`] then returns true and
/// the updater goes live on the next release.
pub const UPDATER_PUBKEY_PLACEHOLDER: &str =
    "SHIP_DARK__no_production_updater_key_yet__see_E-18_public_flip";

/// The ship-dark decision (pure, so it is unit-tested without a Tauri harness): the
/// updater is LIVE only when a real production pubkey is configured. An empty or
/// placeholder pubkey means the production keypair does not exist yet, so the
/// updater is wired but dark. This is the one place the "no production key -> ship
/// dark" rule is decided.
pub fn updater_is_live(pubkey: &str) -> bool {
    let key = pubkey.trim();
    !key.is_empty() && key != UPDATER_PUBKEY_PLACEHOLDER
}

/// Whether the on-launch update check should run, given the persisted
/// `auto_update_check` toggle and whether the updater is live (pure, unit-tested).
/// The launch check runs only when the user has it enabled AND a production key is
/// configured; a manual "Check for updates" ignores the toggle and is not gated
/// here. Combining both gates keeps the launch path from spawning a doomed check on
/// a ship-dark build.
pub fn should_run_launch_check(auto_update_check: bool, updater_live: bool) -> bool {
    auto_update_check && updater_live
}

/// The configured `plugins.updater.pubkey` from the running app's resolved config,
/// or `""` when the updater is not configured at all. Read at runtime (not at
/// compile time) so activation is a config-only change with no code edit.
pub fn configured_pubkey(app: &AppHandle) -> String {
    app.config()
        .plugins
        .0
        .get("updater")
        .and_then(|updater| updater.get("pubkey"))
        .and_then(|pubkey| pubkey.as_str())
        .unwrap_or("")
        .to_string()
}

/// The mapped result of a plugin update check, decoupled from the plugin type so
/// [`build_availability`] is a pure, unit-testable function (the real `Update` type
/// cannot be constructed in a unit test).
enum CheckOutcome {
    /// The server was reachable and the running version is current.
    UpToDate,
    /// A newer signed release is available.
    Available {
        version: String,
        notes: Option<String>,
    },
    /// The update server could not be reached (offline, the inert private-repo 404,
    /// or ship-dark). Rendered gently as "could not reach the update server."
    Unreachable,
}

/// Build the typed [`UpdateAvailability`] payload from the current version and a
/// mapped check outcome (pure). The three UI states are distinguished WITHOUT
/// throwing: available, up to date (no error), and unreachable (error present).
fn build_availability(current_version: String, outcome: CheckOutcome) -> UpdateAvailability {
    match outcome {
        CheckOutcome::UpToDate => UpdateAvailability {
            current_version,
            available: false,
            new_version: None,
            notes: None,
            error: None,
        },
        CheckOutcome::Available { version, notes } => UpdateAvailability {
            current_version,
            available: true,
            new_version: Some(version),
            notes,
            error: None,
        },
        // The gentle "couldn't reach the server" bucket. net.offline is the closest
        // frozen code; the Settings UI renders its own fixed copy off `error != null`
        // rather than the payload message, so the exact variant is not user-visible.
        CheckOutcome::Unreachable => UpdateAvailability {
            current_version,
            available: false,
            new_version: None,
            notes: None,
            error: Some(AppError::Offline.to_payload()),
        },
    }
}

/// Map a plugin error from the CHECK path to the gentle unreachable bucket. Any
/// failure to reach or parse the update channel (offline, the private-repo 404, a
/// malformed manifest) is a non-alarming "could not reach the update server," logged
/// with detail.
fn map_check_error(e: &tauri_plugin_updater::Error) -> AppError {
    eprintln!("updater: update check could not reach the server: {e}");
    AppError::Offline
}

/// Map a plugin error from the INSTALL path. A verification/download/install failure
/// leaves the current version intact (`download_and_install` verifies the minisign
/// signature BEFORE replacing the binary), and is surfaced as an internal error
/// carrying the cause; the Settings UI renders "update could not be verified; staying
/// on your current version."
fn map_install_error(e: tauri_plugin_updater::Error) -> AppError {
    eprintln!("updater: install/verify failed (current version retained): {e}");
    AppError::Unexpected {
        context: format!("update could not be verified or installed: {e}"),
    }
}

/// Run the plugin update check and map it to a typed [`UpdateAvailability`]. Never
/// throws: reachable-but-no-update and unreachable are distinct payload states so the
/// UI renders "up to date" vs "couldn't reach the server" correctly. Ship-dark
/// short-circuits to unreachable without any network call.
pub async fn check(app: &AppHandle) -> UpdateAvailability {
    let current_version = app.package_info().version.to_string();

    if !updater_is_live(&configured_pubkey(app)) {
        eprintln!(
            "updater: ships dark (no production signing key configured); update check skipped"
        );
        return build_availability(current_version, CheckOutcome::Unreachable);
    }

    let updater = match app.updater() {
        Ok(updater) => updater,
        Err(e) => {
            eprintln!("updater: could not build the updater ({e}); reporting unreachable");
            return build_availability(current_version, CheckOutcome::Unreachable);
        }
    };

    match updater.check().await {
        // The plugin only returns Some when the manifest version is semver-greater
        // than the running version, so downgrade protection (AC8) is enforced here.
        Ok(Some(update)) => build_availability(
            current_version,
            CheckOutcome::Available {
                version: update.version.clone(),
                notes: update.body.clone(),
            },
        ),
        Ok(None) => build_availability(current_version, CheckOutcome::UpToDate),
        Err(e) => {
            let _ = map_check_error(&e);
            build_availability(current_version, CheckOutcome::Unreachable)
        }
    }
}

/// Download, verify, and install the pending update, then relaunch. Called ONLY
/// after the user confirms. Re-runs the check to obtain a fresh `Update` (the plugin
/// `Update` handle is not serializable and cannot cross IPC), so the command is
/// stateless with no managed pending-update slot to race. A verification/download
/// failure returns a typed [`AppError`] and leaves the current version intact.
pub async fn install(app: &AppHandle) -> Result<(), AppError> {
    if !updater_is_live(&configured_pubkey(app)) {
        // Ship-dark: nothing to install. Reported as the same gentle bucket.
        return Err(AppError::Offline);
    }

    let updater = app.updater().map_err(|e| map_check_error(&e))?;
    let update = updater
        .check()
        .await
        .map_err(|e| map_check_error(&e))?
        .ok_or_else(|| AppError::Unexpected {
            context: "no pending update to install".into(),
        })?;

    // The plugin verifies the minisign signature inside download_and_install BEFORE
    // the running binary is replaced; a failure aborts and leaves the current
    // version untouched (AC2/AC8). Progress callbacks are no-ops: the install is a
    // one-shot confirm-then-install with no in-app progress bar in V1.
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(map_install_error)?;

    // Relaunch into the freshly-installed version. `restart()` diverges on every
    // supported target (it re-execs the process), so the Ok below is only reached in
    // the degenerate case where the platform returned from restart.
    app.restart();
    #[allow(unreachable_code)]
    Ok(())
}

/// The on-launch update check (spawned in `lib.rs` setup when
/// [`should_run_launch_check`] is true). Runs [`check`] once in the background; if an
/// update is available, surfaces a NON-BLOCKING prompt via a single OS toast (reusing
/// the E-14 notification plugin - no new frozen event) inviting the user to Settings
/// to review and install. Never auto-installs. Unreachable/up-to-date is log-only, so
/// a cold start on the inert private endpoint raises no error toast (AC8).
pub async fn run_launch_check(app: &AppHandle) {
    let availability = check(app).await;
    if availability.available {
        if let Some(version) = availability.new_version.as_deref() {
            notify_update_available(app, version);
        }
    }
}

/// Raise the single non-blocking "update available" OS toast for the launch check.
/// Best-effort: a toast failure is logged and swallowed, exactly like the E-14
/// notification edge.
fn notify_update_available(app: &AppHandle, version: &str) {
    if let Err(e) = app
        .notification()
        .builder()
        .title("RepoSync update available")
        .body(format!(
            "Version {version} is available. Open Settings > Updates to review and install."
        ))
        .show()
    {
        eprintln!("updater: could not raise the update-available toast: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The disposable-test-key sentinel that lives ONLY in the TEST-ONLY E2E overlay
    /// (`tauri.updater-e2e.conf.json`). The E2E script replaces it at runtime with a
    /// freshly-generated throwaway public key; it must NEVER appear in the committed
    /// production `tauri.conf.json`.
    const UPDATER_E2E_TEST_PUBKEY_SENTINEL: &str = "DISPOSABLE_TEST_UPDATER_PUBKEY_E2E_ONLY";

    /// Markers that must NEVER appear in the committed production `tauri.conf.json` -
    /// they belong only in the test-only E2E overlay: the plain-http transport opt-in
    /// (`dangerousInsecureTransportProtocol`) and the disposable test pubkey sentinel.
    /// If either leaked into the production config a shipped build could accept updates
    /// over insecure transport or trust a throwaway key. (A `http://localhost` endpoint
    /// is deliberately NOT a marker: the production config's `build.devUrl` is
    /// legitimately `http://localhost`, and without `dangerousInsecureTransportProtocol`
    /// Tauri enforces TLS on updater endpoints at runtime anyway, so a stray http
    /// updater endpoint could not serve updates in production regardless.) The pre-tag
    /// release gate (the runbook) greps for the same markers; the test below is the
    /// deterministic in-suite mirror.
    const FORBIDDEN_PRODUCTION_UPDATER_MARKERS: [&str; 2] = [
        "dangerousInsecureTransportProtocol",
        UPDATER_E2E_TEST_PUBKEY_SENTINEL,
    ];

    /// The committed production config carries none of the test-only updater markers.
    fn config_is_production_clean(config_text: &str) -> bool {
        !FORBIDDEN_PRODUCTION_UPDATER_MARKERS
            .iter()
            .any(|marker| config_text.contains(marker))
    }

    #[test]
    fn updater_is_live_only_with_a_real_pubkey() {
        // Ship-dark: empty and the placeholder sentinel are NOT live; a real-looking
        // key is. This is the "ship-dark decision when no pubkey is configured" gate.
        assert!(!updater_is_live(""));
        assert!(!updater_is_live("   "));
        assert!(!updater_is_live(UPDATER_PUBKEY_PLACEHOLDER));
        assert!(
            updater_is_live("dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk"),
            "a real minisign-style key must read as live"
        );
    }

    #[test]
    fn launch_check_runs_only_when_enabled_and_live() {
        // The launch check fires only when the toggle is on AND the updater is live.
        assert!(should_run_launch_check(true, true));
        assert!(
            !should_run_launch_check(false, true),
            "toggle off suppresses the launch check even when live"
        );
        assert!(
            !should_run_launch_check(true, false),
            "ship-dark suppresses the launch check even when the toggle is on"
        );
        assert!(!should_run_launch_check(false, false));
    }

    #[test]
    fn build_availability_distinguishes_the_three_states() {
        let current = "0.9.0".to_string();

        let up_to_date = build_availability(current.clone(), CheckOutcome::UpToDate);
        assert!(!up_to_date.available);
        assert!(up_to_date.error.is_none(), "up to date carries no error");
        assert!(up_to_date.new_version.is_none());

        let available = build_availability(
            current.clone(),
            CheckOutcome::Available {
                version: "0.9.1".into(),
                notes: Some("Fixes.".into()),
            },
        );
        assert!(available.available);
        assert_eq!(available.new_version.as_deref(), Some("0.9.1"));
        assert!(available.error.is_none());

        let unreachable = build_availability(current, CheckOutcome::Unreachable);
        assert!(!unreachable.available, "unreachable never claims an update");
        assert!(
            unreachable.error.is_some(),
            "unreachable carries an error so the UI shows 'couldn't reach the server', not 'up to date'"
        );
    }

    #[test]
    fn config_hygiene_rejects_each_test_only_marker() {
        // A clean production-shaped config passes.
        let clean = r#"{ "plugins": { "updater": { "pubkey": "REALKEY", "endpoints": ["https://example.com/latest.json"] } } }"#;
        assert!(config_is_production_clean(clean));

        // A production-shaped config with a legitimate localhost devUrl still passes
        // (localhost is deliberately not a marker; devUrl is legitimately localhost).
        assert!(config_is_production_clean(
            r#"{ "build": { "devUrl": "http://localhost:1420" }, "plugins": { "updater": { "pubkey": "REALKEY" } } }"#
        ));

        // Each forbidden marker fails the gate.
        assert!(!config_is_production_clean(
            r#"{ "plugins": { "updater": { "dangerousInsecureTransportProtocol": true } } }"#
        ));
        assert!(!config_is_production_clean(&format!(
            r#"{{ "plugins": {{ "updater": {{ "pubkey": "{UPDATER_E2E_TEST_PUBKEY_SENTINEL}" }} }} }}"#
        )));
    }

    #[test]
    fn production_tauri_conf_has_no_test_only_updater_markers() {
        // Deterministic in-suite config-hygiene gate: the committed production config
        // must carry NEITHER the insecure-transport opt-in NOR the disposable test
        // pubkey - both belong only in the E2E overlay. include_str! ties this to the
        // real file, so it fails the instant the production config gains a forbidden
        // marker (mirrors the pre-tag grep documented in the runbook).
        let cfg = include_str!("../tauri.conf.json");
        assert!(
            config_is_production_clean(cfg),
            "production tauri.conf.json must not contain any test-only updater marker"
        );
    }
}
