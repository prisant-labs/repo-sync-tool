//! Window lifecycle for the RepoSync shell: initial visibility + close-to-tray.
//!
//! Owning effort: E-01 (Foundation) for the stub; E-13 (tray native menu, P3-C)
//! wires the resident-utility window lifecycle here.
//!
//! RepoSync is a resident tray utility with one main window (declared
//! `visible: false` in `tauri.conf.json`). Two behaviors live here:
//!
//!   - **Initial visibility (E-15 AC3):** a NORMAL launch shows + focuses the window;
//!     an AUTOSTART launch leaves it hidden in the tray (the tray "Show RepoSync"
//!     item is the restore path). Declaring the window hidden and showing it
//!     explicitly on a normal launch avoids the startup flash the earlier
//!     hide-after-show approach could cause (see the handoff note on
//!     [`crate::autostart::AUTOSTART_LAUNCH_FLAG`]).
//!   - **Close-to-tray (E-13 AC3):** the window's close button HIDES it to the tray
//!     instead of exiting; only the tray "Quit" item fully exits the app.
//!
//! Both behaviors are GATED on a successfully built system tray (finding 2). Because
//! the tray is the only restore/quit path, hiding-on-close or starting-minimized
//! WITHOUT a tray would strand an invisible, unquittable app. So `init` is called
//! from `lib.rs` setup AFTER `tray::init`, threaded with whether the tray built: with
//! a tray it wires the resident lifecycle above; without one it falls back to a plain
//! window (never hide-on-close, never start hidden), so even an autostart launch ends
//! visible and quittable.

use tauri::{AppHandle, Manager, WindowEvent};

/// Reconcile the main window's initial visibility with how the app was launched and
/// wire close-to-tray, gated on whether a system tray was successfully built
/// (`tray_available`). Called once from `lib.rs` setup AFTER `tray::init`. A missing
/// main window is a no-op.
pub fn init(app: &AppHandle, tray_available: bool) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    let lifecycle =
        decide_window_lifecycle(tray_available, crate::autostart::launched_by_autostart());

    // Initial visibility (E-15 AC3). The window is config-declared hidden, so a normal
    // launch must explicitly show + focus it. Only an autostart launch WITH a tray to
    // restore from stays hidden; with no tray we always show, so an autostart launch
    // never ends up invisible with no way back (finding 2).
    if lifecycle.start_hidden {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }

    // Close-to-tray (E-13 AC3): intercept the close request so the close button hides
    // the window and the app keeps running in the tray - but ONLY when a tray exists
    // as the restore/quit path. Without a tray we leave the default close (exit), so a
    // failed tray can never strand an invisible unquittable app (finding 2). A clone of
    // the window handle is moved into its own event handler (the handler cannot borrow
    // the window it is attached to).
    if lifecycle.intercept_close {
        let hide_target = window.clone();
        window.on_window_event(move |event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = hide_target.hide();
            }
        });
    }
}

/// The window-lifecycle decision for a resident tray utility (finding 2): given
/// whether a system tray was successfully built (the restore/quit path) and whether
/// THIS process was launched by autostart, decide the initial window visibility and
/// whether the close button hides-to-tray or exits.
///
/// Close-to-tray and start-minimized are only safe when a tray exists to restore or
/// quit from. If the tray failed to build, we must NOT start hidden (nothing could
/// re-show the window) and must NOT intercept the close (the close button must exit,
/// not strand an invisible unquittable app) - even an autostart launch then ends
/// VISIBLE and quittable rather than hidden and unreachable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct WindowLifecycle {
    /// Start hidden in the tray (an autostart launch WITH a working tray).
    start_hidden: bool,
    /// Intercept the close button to hide-to-tray instead of exiting.
    intercept_close: bool,
}

/// Pure decision behind [`init`], so the tray-available fallback is unit-testable
/// without a Tauri window/runtime.
fn decide_window_lifecycle(tray_available: bool, launched_by_autostart: bool) -> WindowLifecycle {
    WindowLifecycle {
        // Only stay hidden when a tray can restore the window; without a tray even an
        // autostart launch must still end visible.
        start_hidden: tray_available && launched_by_autostart,
        // Only hide-to-tray on close when a tray exists to quit/restore from.
        intercept_close: tray_available,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_lifecycle_without_tray_stays_visible_and_quittable() {
        // No tray = no restore path: never start hidden, never intercept close - even
        // for an autostart launch, which must still end VISIBLE and quittable so a
        // failed tray cannot strand an invisible unquittable app (finding 2).
        let normal = decide_window_lifecycle(false, false);
        assert!(!normal.start_hidden);
        assert!(!normal.intercept_close);

        let autostart = decide_window_lifecycle(false, true);
        assert!(
            !autostart.start_hidden,
            "no tray: an autostart launch must not start hidden"
        );
        assert!(
            !autostart.intercept_close,
            "no tray: the close button must exit, not hide"
        );
    }

    #[test]
    fn window_lifecycle_with_tray_honors_launch_mode() {
        // With a tray as the restore/quit path: a normal launch shows, an autostart
        // launch starts hidden, and the close button hides-to-tray in both cases.
        let normal = decide_window_lifecycle(true, false);
        assert!(!normal.start_hidden, "a normal launch shows the window");
        assert!(normal.intercept_close);

        let autostart = decide_window_lifecycle(true, true);
        assert!(
            autostart.start_hidden,
            "an autostart launch with a tray starts hidden"
        );
        assert!(autostart.intercept_close);
    }
}
