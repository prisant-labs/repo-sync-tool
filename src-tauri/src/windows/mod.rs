//! Window lifecycle for the RepoSync shell: initial visibility + close-to-tray.
//!
//! Owning effort: E-01 (Foundation) for the stub; E-13 (tray native menu, P3-C)
//! wires the resident-utility window lifecycle here.
//!
//! RepoSync is a resident tray utility with one main window (created hidden
//! (`visible: false`) in Rust `setup()` per BL-NI-59). Two behaviors live here:
//!
//!   - **Initial visibility (E-15 AC3):** a NORMAL launch shows + focuses the window;
//!     an AUTOSTART launch leaves it hidden in the tray (the tray "Show RepoSync"
//!     item is the restore path). Declaring the window hidden and showing it
//!     explicitly on a normal launch avoids the startup flash the earlier
//!     hide-after-show approach could cause (see the handoff note on
//!     [`crate::autostart::AUTOSTART_LAUNCH_FLAG`]).
//!   - **Close-to-tray (E-13 AC3), user-configurable:** when the
//!     `close_minimizes_to_tray` setting is ON (the default), the window's close
//!     button HIDES it to the tray instead of exiting, and only the tray "Quit"
//!     item fully exits; when OFF, the close button exits the app. Read live from a
//!     shared AtomicBool so a Settings toggle takes effect with no restart.
//!
//! Both behaviors are GATED on a successfully built system tray (finding 2). Because
//! the tray is the only restore/quit path, hiding-on-close or starting-minimized
//! WITHOUT a tray would strand an invisible, unquittable app. So `init` is called
//! from `lib.rs` setup AFTER `tray::init`, threaded with whether the tray built: with
//! a tray it wires the resident lifecycle above; without one it falls back to a plain
//! window (never hide-on-close, never start hidden), so even an autostart launch ends
//! visible and quittable.

use tauri::{AppHandle, Manager, WindowEvent};

/// What [`init`] actually accomplished, reported back so the CALLER can decide what to
/// log. `init` itself never decides that, and never changes what the app does.
///
/// This exists because every one of these outcomes used to be silent: a missing main
/// window was a bare `return`, and `show`, `hide` and `set_focus` were all `let _ =`.
/// The app behaved identically in each case and nothing recorded which had happened, so
/// a launch that ended with no window on screen was indistinguishable in the log from a
/// perfect one. That is a false green for the binary smoke gate (BL-NI-88 - no gate ever
/// launches the built binary), which is why this is returned now rather than dropped.
///
/// What it deliberately does NOT do is change behavior. None of these conditions has
/// ever been fatal, and making one fatal now would risk bricking an install over
/// something nobody can reproduce. Every variant leaves the app running exactly as it
/// runs today; only the log differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WindowStartup {
    /// The lifecycle applied to a real window. The two flags record imperfections that
    /// do NOT make the app unusable, so they are reported and do not block readiness.
    Applied {
        /// Whether this launch was meant to stay hidden in the tray.
        start_hidden: bool,
        /// `set_focus()` failed. The window is up, it just is not in front.
        focus_failed: bool,
        /// `hide()` failed on an autostart launch, so the window is VISIBLE when it
        /// should have stayed in the tray. Wrong, and worth seeing in a log, but the
        /// app is entirely usable, and it lands in precisely the state the no-tray
        /// fallback in [`decide_window_lifecycle`] chooses on purpose.
        hide_failed: bool,
    },
    /// There is no `main` window to apply the lifecycle to. The app is running with no
    /// UI at all and no way to get one.
    MainWindowMissing,
    /// A launch that was supposed to show its window could not. The window is created
    /// hidden (`visible(false)`, BL-NI-59), so a failed `show()` means the user started
    /// the app and nothing appeared.
    ShowFailed(String),
}

impl WindowStartup {
    /// Whether this outcome left the user with a window they can actually use.
    ///
    /// The line between blocking and cosmetic is drawn at "can the person who just
    /// launched this app see and use it":
    ///
    ///   * [`WindowStartup::MainWindowMissing`] and [`WindowStartup::ShowFailed`] both
    ///     end with nothing on screen on a launch that was supposed to put something
    ///     there. Not usable.
    ///   * A failed `set_focus` leaves a visible window that is merely not in front,
    ///     and focus is also the one operation a CI session can plausibly refuse for
    ///     reasons that say nothing about the build. Cosmetic.
    ///   * A failed `hide` on an autostart launch leaves the window VISIBLE. That is
    ///     the wrong behavior, but it is MORE visibility rather than less, and it is
    ///     the same end state [`decide_window_lifecycle`] deliberately chooses when
    ///     there is no tray. Blocking on it would fail the gate for a condition the
    ///     design already tolerates on purpose. Cosmetic.
    ///
    /// Pure, so the rule is unit-testable with no Tauri window or runtime.
    pub fn is_usable(&self) -> bool {
        matches!(self, WindowStartup::Applied { .. })
    }
}

