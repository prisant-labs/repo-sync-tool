//! Window creation and management for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) for the stub; a later GUI effort for the
//! real windows.
//!
//! RepoSync uses a small set of windows (the tray popover and a main/settings
//! window). This module will own creating, showing, hiding, and positioning
//! them. The E-01 `tauri.conf.json` declares no windows; they are created at
//! runtime by the GUI effort.
//!
// TODO(GUI): build the popover and main windows via `WebviewWindowBuilder`,
// wiring creation/visibility into the tray and command flows.
