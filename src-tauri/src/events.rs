//! Backend to frontend event emission for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) for the stub; E-06 (IPC contract) for the
//! payload types; E-12 (tracer bullet) wires the first real event.
//!
//! Long-running work in `reposync-core` surfaces to the UI as typed Tauri events
//! emitted from here. The event payload type ([`reposync_core::ipc`]) lives in
//! the Tauri-free core; the `tauri_specta::Event` wrapper and the emit helper
//! are the only Tauri-aware pieces and stay in this shell.

use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;

use reposync_core::error::{AppError, AppErrorPayload};
use reposync_core::ipc::{
    CheckCompletedPayload, CheckResult, CheckStartedPayload, MetadataRefreshedPayload,
    NavigateRequestedPayload, NotificationFiredPayload, SchedulerTickPayload, StateChangedPayload,
    UpdateCompletedPayload, UpdateStartedPayload,
};

/// Typed "check completed" event, broadcast after every `repo_check_now`.
///
/// The wire name is pinned to `repo:check-completed` (the E-06 contract);
/// without the explicit `event_name` the derive would emit `check-completed`
/// from the struct identifier. The frontend listens via the generated
/// `events.checkCompleted` binding, never a raw string.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:check-completed")]
pub struct CheckCompleted(pub CheckCompletedPayload);

// The remaining E-06 event surface. Each wraps a Tauri-free payload from
// `reposync_core::ipc` and pins its wire name explicitly so the generated
// binding key and the listened-for string stay stable across efforts. The real
// emit sites land in their owning efforts (E-07/E-08/E-09/E-11); the derives
// freeze the contract now.

/// Typed `repo:state-changed` event (a repo's cached state was updated).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:state-changed")]
pub struct StateChanged(pub StateChangedPayload);

/// Typed `repo:check-started` event (a check began for a repo).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:check-started")]
pub struct CheckStarted(pub CheckStartedPayload);

/// Typed `repo:update-started` event (an update began for a repo).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:update-started")]
pub struct UpdateStarted(pub UpdateStartedPayload);

/// Typed `repo:update-completed` event (an update finished for a repo).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:update-completed")]
pub struct UpdateCompleted(pub UpdateCompletedPayload);

/// Typed `scheduler:tick` event (the scheduler ran a cycle).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "scheduler:tick")]
pub struct SchedulerTick(pub SchedulerTickPayload);

/// Typed `notification:fired` event (a desktop notification was raised).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "notification:fired")]
pub struct NotificationFired(pub NotificationFiredPayload);

/// Typed `error:raised` event (a backend error reached the global surface).
///
/// DEVIATION from the other six events' `pub struct X(pub Payload)` newtype
/// shape, forced by a tauri-specta rc.25 codegen defect. This is the only event
/// whose payload transitively carries the `serde_json::Value` field
/// (`AppErrorPayload.context`) that the builder's semantic remap rewrites to
/// `unknown`. That remap makes tauri-specta emit a runtime payload transform for
/// the event. For a tuple newtype `ErrorRaised(Payload)`, tauri-specta rc.25
/// walks the newtype body as an unnamed-field struct and indexes the payload as
/// `v[0]`, while the generated TS type (correctly) collapses the newtype to the
/// inner object - so the `v[0]`-indexing transform does not typecheck (verified:
/// `#[serde(transparent)]` / `#[specta(transparent)]` does NOT help, because
/// specta still emits a single unnamed-field struct DataType). Declaring the
/// event as a NAMED single-field struct makes the transform walk the field by
/// name (`v.error...`) instead, which typechecks.
///
/// The wire shape is unchanged: a tuple newtype of `{ error: AppErrorPayload }`
/// and this named struct both serialize to `{ "error": { ...AppErrorPayload } }`,
/// so the frontend contract is identical. The generated TS `ErrorRaised` is
/// `{ error: AppErrorPayload }`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[serde(rename_all = "camelCase")]
#[tauri_specta(event_name = "error:raised")]
pub struct ErrorRaised {
    pub error: AppErrorPayload,
}

/// Typed `repo:metadata-refreshed` event (E-17 finding 3): emitted once per background
/// GitHub metadata refresh pass that changed at least one repo, so the aggregate list
/// view refetches exactly once. Additive E-06 amendment (was a flagged V1.1 surface in
/// the E-17 spec; promoted to V1 to fix the background-refresh-invisible finding).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:metadata-refreshed")]
pub struct MetadataRefreshed(pub MetadataRefreshedPayload);

/// Typed `navigate:requested` event (E-13 tray): the shell asks the UI to switch
/// views.
///
/// Emitted by the tray "Settings" item so it can open + focus the window ON the
/// settings view; the frontend app-shell listens (`events.navigateRequested`) and
/// routes to the named target.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "navigate:requested")]
pub struct NavigateRequested(pub NavigateRequestedPayload);

