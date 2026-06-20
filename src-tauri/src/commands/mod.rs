//! Tauri IPC command handlers for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) for the stub; E-06 (IPC contract) for
//! the real commands.
//!
//! Each `#[tauri::command]` here is a thin adapter: it deserializes a typed
//! request, calls into `reposync-core`, and returns a typed response. The
//! payload types and the `tauri-specta` binding/codegen live with E-06.
//!
// TODO(E-06): define the command set and the specta-exported payload types,
// then register them via `tauri_specta` in `lib.rs::run`.
