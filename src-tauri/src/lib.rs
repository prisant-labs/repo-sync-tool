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

mod autostart;
mod commands;
mod events;
mod localtime;
mod notify;
mod opener;
mod tray;
mod updates;
mod windows;

use tauri::Manager;
use tauri_specta::{collect_commands, collect_events};

use commands::{
    activity_list, app_check_for_update, app_install_update, db_recovery_notice, group_assign,
    group_create, group_delete, group_list, group_rename, group_unassign, groups_for_repo,
    repo_add_path, repo_check_all, repo_check_now, repo_get, repo_group_memberships, repo_list,
    repo_open_editor, repo_open_folder, repo_open_remote, repo_open_terminal,
    repo_refresh_metadata, repo_remove, repo_scan_parent, repo_set_cadence, repo_set_enabled,
    repo_set_policy, repo_update_now, settings_get, settings_set, summary_today, summary_week,
};
use events::{
    CheckCompleted, CheckStarted, ErrorRaised, MetadataRefreshed, NavigateRequested,
    NotificationFired, SchedulerTick, StateChanged, UpdateCompleted, UpdateStarted,
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
/// behind a SHARED `RwLock` handle ([`SharedGitEngine`]) so `settings_set` can
/// re-probe from the newly-saved `git_executable_path` and swap it live
/// (BL-NI-19), AND the resident scheduler reads the CURRENT engine through the
/// SAME handle each cycle (BL-NI-23): a user who fixes a broken git path recovers
/// with no restart on both the command path and the background loop.
///
/// `settings_write_lock` serializes the whole `settings_set` persist -> reschedule
/// -> re-read -> probe -> swap sequence (BL-NI-35), so two overlapping saves
/// cannot interleave and leave the live engine reflecting older settings than the
/// database.
///
/// `db_recovered` / `db_backup_path` carry the E-02 AC7 migration-recovery notice
/// produced by `init_pool_with_recovery`: `db_recovered` is true exactly when the
/// startup migration failed and the old database was moved aside, and
/// `db_backup_path` is where it was preserved. The shell persists this one-time
/// signal here so a later UI/command can surface it; without this it was logged
/// and dropped, so the notice could never reach the UI.
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    /// The discovered git engine behind the SHARED, swappable handle. Readers
    /// clone the engine out under a read guard (they never block each other); the
    /// settings writer takes the write guard to swap it. The resident scheduler
    /// holds a clone of this same `Arc` handle and reads the current engine each
    /// cycle, so a live re-probe is picked up by the background loop too
    /// (BL-NI-19 / BL-NI-23).
    pub git: reposync_core::scheduler::SharedGitEngine,
    /// The per-repo lock map, SHARED with the resident scheduler (it is the
    /// scheduler's own `RepoLocks`). Manual command handlers acquire the same
    /// per-repo mutex the scheduler's jobs do, so a "check now" and a scheduled
    /// check never run two `git` processes in one working tree.
    pub locks: reposync_core::scheduler::RepoLocks,
    /// Single-flight guard serializing the `settings_set` persist/reschedule/
    /// probe/swap sequence (BL-NI-35), so overlapping saves cannot race on which
    /// probe result wins the final engine swap.
    pub settings_write_lock: tokio::sync::Mutex<()>,
    /// The shared, in-memory global-pause flag (E-13 tray Pause/Resume). The tray
    /// menu toggles it and the resident scheduler reads the SAME handle at the start
    /// of every cycle, so a tray pause suppresses scheduled checks with no restart.
    /// In-memory by design: pause resets to running on the next launch.
    pub pause: reposync_core::scheduler::GlobalPause,
    /// The ONE shared GitHub request budget (E-17 finding 2). BOTH the background
    /// metadata-refresh pass and the manual `repo_refresh_metadata` command spend
    /// against this same rolling-hour budgeter, so a manual refresh can never race the
    /// background pass into overspending the unauthenticated 60/hour ceiling. The
    /// budgeter itself is Tauri-free core (`reposync_core::github::RateBudgeter`); this
    /// is only the shared handle. The background loop holds a clone of the same `Arc`.
    pub github_budget: reposync_core::github::SharedBudgeter,
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
            // E-13 tray "Check All Now" (additive)
            repo_check_all,
            // E-06 full surface
            repo_list,
            repo_get,
            repo_scan_parent,
            repo_remove,
            repo_set_enabled,
            repo_set_policy,
            // per-repo cadence write path (BL-NI-30, additive)
            repo_set_cadence,
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
            // db-recovery notice read (BL-NI-33 / E-02 AC7, additive)
            db_recovery_notice,
            // app self-update (E-18, additive): one typed path for the launch check
            // and the Settings button; the ship-dark + toggle gates live in one place.
            app_check_for_update,
            app_install_update,
            // groups / tags
            group_list,
            group_create,
            group_rename,
            group_delete,
            group_assign,
            group_unassign,
            groups_for_repo,
            repo_group_memberships,
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
            // E-13 tray "Settings" navigation (additive)
            NavigateRequested,
            // E-17 finding 3: background metadata-refresh pass completed with changes
            // (additive); one coalesced signal so the list view refetches once per pass.
            MetadataRefreshed,
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

/// Navigation allowlist for the main WebView (BL-NI-59). Returns `true` to permit a
/// navigation, `false` to cancel it.
///
/// The CSP (BL-NI-58) restricts script-interface egress (`connect-src`) but CANNOT
/// restrict top-level navigation, so any script running as `'self'` (most plausibly a
/// compromised bundled dependency, not XSS - React escapes uniformly) could still
/// `window.location` the WebView off-origin to exfiltrate. This closes that gap by
/// allowing ONLY the app's own origin (and, in dev, the Vite dev server) and denying
/// everything else.
///
/// Origins allowed (host-based where possible, so the http/https scheme on Windows does
/// not matter):
///   - `tauri://localhost`         - macOS/Linux production origin (scheme `tauri`)
///   - `http(s)://tauri.localhost` - Windows production origin (host `tauri.localhost`)
///   - `http://localhost:<port>`   - the Vite dev server, DEV runs only (host `localhost`)
///
/// Pure and side-effect-free so the policy is unit-testable without a Tauri runtime. The
/// caller passes `tauri::is_dev()` for `is_dev` - true only under `tauri dev`, when the
/// dev server actually exists. NOT `cfg!(debug_assertions)`, which is also true for a
/// shipped `tauri build --debug` bundle that loads from `tauri.localhost`, where allowing
/// `localhost` would be a needless hole.
fn allow_navigation(scheme: &str, host: Option<&str>, is_dev: bool) -> bool {
    scheme == "tauri" || host == Some("tauri.localhost") || (is_dev && host == Some("localhost"))
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
        // E-14 desktop notifications: the OS-toast plugin (one API; the plugin maps
        // to the Windows toast vs macOS Notification Center). The firing DECISION is
        // in the Tauri-free core; this plugin is the only platform-specific piece.
        .plugin(tauri_plugin_notification::init())
        // E-15 autostart: the OS launch-on-login plugin (Windows Run key / macOS
        // LaunchAgent behind one API). The register/unregister DECISION (`reconcile`)
        // is in the Tauri-free core; this plugin is the only platform-specific piece.
        // The `--autostart` launch argument is baked into the registration so an
        // autostart launch can be detected (AC3) and started minimized; `LaunchAgent`
        // keeps the macOS registration per-user with no elevation (AC4).
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![crate::autostart::AUTOSTART_LAUNCH_FLAG]),
        ))
        // E-18 auto-update: the self-update plugin (check/download/verify/install)
        // and the process plugin for the post-install relaunch. The check/install
        // DECISIONS and the ship-dark gate live in `crate::updates`; these plugins are
        // the only platform-specific pieces. Signature verification against the
        // committed `plugins.updater.pubkey` is the integrity boundary.
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        // BL-NI-46 native folder picker: the "Add repositories" dialog calls the dialog
        // plugin's open({ directory: true }) to browse for a folder (Explorer on Windows /
        // Finder on macOS). Frontend-driven; no core dependency. The `dialog:allow-open`
        // capability permission gates the open command.
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            // Register the event registry so typed emit/listen resolve names.
            builder.mount_events(app);

            // E-14: reconcile the notification permission once (Granted by default
            // for an installed Windows app; a denial is logged, never fatal).
            notify::ensure_permission(app.handle());

            // BL-NI-59: create the main window in Rust (it was removed from
            // tauri.conf.json `app.windows`) so we can attach an `on_navigation`
            // allowlist - a builder-time-only guard the config path cannot express. The
            // CSP (BL-NI-58) restricts script-interface egress (`connect-src`) but CANNOT
            // restrict top-level navigation, so a script running as `'self'` (most
            // plausibly a compromised bundled dependency) could still `window.location`
            // the WebView off-origin to exfiltrate. `allow_navigation` denies that. The
            // window is created HIDDEN (`visible(false)`, exactly as the old config
            // declared); the lifecycle below shows it on a normal launch, so startup
            // still never flashes.
            tauri::WebviewWindowBuilder::new(app, "main", tauri::WebviewUrl::default())
                .title("RepoSync")
                .inner_size(900.0, 600.0)
                .visible(false)
                .on_navigation(|url| {
                    allow_navigation(url.scheme(), url.host_str(), tauri::is_dev())
                })
                .build()?;

            // Window lifecycle (E-13 AC3 + E-15 AC3, P3-C) is wired AFTER the tray is
            // built (below, in the async block), because close-to-tray and start-
            // minimized are only safe when the tray - the sole restore/quit path -
            // actually built (finding 2). The window is created hidden above, so it
            // stays hidden until the lifecycle shows it, and nothing flashes meanwhile.

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
                // Read the persisted settings once for the two startup-config reads
                // it feeds: the git path (below) and the `autostart` setting (the
                // E-15 reconcile, further down).
                let startup_settings = reposync_core::store::settings_get(&pool).await.ok();
                let configured_git_path = startup_settings
                    .as_ref()
                    .and_then(|s| s.git_executable_path.clone());
                let autostart_on = startup_settings
                    .as_ref()
                    .map(|s| s.autostart)
                    .unwrap_or(false);
                // E-18: the on-launch update-check toggle (default ON). Gates the
                // background launch check spawned after the tray/windows are up.
                let auto_update_on = startup_settings
                    .as_ref()
                    .map(|s| s.auto_update_check)
                    .unwrap_or(true);
                let engine = reposync_core::git::SystemGitEngine::new(configured_git_path);
                let initial_git = if engine.availability().is_unavailable() {
                    eprintln!(
                        "warning: git executable not found; git-dependent \
                         actions will report GitNotFound until git is available"
                    );
                    None
                } else {
                    Some(engine)
                };

                // E-15 AC2: reconcile the OS launch-on-login registration against the
                // persisted `autostart` setting now that settings are loaded. The
                // core decides (over a tri-state OS read; a failed query is
                // non-actuating); this call actuates via the plugin. Best-effort -
                // any plugin query/actuation failure is logged, never fatal (see
                // `autostart::reconcile_on_launch`). The plugin manages its
                // AutoLaunchManager during its own setup, which runs before this
                // app-level setup closure, so `autolaunch()` resolves here.
                crate::autostart::reconcile_on_launch(&handle, autostart_on);

                // The SHARED, swappable git handle. `settings_set` re-probes and
                // swaps the inner engine when the user fixes a broken/missing git
                // path (BL-NI-19/BL-NI-26), and the resident scheduler reads the
                // CURRENT engine through this SAME handle each cycle (BL-NI-23), so
                // both the command path and the background loop recover with no
                // restart.
                let git_handle: reposync_core::scheduler::SharedGitEngine =
                    std::sync::Arc::new(tokio::sync::RwLock::new(initial_git));

                // E-13 tray Pause/Resume: the shared, in-memory global-pause flag. The
                // tray menu toggles it and the scheduler reads the SAME handle at the
                // start of every cycle (skipping the whole cycle while paused), so a
                // tray pause suppresses scheduled checks with no restart. Handed to the
                // scheduler via `with_pause` below and to AppState for the tray handler.
                let pause = reposync_core::scheduler::GlobalPause::new();

                // Edge-wiring: spawn the resident scheduler UNCONDITIONALLY
                // (finding 6 / BL-NI-23) and build the shared per-repo lock map.
                // Even when git is absent at startup, the loop runs and its live
                // git-gate skips each cycle cleanly until a settings re-probe swaps
                // an engine into `git_handle` - so "installed without git, add it
                // later, fix the path in Settings" recovers with NO restart. The
                // scheduler reads the current engine through the shared handle each
                // cycle instead of owning a clone. The tick loop is owned here (not
                // `Scheduler::run`) because only the edge holds the `AppHandle`
                // needed to emit `scheduler:tick`; the core scheduler is
                // deliberately Tauri-free (E-08). `locks` is the scheduler's OWN
                // `RepoLocks` (a shared Arc-backed map), handed to AppState so the
                // manual command handlers contend the exact locks the jobs do.
                let locks = {
                    use reposync_core::scheduler::{
                        DbDueQuery, Scheduler, SharedGitEngineSource, SystemClock, SystemJitter,
                        UpdateNowJobRunner, DEFAULT_CONCURRENCY, ONE_MINUTE,
                    };
                    use std::sync::Arc;

                    // E-14: the per-cycle notification buffer. The collecting outcome
                    // writer fills it as each job records its outcome; the tick loop
                    // drains + coalesces it after the cycle's jobs have all joined, so
                    // a multi-repo cycle raises a BOUNDED set of toasts (AC4), not one
                    // per repo. Cloned so the writer and the loop share one buffer.
                    let cycle_notes = crate::notify::CycleNotifications::default();

                    // The clock carries the host's local UTC offset so quiet hours
                    // are evaluated in local time, not UTC. The SAME offset feeds the
                    // notification firing site, so a toast's quiet-hours decision and
                    // the scheduler's agree on "now" (E-14 LocalMinute contract).
                    let scheduler = Scheduler::new(
                        Arc::new(SystemClock::with_utc_offset_minutes(
                            crate::localtime::local_offset_minutes(),
                        )),
                        Arc::new(SystemJitter::new()),
                        Arc::new(SharedGitEngineSource::new(git_handle.clone())),
                        DbDueQuery::new(pool.clone()),
                        UpdateNowJobRunner::new(pool.clone()),
                        crate::notify::CollectingOutcomeWriter::new(
                            handle.clone(),
                            pool.clone(),
                            cycle_notes.clone(),
                        ),
                        DEFAULT_CONCURRENCY,
                    )
                    // E-13: the scheduler honors the shared global-pause flag.
                    .with_pause(pause.clone());
                    let locks = scheduler.locks();
                    let tick_handle = handle.clone();
                    // The firing site reads settings (the notify toggles + quiet
                    // hours) fresh per cycle from this pool clone.
                    let notes_pool = pool.clone();
                    // BL-NI-32: the daily activity-retention sweep runs INSIDE the
                    // resident tick loop (gated once per day) off this pool clone, so a
                    // long-resident tray app prunes old activity rows instead of only
                    // sweeping at startup.
                    let sweep_pool = pool.clone();
                    tauri::async_runtime::spawn(async move {
                        // Startup pass is best-effort; a failure must not kill the loop.
                        if let Err(e) = scheduler.start().await {
                            eprintln!("scheduler: startup pass failed: {e}");
                        }
                        // Fire the startup pass's coalesced notifications too (its jobs
                        // have all joined by the time `start()` returns).
                        crate::notify::fire_cycle_from_collector(
                            &tick_handle,
                            &notes_pool,
                            &cycle_notes,
                        )
                        .await;
                        // BL-NI-32: seed the once-per-day retention-sweep gate to
                        // "just swept" - the startup sweep (above, before the pool
                        // moved into AppState) already ran, so the first tick-driven
                        // sweep happens ~24h later, not immediately.
                        let mut last_sweep_unix: Option<i64> = Some(crate::localtime::now_unix());
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
                                    // Coalesce + raise this cycle's notifications
                                    // (E-14 AC4/AC5): the core decides, the edge fires.
                                    crate::notify::fire_cycle_from_collector(
                                        &tick_handle,
                                        &notes_pool,
                                        &cycle_notes,
                                    )
                                    .await;
                                }
                                Err(e) => eprintln!("scheduler: tick failed: {e}"),
                            }
                            // BL-NI-32: the daily activity-retention sweep, gated once
                            // per day off the same edge clock the tick loop uses. Cheap
                            // (one short DELETE) and it runs AFTER the tick's jobs have
                            // joined, so it never blocks the check cycle. The gate is
                            // advanced on every ATTEMPT (success or failure), so a
                            // transient failure waits for the next day rather than
                            // retrying - and spamming - every minute.
                            let sweep_now = crate::localtime::now_unix();
                            if reposync_core::activity::sweep_due(last_sweep_unix, sweep_now) {
                                match reposync_core::activity::sweep(&sweep_pool, sweep_now).await {
                                    Ok(n) if n > 0 => eprintln!(
                                        "activity: daily retention sweep pruned {n} record(s)"
                                    ),
                                    Ok(_) => {}
                                    Err(e) => {
                                        eprintln!("activity: daily retention sweep failed: {e}")
                                    }
                                }
                                last_sweep_unix = Some(sweep_now);
                            }
                        }
                    });
                    locks
                };

                // E-17: the resident background GitHub metadata + branch/PR
                // intelligence refresh. A budgeted pass over the tracked GitHub repos,
                // oldest-metadata-first, capped by a rolling-hour RateBudgeter so a cold
                // library backfills over several hours rather than bursting past the
                // unauthenticated ceiling (E-17 AC16). Runs the unauthenticated NoToken
                // path only.
                //
                // Finding 2: the budgeter is the SHARED handle in AppState, so the manual
                // `repo_refresh_metadata` command spends against the SAME budget and the
                // two paths can never together overspend the ceiling.
                //
                // Finding 3: after each pass, emit ONE `repo:metadata-refreshed` (only
                // when at least one repo changed) so the aggregate list view refetches
                // exactly once, plus a per-repo `repo:state-changed` for each changed repo
                // so an open drawer refreshes. This is what makes a background PR/release
                // refresh visible to the open UI without an N+1 refetch storm.
                let github_budget = reposync_core::github::shared_budgeter();
                {
                    use reposync_core::github::{refresh_pass, NoToken, ReqwestTransport};
                    let refresh_pool = pool.clone();
                    let refresh_budget = github_budget.clone();
                    let refresh_handle = handle.clone();
                    match ReqwestTransport::new() {
                        Ok(transport) => {
                            tauri::async_runtime::spawn(async move {
                                // A modest tick; the rolling-hour budgeter, the 24h
                                // per-resource window, and the per-endpoint ETags pace the
                                // real request volume, so most ticks are cheap no-ops.
                                let mut interval =
                                    tokio::time::interval(std::time::Duration::from_secs(600));
                                // Consume the immediate first tick so the first real pass
                                // waits one period (startup is busy enough already).
                                interval.tick().await;
                                loop {
                                    interval.tick().await;
                                    let now = crate::localtime::now_unix();
                                    match refresh_pass(
                                        &refresh_pool,
                                        &transport,
                                        &NoToken,
                                        &refresh_budget,
                                        now,
                                    )
                                    .await
                                    {
                                        Ok(report) => {
                                            // Per-repo drawer signal: only the focused
                                            // repo-detail drawer (useRepoBackendEvents,
                                            // scoped) reacts to repo:state-changed, so this
                                            // is not a list-refetch storm.
                                            for id in &report.changed_repo_ids {
                                                crate::events::emit_state_changed(
                                                    &refresh_handle,
                                                    *id,
                                                    None,
                                                );
                                            }
                                            // ONE coalesced list signal per pass that
                                            // actually changed something.
                                            if !report.changed_repo_ids.is_empty() {
                                                crate::events::emit_metadata_refreshed(
                                                    &refresh_handle,
                                                    report.changed_repo_ids.len() as i64,
                                                    now,
                                                );
                                            }
                                        }
                                        Err(e) => eprintln!(
                                            "github: background metadata refresh pass failed: {e}"
                                        ),
                                    }
                                }
                            });
                        }
                        Err(e) => eprintln!(
                            "github: could not build the HTTP client; background \
                             metadata refresh disabled: {e}"
                        ),
                    }
                }

                // Seed the tray's "Open recent" submenu from the most-recently-active
                // repos (E-13) BEFORE the pool moves into AppState. `recent` is owned
                // (it borrows nothing), so it outlives the move below.
                let recent = reposync_core::store::recent_repos(&pool, tray::RECENT_LIMIT)
                    .await
                    .unwrap_or_default();

                handle.manage(AppState {
                    pool,
                    git: git_handle,
                    locks,
                    settings_write_lock: tokio::sync::Mutex::new(()),
                    pause,
                    github_budget,
                    db_recovered,
                    db_backup_path,
                });

                // Build the tray AFTER AppState is managed so a menu click can never
                // race an unmanaged state (the menu handlers read `app.state`). Its
                // success GATES the window lifecycle below: the tray is the only
                // restore/quit path, so close-to-tray and start-minimized are only safe
                // when it actually built (finding 2). A build failure is logged, not
                // fatal.
                let tray_available = match tray::init(&handle, &recent) {
                    Ok(()) => true,
                    Err(e) => {
                        eprintln!("tray: failed to build the tray icon/menu: {e}");
                        false
                    }
                };

                // Window lifecycle (E-13 AC3 + E-15 AC3, P3-C), now that the tray's
                // status is known: WITH a tray, a NORMAL launch shows + focuses the
                // window and an AUTOSTART launch stays hidden in the tray, and the close
                // button hides-to-tray (only the tray "Quit" exits). WITHOUT a tray
                // there is no restore path, so we never start hidden and never intercept
                // the close - even an autostart launch ends visible and quittable
                // (finding 2). The window was created hidden (`visible(false)`) in setup
                // (BL-NI-59), so it stays hidden until this shows it, avoiding a flash.
                windows::init(&handle, tray_available);

                // E-18 auto-update: spawn the on-launch update check in the
                // background, gated by the `auto_update_check` toggle AND the
                // ship-dark state (a build with no production signing key does not
                // check). If an update is available it surfaces a non-blocking OS
                // toast; it never auto-installs. Detached so a slow network never
                // delays startup, and best-effort so a check failure (the common
                // inert private-repo 404) is log-only, not an error toast.
                if updates::should_run_launch_check(
                    auto_update_on,
                    updates::updater_is_live(&updates::configured_pubkey(&handle)),
                ) {
                    let update_handle = handle.clone();
                    tauri::async_runtime::spawn(async move {
                        updates::run_launch_check(&update_handle).await;
                    });
                }
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

