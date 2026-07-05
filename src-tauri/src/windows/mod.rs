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

use tauri::{AppHandle, Manager, WindowEvent};

/// Reconcile the main window's initial visibility with how the app was launched and
/// wire close-to-tray. Called once from `lib.rs` setup (no DB needed, so it can run
/// before the async pool init). A missing main window is a no-op.
pub fn init(app: &AppHandle) {
    let Some(window) = app.get_webview_window("main") else {
        return;
    };

    // Initial visibility (E-15 AC3). The window is config-declared hidden, so a normal
    // launch must explicitly show + focus it; an autostart launch stays hidden.
    if crate::autostart::launched_by_autostart() {
        let _ = window.hide();
    } else {
        let _ = window.show();
        let _ = window.set_focus();
    }

    // Close-to-tray (E-13 AC3): intercept the close request so the close button hides
    // the window and the app keeps running in the tray. Only the tray "Quit" item
    // (which calls `app.exit`) is the real exit. A clone of the window handle is moved
    // into its own event handler (the handler cannot borrow the window it is attached
    // to).
    let hide_target = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            let _ = hide_target.hide();
        }
    });
}
