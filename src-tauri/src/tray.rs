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
/// The menu items that never change, held so every rebuilt menu reuses the SAME
/// handles rather than fresh ones.
///
/// That reuse is load-bearing, not an optimization. The menu-event handler
/// captures a clone of the pause item so a toggle can flip its label; rebuilding
/// that item would leave the handler holding a stale handle and the label would
/// silently stop updating. Only the "Open recent" submenu is ever reconstructed.
struct FixedItems {
    show: MenuItem<Wry>,
    check_all: MenuItem<Wry>,
    pause: MenuItem<Wry>,
    settings: MenuItem<Wry>,
    quit: MenuItem<Wry>,
}

/// The live tray, kept in managed state so its menu can be replaced after launch
/// (BL-NI-40).
pub struct TrayHandles {
    tray: TrayIcon<Wry>,
    items: FixedItems,
    /// The recent set the CURRENT menu was built from, so an unchanged list costs
    /// nothing. A scheduler tick fires every minute and usually changes nothing
    /// about which six repos are most recent; rebuilding a native menu on each
    /// one would be churn a user could plausibly see.
    last_recent: StdMutex<Vec<(i64, String)>>,
}

/// Compose the full menu from the fixed items plus a freshly built "Open recent"
/// submenu. Used by BOTH the launch path and the refresh path, so the two cannot
/// drift into different menus.
fn compose_menu(
    app: &AppHandle,
    items: &FixedItems,
    recent: &[RepoRef],
) -> tauri::Result<Menu<Wry>> {
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

    MenuBuilder::new(app)
        .item(&items.show)
        .item(&items.check_all)
        .item(&items.pause)
        .separator()
        .item(&recent_menu)
        .item(&items.settings)
        .separator()
        .item(&items.quit)
        .build()
}

/// The recent set as a comparable key: id AND name, because a rename changes the
/// label without changing the order and the menu would otherwise keep the old one.
fn recent_key(recent: &[RepoRef]) -> Vec<(i64, String)> {
    recent
        .iter()
        .map(|r| (r.id, r.local_name.clone()))
        .collect()
}

/// Rebuild the "Open recent" submenu from the CURRENT registry (BL-NI-40).
///
/// The submenu used to be a startup snapshot: a repo added after launch never
/// appeared in it, a removed one lingered, and a renamed one kept its old label
/// until the app restarted. For a tray-first tool that made the always-available
/// surface the one most likely to be out of date.
///
/// A no-op when the list is unchanged, which is the common case: this is called
/// after events that COULD reorder it rather than only ones that did, so most
/// calls cost one query and stop.
///
/// Best-effort throughout. A tray that cannot refresh keeps the menu it has, and
/// that must never propagate into the check or scheduler paths that trigger it.
pub async fn refresh_recent_menu(app: &AppHandle) {
    let Some(handles) = app.try_state::<TrayHandles>() else {
        // The tray failed to build at startup. That is already logged and it
        // gates the window lifecycle; there is simply nothing here to refresh.
        return;
    };
    let pool = { app.state::<AppState>().pool.clone() };
    let recent = match reposync_core::store::recent_repos(&pool, RECENT_LIMIT).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("tray: could not read recent repos to refresh the menu: {e}");
            return;
        }
    };

    {
        let mut last = handles
            .last_recent
            .lock()
            .expect("tray recent-menu lock poisoned");
        if *last == recent_key(&recent) {
            return;
        }
        *last = recent_key(&recent);
    }

    match compose_menu(app, &handles.items, &recent) {
        Ok(menu) => {
            if let Err(e) = handles.tray.set_menu(Some(menu)) {
                tracing::warn!("tray: could not install the refreshed menu: {e}");
            }
        }
        Err(e) => tracing::warn!("tray: could not compose the refreshed menu: {e}"),
    }
}

pub fn init(app: &AppHandle, recent: &[RepoRef]) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show RepoSync").build(app)?;
    let check_all = MenuItemBuilder::with_id("check_all", "Check All Now").build(app)?;
    // The Pause item starts as "Pause all" (pause is in-memory and defaults to
    // running at every launch); the on-menu handler flips its label on toggle.
    let pause = MenuItemBuilder::with_id("pause", "Pause all").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    // Compose through the SAME helper the refresh path uses, so the launch menu
    // and every rebuilt one cannot drift apart.
    let items = FixedItems {
        show,
        check_all,
        pause,
        settings,
        quit,
    };
    let menu = compose_menu(app, &items, recent)?;

    // The Pause item is cloned into the handler so a toggle can update its label.
    // It is the SAME handle `FixedItems` holds, so it stays valid across every
    // menu rebuild; that is why the fixed items are reused rather than rebuilt.
    let pause_item = items.pause.clone();

    let mut builder = TrayIconBuilder::with_id("main")
        .menu(&menu)
        .tooltip("RepoSync")
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "check_all" => spawn_check_all(app),
            "pause" => toggle_pause(app, &pause_item),
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

    // Hold the tray and its fixed items so the recent submenu can be replaced
    // later (BL-NI-40). Managed as its own state rather than folded into
    // `AppState`, which is already constructed and managed by the time the tray
    // is built (deliberately: a menu click must never race an unmanaged state).
    app.manage(TrayHandles {
        tray,
        items,
        last_recent: StdMutex::new(recent_key(recent)),
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
fn toggle_pause(app: &AppHandle, pause_item: &MenuItem<Wry>) {
    let now_paused = app.state::<AppState>().pause.toggle();
    let label = if now_paused {
        "Resume all"
    } else {
        "Pause all"
    };
    if let Err(e) = pause_item.set_text(label) {
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
}
