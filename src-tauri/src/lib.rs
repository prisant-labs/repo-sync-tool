// RepoSync Tauri v2 shell library.
//
// Owning effort: E-01 (Foundation); E-12 (tracer bullet) wires the first real
// commands, event, and managed state end to end.
//
// This is the thin Tauri shell that hosts the Tauri-free `reposync-core` crate.
// All product logic lives in the core; this shell only owns the runtime, the
// IPC surface (E-06 contract), event emission, the tray, and windows.
//   - commands  -> IPC command handlers (E-06 owns the payload contract)
//   - events    -> backend -> frontend event emission (E-06)
//   - tray      -> system tray icon and menu (later GUI effort)
//   - windows   -> window creation and management (later GUI effort)

mod commands;
mod events;
mod tray;
mod windows;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events};

use commands::{
    activity_list, repo_add_path, repo_check_now, repo_get, repo_list, repo_open_editor,
    repo_open_folder, repo_open_remote, repo_open_terminal, repo_refresh_metadata, repo_remove,
    repo_scan_parent, repo_set_enabled, repo_set_policy, repo_update_now, settings_get,
    settings_set, summary_today, summary_week,
};
use events::{
    CheckCompleted, CheckStarted, ErrorRaised, NotificationFired, SchedulerTick, StateChanged,
    UpdateCompleted, UpdateStarted,
};

/// Shared, managed application state injected into every command.
///
/// Holds the long-lived SQLite pool and the (optionally) discovered git engine
/// that `reposync-core` flows operate on. Built once in [`run`]'s setup and
/// handed to Tauri via `app.manage`.
///
/// `git` is `None` when git could not be discovered at startup. Git absence must
/// NOT prevent launch (E-03 degraded-state contract): the app still opens and
/// git-dependent commands return [`AppError::GitNotFound`]. The full re-probe
/// state machine is E-03 scope; this is the minimal tolerant form.
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub git: Option<reposync_core::git::SystemGitEngine>,
}

/// Build the `tauri-specta` [`Builder`](tauri_specta::Builder) for the shell.
///
/// Single source of truth for the command + event surface so both [`run`] and
/// the headless `export_bindings` test register exactly the same set, keeping
/// the generated TypeScript bindings in lockstep with the runtime handlers.
fn specta_builder() -> tauri_specta::Builder<tauri::Wry> {
    // `AppErrorPayload.context` is `Option<serde_json::Value>` (free-form JSON).
    // specta-typescript cannot inline `serde_json::Value` because it is mutually
    // recursive (Value -> Vec<Value> -> Value), so map it to the TS `unknown`
    // type, which is the correct frontend type for an opaque JSON blob. This
    // remap lives on the export side, leaving `reposync-core` untouched.
    let semantic = specta_typescript::semantic::Configuration::default()
        .define::<serde_json::Value>(|_| specta_typescript::define("unknown").into(), None, None);

    tauri_specta::Builder::<tauri::Wry>::new()
        .commands(collect_commands![
            // tracer (E-12)
            repo_add_path,
            repo_check_now,
            // E-06 full surface
            repo_list,
            repo_get,
            repo_scan_parent,
            repo_remove,
            repo_set_enabled,
            repo_set_policy,
            repo_update_now,
            repo_refresh_metadata,
            repo_open_folder,
            repo_open_terminal,
            repo_open_editor,
            repo_open_remote,
            activity_list,
            summary_today,
            summary_week,
            settings_get,
            settings_set,
        ])
        .events(collect_events![
            // tracer (E-12)
            CheckCompleted,
            // E-06 full surface
            StateChanged,
            CheckStarted,
            UpdateStarted,
            UpdateCompleted,
            SchedulerTick,
            NotificationFired,
            ErrorRaised,
        ])
        // The IPC payloads carry `i64` repo ids, ahead/behind counts, and unix
        // second timestamps. specta-typescript refuses to emit i64/u64 by
        // default (BigInt precision risk); cast them to TS `number`. Every such
        // value here fits comfortably in JS's 2^53 safe-integer range, so the
        // cast is lossless in practice. This must be set on the shared factory
        // so the runtime invoke surface and the exported bindings agree.
        .dangerously_cast_bigints_to_number()
        .semantic_types(semantic)
}

