//! System tray icon and menu for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) built the stub; E-13 (tray native menu)
//! builds the real, complete tray.
//!
//! RepoSync is a resident tray-first utility: the always-available control surface
//! is the native right-click tray menu. This module builds the tray icon and its
//! full menu - Show RepoSync / Check All Now / Pause all (toggles to Resume all) /
//! Open recent (a submenu of recently-active repos) / Settings / Quit - plus the
//! left-click-to-show behavior. Each menu item is a thin trigger that calls an
//! existing IPC command or core entry point; no product logic lives here (E-13 AC4).
//!
//! The frameless left-click POPOVER window is deliberately cut to V1.1 (BL-V11-01);
//! left-click shows + focuses the main window instead. Window close-to-tray lives in
//! [`crate::windows`] (window lifecycle), with the tray as the restore path.
//!
//! The `tray-icon` Tauri cargo feature is enabled in `Cargo.toml` so these APIs are
//! available.

use std::sync::Mutex as StdMutex;

use tauri::{
    menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, Wry,
};

use reposync_core::ipc::RepoId;
use reposync_core::store::RepoRef;

use crate::AppState;

/// How many recently-active repos the "Open recent" submenu lists.
pub const RECENT_LIMIT: usize = 6;

/// Build and mount the tray icon + menu onto the app (via its [`AppHandle`]).
///
/// Called once from `lib.rs::run`'s setup (AFTER the SQLite pool is initialized, so
/// the "Open recent" submenu can be seeded from the DB). `recent` is the
/// most-recently-active repos, newest first (see
/// [`reposync_core::store::recent_repos`]); each becomes a submenu item whose id is
/// `recent:<repo id>`, opening that repo's folder via the hardened opener.
/// The live tray, kept in managed state so its menu can be replaced after launch
/// (BL-NI-40).
pub struct TrayHandles {
    tray: TrayIcon<Wry>,
    /// The pause item belonging to the CURRENT menu, replaced on every rebuild.
    ///
    /// Every item is rebuilt per menu rather than shared, which is the opposite
    /// of the obvious design and is forced by the platform. In muda's Windows
    /// implementation, adding an item to a menu pushes that menu's two HMENUs
    /// onto the item's `parents_hemnu`, and `Menu::drop` destroys those handles
    /// WITHOUT removing the registrations. A shared item would therefore grow
    /// that vector by two entries per rebuild forever, and `set_text` (which the
    /// pause toggle calls) walks the whole vector issuing `SetMenuItemInfoW`
    /// against handles that no longer exist. In a tray app that runs for weeks
    /// that is unbounded growth plus writes to destroyed handles.
    ///
    /// So the menu-event handler cannot capture a clone; it reads the current
    /// item from here instead. This mutex is the price of rebuilding, and it is
    /// the cheaper price.
    pause: StdMutex<MenuItem<Wry>>,
    /// The recent set the INSTALLED menu was built from.
    ///
    /// Advanced only after `set_menu` SUCCEEDS. Recording it earlier means a
    /// failed compose or install leaves the gate claiming the menu is current
    /// while the tray still shows the old one, and no later refresh for that same
    /// key would ever retry.
    last_recent: StdMutex<Vec<(i64, String)>>,
    /// Serializes a whole refresh: read, compose, install, advance the gate.
    ///
    /// Without it two concurrent refreshes can interleave so the LAST menu
    /// installed is not the one the gate records, leaving a stale menu that no
    /// future refresh will correct because the key already matches.
    refresh: tokio::sync::Mutex<()>,
}

/// One freshly built menu, plus the pause item inside it that the toggle needs.
struct BuiltMenu {
    menu: Menu<Wry>,
    pause: MenuItem<Wry>,
}

