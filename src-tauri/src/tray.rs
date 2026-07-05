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

use tauri::{
    menu::{MenuBuilder, MenuItem, MenuItemBuilder, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
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
pub fn init(app: &AppHandle, recent: &[RepoRef]) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show RepoSync").build(app)?;
    let check_all = MenuItemBuilder::with_id("check_all", "Check All Now").build(app)?;
    // The Pause item starts as "Pause all" (pause is in-memory and defaults to
    // running at every launch); the on-menu handler flips its label on toggle.
    let pause = MenuItemBuilder::with_id("pause", "Pause all").build(app)?;
    let settings = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

    // "Open recent" submenu. Keep the built items alive in a Vec until the submenu is
    // built (they are ref-counted handles the builder clones, but holding them is the
    // clearest, safest pattern). An empty registry shows a single disabled placeholder.
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

    // The Pause item is cloned into the handler so a toggle can update its label.
    let pause_item = pause.clone();

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
    let _tray = builder.build(app)?;

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
        eprintln!("tray: could not update the Pause/Resume label: {e}");
    }
}

/// Spawn a background "check all enabled repos" (E-13 "Check All Now"). Fire-and-
/// forget from the synchronous menu handler: the work runs on the async runtime,
/// per-repo events drive the UI, and an overall failure surfaces on `error:raised`.
fn spawn_check_all(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let (pool, git, locks) = {
            let state = app.state::<AppState>();
            (state.pool.clone(), state.git.clone(), state.locks.clone())
        };
        if let Err(e) = crate::commands::check_all_enabled(&app, &pool, &git, &locks).await {
            crate::events::emit_error_raised(&app, &e);
            eprintln!("tray: check all now failed: {e}");
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
                    eprintln!("tray: open recent repo {id} failed: {e}");
                }
            }
            Err(e) => {
                crate::events::emit_error_raised(&app, &e);
                eprintln!("tray: open recent repo {id} lookup failed: {e}");
            }
        }
    });
}
