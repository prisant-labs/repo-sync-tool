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
mod localtime;
mod opener;
mod tray;
mod windows;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events};

use commands::{
    activity_list, group_assign, group_create, group_delete, group_list, group_rename,
    group_unassign, groups_for_repo, repo_add_path, repo_check_now, repo_get, repo_list,
    repo_open_editor, repo_open_folder, repo_open_remote, repo_open_terminal, repo_refresh_metadata,
    repo_remove, repo_scan_parent, repo_set_enabled, repo_set_policy, repo_update_now, settings_get,
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
/// `git` holds `None` when git could not be discovered. Git absence must NOT
/// prevent launch (E-03 degraded-state contract): the app still opens and
/// git-dependent commands return [`AppError::GitNotFound`]. The engine sits
/// behind an `RwLock` so `settings_set` can re-probe from the newly-saved
/// `git_executable_path` and swap it live (BL-NI-19), letting a user who fixes a
/// broken git path recover without restarting - the command path picks up the
/// new engine immediately. (The resident scheduler keeps its own initial engine
/// and only picks up a re-probe on restart; see the setup note below.)
///
/// `db_recovered` / `db_backup_path` carry the E-02 AC7 migration-recovery notice
/// produced by `init_pool_with_recovery`: `db_recovered` is true exactly when the
/// startup migration failed and the old database was moved aside, and
/// `db_backup_path` is where it was preserved. The shell persists this one-time
/// signal here so a later UI/command can surface it; without this it was logged
/// and dropped, so the notice could never reach the UI.
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    /// The discovered git engine, behind an `RwLock` so `settings_set` can
    /// re-probe and swap it LIVE when the user fixes a broken/missing git path
    /// (BL-NI-19). Readers clone the engine out under a read guard (they never
    /// block each other); only the settings writer takes the write guard. No
    /// `Arc` is needed - `AppState` is already behind Tauri's managed `Arc`.
    pub git: tokio::sync::RwLock<Option<reposync_core::git::SystemGitEngine>>,
    /// The per-repo lock map, SHARED with the resident scheduler (when git is
    /// present it is the scheduler's own `RepoLocks`). Manual command handlers
    /// acquire the same per-repo mutex the scheduler's jobs do, so a "check now"
    /// and a scheduled check never run two `git` processes in one working tree.
    pub locks: reposync_core::scheduler::RepoLocks,
    pub db_recovered: bool,
    pub db_backup_path: Option<std::path::PathBuf>,
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
            // groups / tags
            group_list,
            group_create,
            group_rename,
            group_delete,
            group_assign,
            group_unassign,
            groups_for_repo,
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

            // Build the tray icon + menu (E-01 stub replaced by the GUI effort).
            tray::init(app)?;

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
                // Carry the one-time recovery notice into AppState so a later
                // UI/command can surface it (E-02 AC7); previously it was logged
                // and dropped, so the notice never reached the UI.
                let db_recovered = init.recovered;
                let db_backup_path = init.backup_path;
                let pool = init.pool;
                // E-09: prune the activity log once on startup (best-effort -
                // logged, never gates launch). The daily cadence attaches to the
                // scheduler's launch wiring when that lands.
                reposync_core::activity::sweep_at_startup(&pool).await;
                // Git absence must NOT block launch (E-03 degraded-state
                // contract). Honor the user's configured git path (Settings)
                // before falling back to PATH discovery, so a user whose git is
                // not on PATH can point RepoSync at it and recover on restart.
                // Store None when git is unavailable so git-dependent commands
                // report GitNotFound; the pool/migrations above stay fatal because
                // the DB is essential.
                let configured_git_path = reposync_core::store::settings_get(&pool)
                    .await
                    .ok()
                    .and_then(|s| s.git_executable_path);
                let engine = reposync_core::git::SystemGitEngine::new(configured_git_path);
                let git = if engine.availability().is_unavailable() {
                    eprintln!(
                        "warning: git executable not found; git-dependent \
                         actions will report GitNotFound until git is available"
                    );
                    None
                } else {
                    Some(engine)
                };

                // Edge-wiring: spawn the resident scheduler and build the shared
                // per-repo lock map. The scheduler runs the jittered startup pass,
                // then ticks every minute for the process lifetime - checking due
                // repos and emitting `scheduler:tick` so the UI refreshes live off
                // the same event the frontend already listens for. Only spawn when
                // git is present: without it every job would return GitNotFound, so
                // the degraded (git-absent) launch stays job-free. The tick loop is
                // owned here (not `Scheduler::run`) because only the edge holds the
                // `AppHandle` needed to emit the tick; the core scheduler is
                // deliberately Tauri-free (E-08).
                //
                // `locks` is the scheduler's OWN `RepoLocks` (cloned; it is a shared
                // Arc-backed map), handed to AppState so the manual command handlers
                // contend the exact locks the scheduler's jobs do. When git is
                // absent there is no scheduler, but manual commands still take a
                // (then-uncontended) lock from a standalone map.
                let locks = if let Some(engine) = &git {
                    use reposync_core::scheduler::{
                        DbDueQuery, DbOutcomeWriter, Scheduler, SystemClock, SystemJitter,
                        UpdateNowJobRunner, DEFAULT_CONCURRENCY, ONE_MINUTE,
                    };
                    use std::sync::Arc;

                    // pool/engine are cloned (both cheap: SqlitePool and the git
                    // engine wrap shared handles) so the originals still move into
                    // AppState below. The clock carries the host's local UTC offset
                    // so quiet hours are evaluated in local time, not UTC.
                    let scheduler = Scheduler::new(
                        Arc::new(SystemClock::with_utc_offset_minutes(
                            crate::localtime::local_offset_minutes(),
                        )),
                        Arc::new(SystemJitter::new()),
                        DbDueQuery::new(pool.clone()),
                        UpdateNowJobRunner::new(pool.clone(), Arc::new(engine.clone())),
                        DbOutcomeWriter::new(pool.clone()),
                        DEFAULT_CONCURRENCY,
                    );
                    let locks = scheduler.locks();
                    let tick_handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        // Startup pass is best-effort; a failure must not kill the loop.
                        if let Err(e) = scheduler.start().await {
                            eprintln!("scheduler: startup pass failed: {e}");
                        }
                        let mut interval = tokio::time::interval(ONE_MINUTE);
                        interval.tick().await; // consume the immediate first tick
                        loop {
                            interval.tick().await;
                            match scheduler.tick_once().await {
                                Ok(ran) => {
                                    let ran = ran as i64;
                                    crate::events::emit_scheduler_tick(
                                        &tick_handle,
                                        ran,
                                        ran,
                                        crate::localtime::now_unix(),
                                    );
                                }
                                Err(e) => eprintln!("scheduler: tick failed: {e}"),
                            }
                        }
                    });
                    locks
                } else {
                    reposync_core::scheduler::RepoLocks::default()
                };

                // Wrap the initial engine in an RwLock so `settings_set` can
                // re-probe and swap it live (BL-NI-19). NOTE: the scheduler
                // spawned above captured its OWN clone of the initial `git`
                // engine (a local, not `AppState.git`); the running scheduler
                // therefore keeps that initial engine and only picks up a
                // re-probe on the NEXT restart. This pass makes the command path
                // live, not the scheduler loop - a known limitation.
                handle.manage(AppState {
                    pool,
                    git: tokio::sync::RwLock::new(git),
                    locks,
                    db_recovered,
                    db_backup_path,
                });
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
