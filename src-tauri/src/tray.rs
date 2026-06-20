//! System tray icon and menu for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) for the stub; a later GUI effort for the
//! real tray.
//!
//! RepoSync is a tray-first utility: the primary affordance is a tray icon
//! with a menu and a popover window. This module will own building the tray
//! icon, its menu, and click handling.
//!
//! IMPORTANT (E-01): this stub deliberately calls NO tray APIs. The Tauri
//! `tray-icon` cargo feature is intentionally left disabled in `Cargo.toml`
//! so the skeleton compiles and bundles without pulling tray dependencies.
//! The later GUI effort enables the feature and implements the tray here.
//!
// TODO(GUI): enable the `tray-icon` feature on the `tauri` dependency and
// build the tray icon + menu, wiring it into `lib.rs::run`.