/// Emit a [`CheckCompleted`] event derived from a finished [`CheckResult`].
///
/// Best-effort: an emit failure (e.g. no webview yet) is swallowed so a check
/// that already succeeded in the core is not reported as failed to the caller.
pub fn emit_check_completed(app: &AppHandle, r: &CheckResult) {
    let payload = CheckCompletedPayload {
        repo_id: r.repo_id,
        decision: r.decision.clone(),
        ahead: r.ahead,
        behind: r.behind,
        checked_at: r.checked_at,
    };
    let _ = CheckCompleted(payload).emit(app);
}

/// Emit the `repo:update-started` event before an update runs (E-07).
///
/// Best-effort, like [`emit_check_completed`]: a missing webview must not fail an
/// update that the core is about to perform.
pub fn emit_update_started(app: &AppHandle, repo_id: i64, mode: &str) {
    let _ = UpdateStarted(UpdateStartedPayload {
        repo_id,
        mode: mode.to_string(),
    })
    .emit(app);
}

/// Emit the `repo:update-completed` event after an update finishes (E-07),
/// carrying the result's stable `outcome` string. Best-effort.
pub fn emit_update_completed(app: &AppHandle, repo_id: i64, outcome: &str) {
    let _ = UpdateCompleted(UpdateCompletedPayload {
        repo_id,
        outcome: outcome.to_string(),
    })
    .emit(app);
}

/// Emit the `scheduler:tick` event after each resident-loop cycle (edge-wiring).
///
/// `checked` and `due` both carry the count of repos the tick actually ran;
/// `tick_once` runs exactly the due set, so the two counts coincide until the
/// scheduler grows a distinct "due but skipped" return. Best-effort like the
/// other emits: a missing webview must never tear down the scheduler loop.
pub fn emit_scheduler_tick(app: &AppHandle, checked: i64, due: i64, at: i64) {
    let _ = SchedulerTick(SchedulerTickPayload { checked, due, at }).emit(app);
}

/// Emit the `repo:check-started` event when a check begins for a repo (BL-NI-31).
///
/// Fired by the manual check paths (`repo_check_now` and tray "Check All Now") so a
/// future UI spinner can reflect an in-flight check. Best-effort like every emit.
pub fn emit_check_started(app: &AppHandle, repo_id: i64) {
    let _ = CheckStarted(CheckStartedPayload { repo_id }).emit(app);
}

/// Emit the `repo:state-changed` event when a repo's cached state changed as a result
/// of a check/update (BL-NI-31). This is the event the frontend subscribes to via
/// `repoStateChanged` (`useBackendEvents` / `useRepoBackendEvents`) to refetch, and it
/// is the emit that unblocks finding 11: it fires on the SCHEDULED completion path
/// (the only per-repo completion that otherwise emits nothing), so the dashboard rows
/// and the open repo-detail drawer refresh on a background check. `last_error_code`
/// carries the repo's current error code (or `None` for a healthy outcome); the
/// frontend re-reads authoritative state on the refetch either way. Best-effort.
pub fn emit_state_changed(app: &AppHandle, repo_id: i64, last_error_code: Option<String>) {
    let _ = StateChanged(StateChangedPayload {
        repo_id,
        last_error_code,
    })
    .emit(app);
}

/// Emit the `error:raised` global-error event for a backend error that has no
/// synchronous caller to receive it (BL-NI-31) - e.g. a failure inside the tray
/// "Check All Now" or "Open recent", which are fire-and-forget. A command that
/// returns its error to the invoking frontend does NOT go through here (the caller
/// toasts the returned error). Best-effort.
pub fn emit_error_raised(app: &AppHandle, err: &AppError) {
    let _ = ErrorRaised {
        error: err.to_payload(),
    }
    .emit(app);
}

/// Emit the `repo:metadata-refreshed` event after a background metadata refresh pass
/// that changed at least one repo (E-17 finding 3). ONE per pass, so the aggregate list
/// view (dashboard / repos) refetches exactly once - the per-repo drawer refresh rides
/// the separate `repo:state-changed` events. `changed_count` is how many repos moved;
/// `at` is the pass timestamp. Best-effort like every emit.
pub fn emit_metadata_refreshed(app: &AppHandle, changed_count: i64, at: i64) {
    let _ = MetadataRefreshed(MetadataRefreshedPayload { changed_count, at }).emit(app);
}

/// Emit the `nav:requested` event asking the frontend to switch to `target` (E-13
/// tray "Settings"). Best-effort; a missing webview just means no navigation.
pub fn emit_navigate(app: &AppHandle, target: &str) {
    let _ = NavigateRequested(NavigateRequestedPayload {
        target: target.to_string(),
    })
    .emit(app);
}
