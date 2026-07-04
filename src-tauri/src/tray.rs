//! System tray icon and menu for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) built the stub; this GUI effort builds the
//! real tray.
//!
//! RepoSync is a tray-first utility: the primary affordance is a tray icon
//! with a menu and a popover window. This module builds the tray icon, its
//! menu ("Show RepoSync" / "Quit"), and click handling for both the menu and
//! the tray icon itself.
//!
//! The `tray-icon` Tauri cargo feature is enabled in `Cargo.toml` so these
//! APIs are available.

use tauri::{
    menu::{MenuBuilder, MenuItemBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    Manager,
};

/// Build and mount the tray icon + menu onto `app`.
///
/// Called once from `lib.rs::run`'s `.setup(...)` hook. The menu offers
/// "Show RepoSync" (id `"show"`) and "Quit" (id `"quit"`); a left-click on the
/// tray icon itself also shows the main window rather than opening the menu.
pub fn init(app: &tauri::App) -> tauri::Result<()> {
    let show = MenuItemBuilder::with_id("show", "Show RepoSync").build(app)?;
    let quit = MenuItemBuilder::with_id("quit", "Quit").build(app)?;
    let menu = MenuBuilder::new(app)
        .item(&show)
        .separator()
        .item(&quit)
        .build()?;

    let mut builder = TrayIconBuilder::new()
        .menu(&menu)
        .tooltip("RepoSync")
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "show" => show_main_window(app),
            "quit" => app.exit(0),
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
/// Shared by both the tray menu's "show" item and a left-click on the tray
/// icon so the two entry points behave identically.
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
    }
}