/// Build every menu item and compose them into a menu.
///
/// Used by BOTH the launch path and the refresh path, so the two cannot drift
/// into different menus. Every item is new each time; see [`TrayHandles`] for why
/// sharing them is not an option on Windows.
fn build_menu(app: &AppHandle, recent: &[RepoRef]) -> tauri::Result<BuiltMenu> {
    let show = MenuItemBuilder::with_id("show", "Show RepoSync").build(app)?;
    let check_all = MenuItemBuilder::with_id("check_all", "Check All Now").build(app)?;
    // Read the LIVE pause state rather than defaulting: a rebuilt menu must keep
    // the label the user last set, not silently reset to "Pause all".
    let pause = MenuItemBuilder::with_id("pause", pause_label(is_paused(app))).build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    // Keep the built items alive in a Vec until the submenu is built (they are
    // ref-counted handles the builder clones, but holding them is the clearest,
    // safest pattern). An empty registry shows a single disabled placeholder.
    let mut recent_items: Vec<MenuItem<Wry>> = Vec::new();
    if recent.is_empty() {
        recent_items.push(
            MenuItemBuilder::with_id("recent-empty", "No recent repos")
                .enabled(false)
                .build(app)?,
        );
    } else {
        for r in recent {
            recent_items.push(
                MenuItemBuilder::with_id(format!("recent:{}", r.id), &r.local_name).build(app)?,
            );
        }
    }
    let mut recent_builder = SubmenuBuilder::new(app, "Open recent");
    for item in &recent_items {
        recent_builder = recent_builder.item(item);
    }
    let recent_menu = recent_builder.build()?;

    let menu = MenuBuilder::new(app)
        .item(&show)
        .item(&check_all)
        .item(&pause)
        .separator()
        .item(&recent_menu)
        .item(&settings)
        .separator()
        .item(&quit)
        .build()?;

    Ok(BuiltMenu { menu, pause })
}

/// The pause item's label for a given paused state. Pure, so the one place a
/// swapped boolean would silently mislabel the tray is asserted by a test.
fn pause_label(paused: bool) -> &'static str {
    if paused {
        "Resume all"
    } else {
        "Pause all"
    }
}

/// The current global-pause state, read so a REBUILT menu keeps the label the
/// user last set instead of resetting on the next refresh.
fn is_paused(app: &AppHandle) -> bool {
    app.try_state::<AppState>()
        .map(|s| s.pause.is_paused())
        .unwrap_or(false)
}

/// The recent set as a comparable key: id AND name, because a rename changes the
/// label without changing the order and the menu would otherwise keep the old one.
fn recent_key(recent: &[RepoRef]) -> Vec<(i64, String)> {
    recent
        .iter()
        .map(|r| (r.id, r.local_name.clone()))
        .collect()
}

/// Rebuild the tray menu from the CURRENT registry (BL-NI-40).
///
/// The "Open recent" submenu used to be a startup snapshot: a repo added after
/// launch never appeared in it, a removed one lingered, and a renamed one kept
/// its old label until the app restarted. For a tray-first tool that made the
/// always-available surface the one most likely to be out of date.
///
/// A no-op when the list is unchanged, which is the common case: this is called
/// after events that COULD reorder it rather than only ones that did.
///
/// Best-effort throughout. A tray that cannot refresh keeps the menu it has, and
/// that must never propagate into the check or scheduler paths that trigger it.
pub async fn refresh_recent_menu(app: &AppHandle) {
    let Some(handles) = app.try_state::<TrayHandles>() else {
        // The tray failed to build at startup. That is already logged and it
        // gates the window lifecycle; there is nothing here to refresh.
        return;
    };
    // Held for the WHOLE operation: read, compose, install, advance the gate.
    let _serialized = handles.refresh.lock().await;

    let pool = { app.state::<AppState>().pool.clone() };
    let recent = match reposync_core::store::recent_repos(&pool, RECENT_LIMIT).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("tray: could not read recent repos to refresh the menu: {e}");
            return;
        }
    };

    let key = recent_key(&recent);
    let unchanged = {
        let last = handles
            .last_recent
            .lock()
            .expect("tray recent-menu lock poisoned");
        *last == key
    };
    if unchanged {
        return;
    }

    let built = match build_menu(app, &recent) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("tray: could not compose the refreshed menu: {e}");
            return;
        }
    };
    if let Err(e) = handles.tray.set_menu(Some(built.menu)) {
        // The gate is deliberately NOT advanced here: the tray still shows the
        // old menu, so the next trigger must be free to try again.
        tracing::warn!("tray: could not install the refreshed menu: {e}");
        return;
    }

    // Installed. Only now does the recorded state match what the user can see.
    *handles.pause.lock().expect("tray pause-item lock poisoned") = built.pause;
    *handles
        .last_recent
        .lock()
        .expect("tray recent-menu lock poisoned") = key;
}