#[cfg(test)]
mod nav_guard_tests {
    use super::allow_navigation;

    #[test]
    fn allows_the_app_origin_on_every_platform() {
        // macOS/Linux production origin: tauri://localhost.
        assert!(allow_navigation("tauri", Some("localhost"), false));
        // Windows production origin: http(s)://tauri.localhost. Host-based, so either
        // scheme (default http, or https when useHttpsScheme is set) is allowed.
        assert!(allow_navigation("http", Some("tauri.localhost"), false));
        assert!(allow_navigation("https", Some("tauri.localhost"), false));
    }

    #[test]
    fn allows_the_dev_server_only_in_dev() {
        // Under `tauri dev` the Vite dev server (http://localhost:1420) is the origin.
        assert!(allow_navigation("http", Some("localhost"), true));
        // In a shipped build (is_dev == false) the dev-server host is NOT allowed - even
        // a `tauri build --debug` bundle (debug_assertions on) loads from tauri.localhost,
        // so gating on is_dev rather than debug_assertions avoids a needless hole.
        assert!(!allow_navigation("http", Some("localhost"), false));
    }

    #[test]
    fn denies_external_navigation() {
        assert!(!allow_navigation("https", Some("evil.example"), false));
        assert!(!allow_navigation("https", Some("evil.example"), true));
        // Exact host match: neither a look-alike subdomain nor a suffix/prefix slips in.
        assert!(!allow_navigation(
            "https",
            Some("tauri.localhost.evil.example"),
            false
        ));
        assert!(!allow_navigation(
            "https",
            Some("evil-tauri.localhost"),
            false
        ));
        // An opaque origin with no host (e.g. data:) is denied.
        assert!(!allow_navigation("data", None, false));
    }
}