/// Reconcile the main window's initial visibility with how the app was launched and
/// wire close-to-tray, gated on whether a system tray was successfully built
/// (`tray_available`). Called once from `lib.rs` setup AFTER `tray::init`.
///
/// Returns a [`WindowStartup`] describing what happened. Behavior is unchanged from
/// when this returned `()`: a missing main window is still a no-op for the app, and a
/// failed visibility call still never stops anything. The difference is that the caller
/// can now SEE which of those occurred and withhold the startup readiness marker.
pub fn init(
    app: &AppHandle,
    tray_available: bool,
    close_minimizes_to_tray: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> WindowStartup {
    // The one early return in this function, and the only one that is safe: there is no
    // window here, so there is nothing below to skip. Every other outcome falls through
    // to the single exit at the end, AFTER the close-to-tray wiring.
    let Some(window) = app.get_webview_window("main") else {
        return WindowStartup::MainWindowMissing;
    };

    let lifecycle =
        decide_window_lifecycle(tray_available, crate::autostart::launched_by_autostart());

    // Initial visibility (E-15 AC3). The window is config-declared hidden, so a normal
    // launch must explicitly show + focus it. Only an autostart launch WITH a tray to
    // restore from stays hidden; with no tray we always show, so an autostart launch
    // never ends up invisible with no way back (finding 2).
    //
    // The results are CAPTURED now rather than dropped. Capturing is the ONLY change:
    // every call below still runs exactly when it ran before, and a failure still stops
    // nothing. A `show` that failed is the difference between an app the user can see
    // and one they cannot, and it used to leave no trace anywhere.
    //
    // Nothing here returns early, and that is load-bearing rather than stylistic. A
    // failed `show` does NOT destroy the window: the window object is still there, it is
    // simply not visible, and the tray - which this entire module is built around as THE
    // restore path - can still surface it. Skipping the close-to-tray wiring below would
    // therefore leave a window the user CAN reach whose close button QUITS RepoSync,
    // while `close_minimizes_to_tray` is on and is the default. A resident sync utility
    // that silently stops syncing is strictly worse than the invisible window this
    // outcome exists to report. So the error is recorded and returned at the end.
    // (Written from the defect: an earlier revision of this change did return here.)
    let mut focus_failed = false;
    let mut hide_failed = false;
    let mut show_error: Option<String> = None;
    if lifecycle.start_hidden {
        hide_failed = window.hide().is_err();
    } else {
        // Both run unconditionally, exactly as they did when both were `let _ =`. A
        // failed `show` does not skip the `set_focus` under it either.
        show_error = window.show().err().map(|e| e.to_string());
        focus_failed = window.set_focus().is_err();
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
                // The flag is read FRESH on every close, not captured when the handler
                // was registered, so toggling the setting in Settings takes effect with
                // no restart. `tray_available` is passed as `true` because this handler
                // is only registered when `intercept_close` was true, which already
                // required a tray; `decide_close_action` restates that gate so the rule
                // lives in one tested place.
                let action = decide_close_action(
                    true,
                    close_minimizes_to_tray.load(std::sync::atomic::Ordering::Relaxed),
                );
                if action == CloseAction::HideToTray {
                    api.prevent_close();
                    let _ = hide_target.hide();
                }
                // CloseAction::Exit: do NOT prevent the close, so the window closes and
                // the app exits (this is the only window).
            }
        });
    }

    // The single exit. Everything above this line has run, including the close-to-tray
    // wiring, on every path that had a window at all.
    classify_startup(
        lifecycle.start_hidden,
        show_error,
        focus_failed,
        hide_failed,
    )
}

