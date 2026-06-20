//! Backend to frontend event emission for the RepoSync shell.
//!
//! Owning effort: E-01 (Foundation) for the stub; E-06 (IPC contract) for
//! the real event channel and payload types.
//!
//! Long-running work in `reposync-core` (scheduled syncs, activity updates,
//! summary generation) surfaces to the UI as typed Tauri events emitted from
//! here. The event names and payloads are part of the E-06 IPC contract.
//!
// TODO(E-06): define the event enum and typed payloads, and emit them via the
// `AppHandle` from the relevant command/background flows.