/// Application entry point invoked by `main.rs` (and the mobile entry point).
///
/// Builds the `tauri-specta` command/event surface, wires the invoke handler,
/// mounts the events, then initializes the SQLite pool and git engine into
/// managed [`AppState`] before running the Tauri runtime.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = specta_builder();

    // Dev convenience: regenerate the bindings on every debug run so a local
    // contract change is visible immediately. Goes through the shared
    // `export_bindings` helper (same builder + header) so the dev-written file
    // is byte-identical to the one the `export_bindings` test commits; the test
    // is the canonical producer CI relies on.
    #[cfg(debug_assertions)]
    export_bindings("../src/lib/bindings.ts").expect("failed to export typescript bindings");

    tauri::Builder::default()
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // Register the event registry so typed emit/listen resolve names.
            builder.mount_events(app);

            // Initialize the pool + git engine synchronously during setup. The
            // tracer accepts a blocking init; later efforts can move this off
            // the setup thread if startup latency matters.
            let handle = app.handle().clone();
            tauri::async_runtime::block_on(async move {
                // Resolve the path seam once. The OneDrive backstop (E-02 AC6):
                // %LOCALAPPDATA% is already outside the synced tree, but if the
                // resolved data dir somehow lands under a OneDrive root, warn -
                // a WAL db there can corrupt when the sync agent snapshots its
                // sidecars mid-write. We do not relocate at runtime; the warning
                // is the signal and the base-dir choice is the structural defense.
                let paths = reposync_core::paths::AppPaths::from_env();
                if paths.is_onedrive_rooted() {
                    eprintln!(
                        "warning: RepoSync data dir {} is under a OneDrive root; a \
                         WAL database in a synced folder can corrupt. Consider moving \
                         app data out of OneDrive.",
                        paths.data_dir().display()
                    );
                }

                // Open the pool and migrate, recovering from a corrupt/failed
                // migration instead of crashing (E-02 AC7). `recovered` is a
                // one-time notice the shell can surface later.
                let init = reposync_core::db::init_pool_with_recovery(&paths)
                    .await
                    .expect("failed to initialize database");
                if init.recovered {
                    eprintln!(
                        "warning: the database could not be migrated and was reset; \
                         the previous database was preserved at {:?}.",
                        init.backup_path
                    );
                }
                let pool = init.pool;
                // Git absence must NOT block launch (E-03 degraded-state
                // contract). Store None on GitNotFound and log a warning; the
                // pool/migrations above stay fatal because the DB is essential.
                let git = match reposync_core::git::SystemGitEngine::discover() {
                    Ok(engine) => Some(engine),
                    Err(_) => {
                        eprintln!(
                            "warning: git executable not found; git-dependent \
                             actions will report GitNotFound until git is available"
                        );
                        None
                    }
                };
                handle.manage(AppState { pool, git });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running RepoSync");
}

/// Export the TypeScript IPC bindings to `path`, headlessly (no GUI launch).
///
/// Canonical producer of the committed `src/lib/bindings.ts`: it builds the
/// exact same `tauri-specta` surface the runtime uses (via [`specta_builder`])
/// and writes the TypeScript. Exposed so the headless `export_bindings`
/// integration test (in `tests/`) can call it. The integration test - not a
/// `--lib` unit test - is required on Windows because the comctl32-v6 manifest
/// that lets a Tauri-linked test executable start is attached only to `[[test]]`
/// targets (see `build.rs`).
pub fn export_bindings(path: &str) -> Result<(), specta_typescript::Error> {
    // The generated file uses `any` in its runtime shim, which the project
    // eslint config rejects. It is machine-generated and never hand-edited, so
    // exempt the whole file from eslint via a leading `/* eslint-disable */`.
    // (It type-checks cleanly under tsc, so no `@ts-nocheck` is needed - and
    // `ban-ts-comment` would flag that anyway.)
    let ts = specta_typescript::Typescript::default().header("/* eslint-disable */\n");
    specta_builder().export(ts, path)
}
