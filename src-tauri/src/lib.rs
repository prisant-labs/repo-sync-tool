// RepoSync Tauri v2 shell library.
//
// Owning effort: E-01 (Foundation).
//
// This is the thin Tauri shell that hosts the Tauri-free `reposync-core`
// crate. In E-01 it is a minimal builder that starts and runs the runtime
// with no commands, events, tray, or windows wired up yet. Later efforts
// fill in the stub modules referenced below:
//   - commands  -> IPC command handlers (E-06 owns the payload contract)
//   - events    -> backend -> frontend event emission (E-06)
//   - tray      -> system tray icon and menu (later GUI effort)
//   - windows   -> window creation and management (later GUI effort)
//
// TODO(E-06): register the generated command/event handlers and managed state.

mod commands;
mod events;
mod tray;
mod windows;

/// Application entry point invoked by `main.rs` (and the mobile entry point).
///
/// Builds and runs the Tauri v2 runtime. In the E-01 skeleton this is the
/// default builder with no handlers; it exists so the shell compiles, links,
/// and bundles end to end.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // TODO(E-06): chain `.invoke_handler(...)`, `.manage(...)`, tray setup,
    // and window creation onto this builder as those efforts land.
    tauri::Builder::default()
        .run(tauri::generate_context!())
        .expect("error while running RepoSync");
}
