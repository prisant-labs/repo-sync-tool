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

use reposync_core::ipc::{CheckCompletedPayload, CheckResult};

/// Typed "check completed" event, broadcast after every `repo_check_now`.
///
/// The wire name is pinned to `repo:check-completed` (the E-06 contract);
/// without the explicit `event_name` the derive would emit `check-completed`
/// from the struct identifier. The frontend listens via the generated
/// `events.checkCompleted` binding, never a raw string.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type, Event)]
#[tauri_specta(event_name = "repo:check-completed")]
pub struct CheckCompleted(pub CheckCompletedPayload);

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
