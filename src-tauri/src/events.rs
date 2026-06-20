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

use reposync_core::error::AppErrorPayload;
use reposync_core::ipc::{
    CheckCompletedPayload, CheckResult, CheckStartedPayload, NotificationFiredPayload,
    SchedulerTickPayload, StateChangedPayload, UpdateCompletedPayload, UpdateStartedPayload,
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
