//! The startup singleton: what happens when RepoSync is launched a second time.
//!
//! Owning item: BL-NI-73 (nothing stops two RepoSync instances sharing one database),
//! in `docs/backlog.md`.
//!
//! ## Why a guard at all, and why it must be per-APPLICATION
//!
//! Before this, launching RepoSync twice gave you two processes over one data
//! directory, and the damage came in two halves. The database half is the visible one:
//! `db.rs` opens SQLite in WAL mode, which is built for shared access, so two
//! schedulers happily interleave activity rows and race every read-modify-write on
//! `repo_local_state` - including the failure counter BL-NI-72 (stale failure count)
//! fixed WITHIN one process.
//!
//! The working-tree half is the dangerous one. `RepoLocks` is an in-memory
//! `Arc<HashMap<..>>`, so the per-repo mutex that serializes a manual check against a
//! scheduled one serializes ONE PROCESS. Two instances run two `git` processes in the
//! SAME working tree at once, which is precisely the condition that lock exists to
//! prevent and which can corrupt an index.
//!
//! That second half is why the guard is keyed on the APPLICATION and not on the data
//! directory. Keying on the data directory is tempting - it would let two instances
//! with separate databases coexist - and it fixes only the half that matters least.
//! Two instances with separate databases can still have the same repositories
//! registered, and then they run concurrent `git` in the same working trees with
//! nothing between them at all. `tauri-plugin-single-instance` keys on the bundle
//! identifier (`com.reposync.app`), so the guard is per-application, which is the
//! scope the harm actually has.
//!
//! ## What the plugin does, and what this module owns
//!
//! The plugin is registered FIRST in `lib.rs`, which is load-bearing (see the comment
//! at the registration site). Everything below is the plugin's, not ours: the second
//! process creates a named mutex `com.reposync.app-sim`, finds `ERROR_ALREADY_EXISTS`,
//! sends its argv and cwd to the running instance over `WM_COPYDATA`, and leaves via
//! `std::process::exit(0)` from inside the plugin's own setup hook. On macOS the same
//! handshake runs over a unix socket in `/tmp`. No entitlement, `Info.plist` key, or
//! `capabilities/default.json` permission is involved: the plugin exposes no JS command
//! surface.
//!
//! This module owns exactly one thing - the CALLBACK, which runs in the FIRST instance.
//!
//! ## Two limitations, recorded rather than papered over
//!
//! * On Windows the second instance only exits if it can FIND the running instance's
//!   message window. A launch landing between the first instance's `CreateMutexW` and
//!   its window creation (sub-millisecond, at startup) finds no window and proceeds as
//!   a full second instance. That is the plugin's behavior; closing it at the app layer
//!   would need a second guard of our own, which is a worse trade than the race.
//! * The mutex carries no `Global\` prefix, so it is scoped to the logon session. Two
//!   Windows users signed in at once each get their own RepoSync, which is correct:
//!   each has their own `%LOCALAPPDATA%` and therefore their own database and working
//!   trees.

use tauri::AppHandle;

use crate::windows::RaiseOutcome;

/// Called in the RUNNING instance when someone launches RepoSync again.
///
/// Two jobs, in this order: put the window the user asked for in front of them, and
/// leave a durable record that the guard fired.
///
/// **Showing the window is right even when this instance is hidden in the tray.**
/// Someone who launches the app again is asking to see it, and surfacing the existing
/// window is what a tray user expects a second launch to do. An autostart launch that
/// deliberately started hidden is no exception: the hiding was a startup policy, and
/// this is a fresh, explicit request.
///
/// **Nothing here is fatal.** A failed `show` or `set_focus` is recorded and logged,
/// exactly as `windows::init` does on the startup path, and never panics or exits. This
/// runs inside the running instance; a panic here would take down the app the user was
/// trying to reach, over the act of trying to reach it.
///
/// **It is also fast, and must stay that way.** On Windows this executes inside the
/// `WM_COPYDATA` window procedure while the second process is blocked in
/// `SendMessageW`, so any wait added here is a wait on a process that is trying to
/// exit. Unminimize, show, focus, one log line - nothing awaited, nothing on disk.
pub fn on_second_launch(app: &AppHandle, argv: Vec<String>, _cwd: String) {
    let outcome = crate::windows::raise_main_window(app);

    // ONE event name for both branches, unlike the startup path's
    // `app.startup_completed` / `app.window_setup_failed` pair. The question this line
    // answers is "did the guard turn a second launch away", and the answer is yes on
    // both branches; how well the window came up is a FIELD, not a different event.
    // The binary smoke gate asserts this name to tell a guard that fired apart from a
    // second process that merely died, and that assertion must not depend on the raise
    // having gone perfectly.
    //
    // `argv.len()`, never `argv` or `cwd`: both are filesystem paths, and this line
    // lands in a log a user may email to a maintainer. The count is enough to see that
    // a launcher passed something unexpected. On Windows the plugin includes the
    // executable path as the first element, so an ordinary double-click reports 1.
    let arg_count = argv.len();
    if raise_needs_a_warning(&outcome) {
        tracing::warn!(
            event = reposync_core::logging::event::APP_SECOND_INSTANCE_DEFERRED,
            arg_count,
            outcome = ?outcome,
            "a second launch was deferred to this instance, but its window could not be \
             brought to the front; the user asked for the app and may have seen nothing happen"
        );
    } else {
        tracing::info!(
            event = reposync_core::logging::event::APP_SECOND_INSTANCE_DEFERRED,
            arg_count,
            outcome = ?outcome,
            "a second launch was deferred to this instance; its window was brought to the front"
        );
    }
}

/// Whether the deferral line should be logged as a warning rather than as information.
///
/// Trivial by design, and extracted for one reason: it pins the rule that a second
/// launch classifies a raise EXACTLY as startup classifies a window
/// ([`RaiseOutcome::is_usable`]), so the two cannot drift into disagreeing about
/// whether an unfocused window is a problem.
fn raise_needs_a_warning(outcome: &RaiseOutcome) -> bool {
    !outcome.is_usable()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_window_the_user_cannot_see_makes_a_deferral_a_warning() {
        // Nothing on screen: the user launched the app and got nothing.
        assert!(raise_needs_a_warning(&RaiseOutcome::MainWindowMissing));
        assert!(raise_needs_a_warning(&RaiseOutcome::ShowFailed(
            "os error 5".to_string()
        )));

        // A window that came up is a success even with the cosmetic flags set. The
        // focus one is not hypothetical: on Windows the foreground right belongs to the
        // process the user just launched - the second instance - not to this one, so a
        // guard that works perfectly can still leave a flashing taskbar button. Logging
        // that as a warning would train a maintainer to ignore the line.
        assert!(!raise_needs_a_warning(&RaiseOutcome::Raised {
            unminimize_failed: false,
            focus_failed: false,
        }));
        assert!(!raise_needs_a_warning(&RaiseOutcome::Raised {
            unminimize_failed: true,
            focus_failed: true,
        }));
    }
}