pub fn init(app: &AppHandle, recent: &[RepoRef]) -> tauri::Result<()> {
    // Built through the SAME helper the refresh path uses, so the launch menu and
    // every rebuilt one cannot drift apart.
    let built = build_menu(app, recent)?;

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&built.menu)
        .tooltip("RepoSync")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "check_all" => spawn_check_all(app),
            // The handler looks the CURRENT pause item up rather than capturing
            // a clone: items are rebuilt with every menu, so a captured handle
            // would go stale on the first refresh and the label would stop
            // updating. See `TrayHandles::pause`.
            "pause" => toggle_pause(app),
            "settings" => {
                // Open + focus the window, then ask the frontend to route to Settings
                // (E-13 AC2). The typed `nav:requested` event is handled by the app
                // shell; if no webview is up, the navigation is simply a no-op.
                show_main_window(app);
                crate::events::emit_navigate(app, "settings");
            }
            "quit" => app.exit(0),
            other if other.starts_with("recent:") => {
                if let Some(id) = other
                    .strip_prefix("recent:")
                    .and_then(|s| s.parse::<i64>().ok())
                {
                    open_recent_repo(app, id);
                }
            }
            _ => {}
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main_window(tray.app_handle());
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let tray = builder.build(app)?;

    // Hold the tray and the current pause item so the menu can be replaced later
    // (BL-NI-40). Managed as its own state rather than folded into `AppState`,
    // which is already constructed and managed by the time the tray is built
    // (deliberately: a menu click must never race an unmanaged state).
    app.manage(TrayHandles {
        tray,
        pause: StdMutex::new(built.pause),
        last_recent: StdMutex::new(recent_key(recent)),
        refresh: tokio::sync::Mutex::new(()),
    });

    Ok(())
}

/// Unminimize, show, and focus the main window, if it exists.
///
/// Shared by the tray menu's "show"/"settings" items and a left-click on the tray
/// icon so every entry point behaves identically.
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}

/// Toggle the global-pause flag and reflect the NEW state in the menu item's label
/// ("Pause all" while running, "Resume all" while paused). The scheduler reads the
/// same shared flag at the start of every cycle, so a toggle takes effect on the next
/// tick without a restart.
fn toggle_pause(app: &AppHandle) {
    let now_paused = app.state::<AppState>().pause.toggle();
    // The item is read from managed state rather than captured, because every
    // menu rebuild replaces it. A captured clone would still be a valid Rust
    // handle after a refresh and would write to a destroyed native menu.
    let Some(handles) = app.try_state::<TrayHandles>() else {
        return;
    };
    let item = handles
        .pause
        .lock()
        .expect("tray pause-item lock poisoned")
        .clone();
    if let Err(e) = item.set_text(pause_label(now_paused)) {
        tracing::warn!("tray: could not update the Pause/Resume label: {e}");
    }
}