/// Fold what the visibility calls reported into the outcome the caller sees.
///
/// Pure, and split out of [`init`] for two reasons. It makes the mapping testable with
/// no Tauri runtime. More importantly it leaves `init` with exactly ONE exit point after
/// the close-to-tray wiring, so no later edit can reintroduce an early return that
/// silently skips it. That is not a hypothetical risk: an earlier revision of this change
/// returned as soon as `show` failed, which dropped the close handler and turned the
/// close button into "quit the app" for a window the tray could still restore.
fn classify_startup(
    start_hidden: bool,
    show_error: Option<String>,
    focus_failed: bool,
    hide_failed: bool,
) -> WindowStartup {
    match show_error {
        Some(error) => WindowStartup::ShowFailed(error),
        None => WindowStartup::Applied {
            start_hidden,
            focus_failed,
            hide_failed,
        },
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

/// What the window's close (X) button should do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseAction {
    /// Keep running: prevent the close and hide the window to the tray.
    HideToTray,
    /// Let the close proceed. This is the only window, so it exits the app.
    Exit,
}

/// Pure decision for the close button, so the setting's effect is unit-testable
/// without a Tauri window/runtime.
///
/// `tray_available` is a HARD gate that the user setting cannot override: hiding is
/// only safe when a tray exists to restore and quit from, so with no tray the close
/// button exits regardless of the setting rather than stranding an invisible,
/// unquittable app (finding 2). The setting is only consulted once that is satisfied.
fn decide_close_action(tray_available: bool, close_minimizes_to_tray: bool) -> CloseAction {
    if tray_available && close_minimizes_to_tray {
        CloseAction::HideToTray
    } else {
        CloseAction::Exit
    }
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

    #[test]
    fn close_hides_to_tray_only_when_the_setting_is_on() {
        assert_eq!(
            decide_close_action(true, true),
            CloseAction::HideToTray,
            "setting ON with a tray: close hides and the app keeps running"
        );
        assert_eq!(
            decide_close_action(true, false),
            CloseAction::Exit,
            "setting OFF: close exits the app"
        );
    }

    /// The safety invariant: `tray_available` is a HARD gate the user setting cannot
    /// override. Without a tray there is no restore or quit path, so honoring
    /// "minimize to tray" would strand an invisible, unquittable app - the same
    /// finding-2 hazard `decide_window_lifecycle` guards, restated for the setting.
    #[test]
    fn close_exits_without_a_tray_even_when_the_setting_is_on() {
        assert_eq!(
            decide_close_action(false, true),
            CloseAction::Exit,
            "no tray: close must exit even with minimize-to-tray ON"
        );
        assert_eq!(decide_close_action(false, false), CloseAction::Exit);
    }

    #[test]
    fn only_a_window_the_user_cannot_see_blocks_readiness() {
        // The two outcomes that leave nothing on screen block the readiness marker.
        assert!(!WindowStartup::MainWindowMissing.is_usable());
        assert!(!WindowStartup::ShowFailed("os error".to_string()).is_usable());

        // A window that came up is usable, cosmetic imperfections included.
        assert!(WindowStartup::Applied {
            start_hidden: false,
            focus_failed: false,
            hide_failed: false,
        }
        .is_usable());
        assert!(
            WindowStartup::Applied {
                start_hidden: false,
                focus_failed: true,
                hide_failed: false,
            }
            .is_usable(),
            "an unfocused window is still a window the user can see and click"
        );
        // The interesting one. An autostart launch that ends VISIBLE is wrong, but it
        // is MORE visibility rather than less, and it is the same end state
        // `decide_window_lifecycle` picks on purpose when there is no tray. Making it
        // block would fail a CI gate for a condition the design already tolerates.
        assert!(
            WindowStartup::Applied {
                start_hidden: true,
                focus_failed: false,
                hide_failed: true,
            }
            .is_usable(),
            "a hide that failed leaves a usable app, just a more visible one"
        );
    }

    #[test]
    fn a_failed_show_is_reported_without_discarding_the_other_results() {
        // `show` failing wins the outcome, and it does so at the END of `init` rather
        // than by returning early. The proof available to a unit test is that the other
        // results are still gathered around it: `classify_startup` is only reachable
        // with a `focus_failed` value, which is read from a `set_focus` call made AFTER
        // the failing `show`, on the same path that then runs the close-to-tray wiring.
        assert_eq!(
            classify_startup(false, Some("os error 5".to_string()), true, false),
            WindowStartup::ShowFailed("os error 5".to_string())
        );
    }

    #[test]
    fn a_clean_run_reports_the_flags_it_gathered() {
        assert_eq!(
            classify_startup(false, None, false, false),
            WindowStartup::Applied {
                start_hidden: false,
                focus_failed: false,
                hide_failed: false,
            }
        );
        assert_eq!(
            classify_startup(true, None, false, true),
            WindowStartup::Applied {
                start_hidden: true,
                focus_failed: false,
                hide_failed: true,
            },
            "an autostart launch whose hide failed still reports as applied"
        );
    }

    /// The setting is read from a shared `AtomicBool` on every close rather than
    /// captured once when the handler is registered, so a Settings toggle takes
    /// effect with no restart. This asserts the read-fresh contract: the same
    /// registered decision path yields a different action after a flip, with no
    /// re-registration.
    #[test]
    fn close_action_follows_a_live_setting_flip() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::Arc;

        let setting = Arc::new(AtomicBool::new(true));
        let handler_view = Arc::clone(&setting);
        // Stands in for the registered close handler: it holds only the Arc, and
        // re-reads it per close.
        let on_close = move || decide_close_action(true, handler_view.load(Ordering::Relaxed));

        assert_eq!(on_close(), CloseAction::HideToTray);
        setting.store(false, Ordering::Relaxed);
        assert_eq!(
            on_close(),
            CloseAction::Exit,
            "toggling the setting must take effect without re-registering the handler"
        );
        setting.store(true, Ordering::Relaxed);
        assert_eq!(on_close(), CloseAction::HideToTray, "and back again");
    }
}