/// Spawn a background "check all enabled repos" (E-13 "Check All Now"). Fire-and-
/// forget from the synchronous menu handler: the work runs on the async runtime,
/// per-repo events drive the UI, and an overall failure surfaces on `error:raised`.
fn spawn_check_all(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (pool, git, locks, semaphore) = {
            let state = app.state::<AppState>();
            (
                state.pool.clone(),
                state.git.clone(),
                state.locks.clone(),
                std::sync::Arc::clone(&state.check_all_semaphore),
            )
        };
        if let Err(e) =
            crate::commands::check_all_enabled(&app, &pool, &git, &locks, &semaphore).await
        {
            crate::events::emit_error_raised(&app, &e);
            tracing::warn!("tray: check all now failed: {e}");
        }
    });
}

/// Spawn a background open-folder for the recent-submenu repo `id`, resolving its
/// current path from the DB (so a moved clone opens where it actually lives) and
/// routing through the hardened [`crate::opener::open_folder`]. Any failure surfaces
/// on `error:raised`.
fn open_recent_repo(app: &AppHandle, id: i64) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let pool = app.state::<AppState>().pool.clone();
        match reposync_core::store::repo_get(&pool, RepoId(id)).await {
            Ok(detail) => {
                if let Err(e) = crate::opener::open_folder(std::path::Path::new(&detail.local_path))
                {
                    crate::events::emit_error_raised(&app, &e);
                    tracing::warn!("tray: open recent repo {id} failed: {e}");
                }
            }
            Err(e) => {
                crate::events::emit_error_raised(&app, &e);
                tracing::warn!("tray: open recent repo {id} lookup failed: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(id: i64, name: &str) -> RepoRef {
        RepoRef {
            id,
            local_name: name.to_string(),
            local_path: format!("C:/repos/{name}"),
        }
    }

    /// The refresh short-circuits on an unchanged list, so what counts as
    /// "unchanged" decides whether the menu ever updates. This is the one part of
    /// BL-NI-40 that is testable without a running tray, and it is also the part
    /// most likely to be wrong: everything else is Tauri calls.
    #[test]
    fn the_recent_key_changes_when_the_order_changes() {
        let a = recent_key(&[repo(1, "alpha"), repo(2, "beta")]);
        let b = recent_key(&[repo(2, "beta"), repo(1, "alpha")]);
        assert_ne!(
            a, b,
            "the submenu lists most-recent-first, so a reorder IS a change even \
             though the membership is identical"
        );
    }

    /// A rename changes the label without touching the order. Keying on ids alone
    /// would leave the old name in the menu until something else happened to
    /// reorder the list, which could be days.
    #[test]
    fn the_recent_key_changes_when_a_repo_is_renamed() {
        let before = recent_key(&[repo(1, "alpha")]);
        let after = recent_key(&[repo(1, "alpha-renamed")]);
        assert_ne!(before, after);
    }

    #[test]
    fn the_recent_key_changes_when_membership_changes() {
        let before = recent_key(&[repo(1, "alpha")]);
        assert_ne!(before, recent_key(&[repo(1, "alpha"), repo(2, "beta")]));
        assert_ne!(before, recent_key(&[]));
    }

    /// The common case by far: a scheduler tick ran, nothing about the six most
    /// recent repos moved, and the menu must NOT be rebuilt. This loop fires every
    /// minute for the life of the process, so a key that compared unequal to
    /// itself would rebuild a native menu sixty times an hour.
    #[test]
    fn an_identical_list_produces_an_identical_key() {
        let list = [repo(1, "alpha"), repo(2, "beta"), repo(3, "gamma")];
        assert_eq!(recent_key(&list), recent_key(&list));
    }
    /// The pause label is one boolean away from mislabelling the tray, and a
    /// swap would produce a menu that says "Pause all" while paused with nothing
    /// failing anywhere. It is also read on every REBUILD now, so a rebuilt menu
    /// keeps the state the user set instead of resetting.
    #[test]
    fn the_pause_label_matches_the_state() {
        assert_eq!(pause_label(true), "Resume all");
        assert_eq!(pause_label(false), "Pause all");
    }
}
