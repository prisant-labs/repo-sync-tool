//! notify (edge) - the OS-toast + event-emit half of E-14 desktop notifications.
//!
//! Owning effort: E-14 (desktop notifications), edge-wiring portion (AC5).
//!
//! The PURE firing decision and per-cycle coalescing live in the Tauri-free
//! [`reposync_core::notify`] core ([`decide`] / [`coalesce`]): given a notifiable
//! event plus the persisted settings (the notify toggles + quiet-hours window) and
//! the current local minute, the core decides WHETHER a toast should fire and WHAT
//! it should say. This edge module is the thin actuator the core cannot be: it
//! sources the local minute from the SAME UTC offset the scheduler's clock uses
//! (so notifications and scheduling agree on "now"), reads the settings, asks the
//! core, then raises each OS toast via `tauri-plugin-notification` and emits the
//! typed `notification:fired` event so the frontend can mirror it. reposync-core
//! stays Tauri-free: the plugin call and the emit live only here.
//!
//! Two firing sites, one chokepoint:
//!   * the resident scheduler's per-cycle completion, coalesced ([`fire_cycle`],
//!     fed by [`CollectingOutcomeWriter`] which records each failed job into a
//!     per-cycle [`CycleNotifications`] buffer the tick loop drains); and
//!   * the manual metadata refresh, when it brings in a genuinely new release
//!     ([`fire_one`] with a single [`decide`]).

use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};
use tauri_specta::Event;

use reposync_core::error::AppError;
use reposync_core::ipc::{NotificationFiredPayload, RepoId, Settings};
use reposync_core::notify::{coalesce, decide, LocalMinute, NoteKind, NotifiableEvent};
use reposync_core::policy::RepoStatus;
use reposync_core::scheduler::{local_minutes_at, DbOutcomeWriter, DueRepo, OutcomeWriter};

use crate::events::NotificationFired;

/// Minute 0 (local midnight), the defensive fallback for [`local_minute_now`].
/// [`local_minutes_at`] is always in `0..=1439`, so the fallback is unreachable in
/// practice; it exists only so a wildly-wrong clock can never panic the edge.
const MIDNIGHT: LocalMinute = match LocalMinute::new(0) {
    Some(m) => m,
    None => panic!("0 is a valid minute-of-day"),
};

/// The current local minute-of-day, derived from the SAME injected UTC offset the
/// scheduler's `SystemClock` uses ([`crate::localtime::local_offset_minutes`]) via
/// the core's pure [`local_minutes_at`], so a quiet-hours decision at a firing site
/// agrees with the scheduler's own quiet-hours gate (the [`LocalMinute`] contract:
/// the offset is owned at the edge, not in the core).
fn local_minute_now() -> LocalMinute {
    let minute = local_minutes_at(
        crate::localtime::now_unix(),
        crate::localtime::local_offset_minutes(),
    );
    LocalMinute::new(minute).unwrap_or(MIDNIGHT)
}

/// Raise ONE OS toast and emit the typed `notification:fired` event for it.
///
/// Best-effort on BOTH sides, by design: a plugin `show` failure (permission
/// denied, no notification service, packaged-only quirk) is logged and swallowed
/// so it never propagates into the check/scheduler pipeline (the underlying work
/// already ran and was logged; only the toast is lost), and the emit is
/// best-effort like every other event (a missing webview must not tear down the
/// caller).
fn raise(app: &AppHandle, payload: &NotificationFiredPayload) {
    if let Err(e) = app
        .notification()
        .builder()
        .title(payload.title.clone())
        .body(payload.body.clone())
        .show()
    {
        eprintln!(
            "notify: failed to raise OS toast (kind={}): {e}",
            payload.kind
        );
    }
    // Emit regardless of the OS toast result: the frontend mirror should reflect a
    // fired notification even if the OS suppressed the visible toast.
    let _ = NotificationFired(payload.clone()).emit(app);
}

/// Fire the notification for ONE event (a manual command path): the core decides
/// (the toggle + quiet-hours gate), the edge raises + emits. A no-op when the core
/// decides to stay silent (toggle off, or inside quiet hours - AC3).
pub fn fire_one(app: &AppHandle, settings: &Settings, event: &NotifiableEvent) {
    if let Some(payload) = decide(event, settings, local_minute_now()) {
        raise(app, &payload);
    }
}

/// Fire the COALESCED notifications for a whole scheduler cycle: the core reduces
/// the cycle's events to a bounded set (each kind shown individually up to a cap,
/// then one overflow summary - AC4), the edge raises + emits each. Quiet hours and
/// the toggles are applied inside [`coalesce`], so this stays a dumb actuator.
pub fn fire_cycle(app: &AppHandle, settings: &Settings, events: &[NotifiableEvent]) {
    for payload in coalesce(events, settings, local_minute_now()) {
        raise(app, &payload);
    }
}

/// Drain a cycle's collected failures and fire their coalesced toasts. Reads the
/// settings once per cycle (the toggles + quiet hours that gate firing). A no-op
/// when the cycle raised no notifiable events, so the common quiet cycle never
/// touches the DB or the plugin.
pub async fn fire_cycle_from_collector(
    app: &AppHandle,
    pool: &SqlitePool,
    collector: &CycleNotifications,
) {
    let events = collector.drain();
    if events.is_empty() {
        return;
    }
    match reposync_core::store::settings_get(pool).await {
        Ok(settings) => fire_cycle(app, &settings, &events),
        Err(e) => eprintln!("notify: could not read settings to fire cycle notifications: {e}"),
    }
}

/// Best-effort notification-permission reconciliation, run once at startup. On
/// desktop (Windows-first) an installed app is Granted by default, so this is
/// usually a no-op; where the state is not Granted we request it once and log the
/// result. A denial is LOGGED, never fatal - a check must never fail because
/// toasts are off (the task's permission-graceful requirement). Firing itself also
/// swallows its own failure ([`raise`]), so this is purely an early, clearer log.
pub fn ensure_permission(app: &AppHandle) {
    match app.notification().permission_state() {
        Ok(PermissionState::Granted) => {}
        Ok(_) => match app.notification().request_permission() {
            Ok(PermissionState::Granted) => {}
            Ok(other) => eprintln!(
                "notify: notification permission not granted ({other:?}); \
                 OS toasts will be suppressed until the user enables them"
            ),
            Err(e) => eprintln!("notify: could not request notification permission: {e}"),
        },
        Err(e) => eprintln!("notify: could not read notification permission state: {e}"),
    }
}

// =============================================================================
// The scheduler per-cycle collector (the failure/auth firing path).
// =============================================================================

/// A per-cycle buffer of the scheduler's notifiable failures, shared between the
/// [`CollectingOutcomeWriter`] (which fills it as each job records its outcome) and
/// the tick loop (which [`drain`](CycleNotifications::drain)s it after the cycle's
/// jobs have all joined, then coalesces). Cheap to clone (an `Arc`), so both the
/// writer and the loop hold the same buffer.
#[derive(Clone, Default)]
pub struct CycleNotifications {
    events: Arc<Mutex<Vec<NotifiableEvent>>>,
}

impl CycleNotifications {
    /// Take the buffered events, leaving it empty for the next cycle. The scheduler
    /// joins ALL of a cycle's jobs before the tick loop returns, so a drain right
    /// after `start()`/`tick_once()` sees exactly that cycle's events with no
    /// overlap from the next.
    pub fn drain(&self) -> Vec<NotifiableEvent> {
        std::mem::take(
            &mut *self
                .events
                .lock()
                .expect("cycle-notifications map poisoned"),
        )
    }

    /// Buffer one notifiable event. The lock is held only for the push (never
    /// across an await), so this stays cheap under the scheduler's concurrent jobs.
    fn push(&self, event: NotifiableEvent) {
        self.events
            .lock()
            .expect("cycle-notifications map poisoned")
            .push(event);
    }
}

/// An [`OutcomeWriter`] that persists via the inner [`DbOutcomeWriter`] AND buffers
/// a notifiable event for each FAILED job into a shared [`CycleNotifications`], so
/// the edge can coalesce the cycle's failures into a bounded set of toasts (AC4).
/// A successful job clears failure state and produces NO event.
///
/// Persistence is the load-bearing effect and runs FIRST; the notification buffer
/// is a best-effort side effect layered after it. Release notifications are NOT
/// produced here: the scheduled path performs a git fetch/pull (E-07), not a
/// GitHub release refresh, so the only notifiable scheduled outcomes are failures
/// and auth failures. Release toasts fire on the manual metadata refresh instead
/// (a durable background release cadence is the deferred BL-NI-15b work).
pub struct CollectingOutcomeWriter {
    app: AppHandle,
    inner: DbOutcomeWriter,
    pool: SqlitePool,
    collector: CycleNotifications,
}

impl CollectingOutcomeWriter {
    pub fn new(
        app: AppHandle,
        pool: SqlitePool,
        collector: CycleNotifications,
    ) -> CollectingOutcomeWriter {
        CollectingOutcomeWriter {
            app,
            inner: DbOutcomeWriter::new(pool.clone()),
            pool,
            collector,
        }
    }
}

impl OutcomeWriter for CollectingOutcomeWriter {
    async fn record(
        &self,
        repo: &DueRepo,
        now_unix: i64,
        status: RepoStatus,
    ) -> Result<(), AppError> {
        // Persist FIRST (the schedule + failure-counter write is load-bearing);
        // the event emit + notification buffer are best-effort side effects after it.
        self.inner.record(repo, now_unix, status).await?;
        // Emit `repo:state-changed` for this scheduled completion (BL-NI-31 / finding
        // 11). The scheduled path is the only per-repo completion that otherwise emits
        // NOTHING the frontend hears, so this is what makes the dashboard rows and the
        // open repo-detail drawer refresh on a BACKGROUND check. The manual command
        // paths emit their own check/update-completed events. Best-effort.
        crate::events::emit_state_changed(&self.app, repo.id.0, status_error_code(status));
        if let Some(kind) = note_kind_for(status) {
            // Resolve the repo name for a human toast body. Only the exceptional
            // FAILURE path pays this read; a successful job never queries here.
            let repo_name = repo_name_or_fallback(&self.pool, repo.id).await;
            self.collector.push(NotifiableEvent {
                kind,
                repo_id: repo.id.0,
                repo_name,
                detail: None,
            });
        }
        Ok(())
    }
}

/// The notifiable-event kind for a persisted [`RepoStatus`], or `None` for a
/// successful run (which raises no toast). An auth pause is a distinct
/// [`NoteKind::Auth`] (so the toast copy can be specific), while a transient retry
/// and the 3-strikes auto-pause are both [`NoteKind::Failure`] - all three are
/// gated by the single `notify_on_failure` toggle in the core (BL-NI-17: a
/// separate always-on auth-notification policy is a V1.1 enhancement).
fn note_kind_for(status: RepoStatus) -> Option<NoteKind> {
    match status {
        RepoStatus::Active => None,
        RepoStatus::PausedOnAuth => Some(NoteKind::Auth),
        RepoStatus::Retry { .. } | RepoStatus::AutoPaused => Some(NoteKind::Failure),
    }
}

/// The stable error code carried on a scheduled `repo:state-changed` payload for a
/// finished job, derived from its persisted [`RepoStatus`]: `None` for a healthy run,
/// and the matching [`AppError`](reposync_core::error::AppError) code for a failing
/// one (an auth pause vs a transient/auto-paused failure). The frontend re-reads
/// authoritative state on the refetch, so this is an informational hint on the event,
/// kept consistent with the frozen error-code vocabulary rather than an invented one.
fn status_error_code(status: RepoStatus) -> Option<String> {
    match status {
        RepoStatus::Active => None,
        RepoStatus::PausedOnAuth => Some("git.auth_failed".to_string()),
        RepoStatus::Retry { .. } | RepoStatus::AutoPaused => Some("git.fetch_failed".to_string()),
    }
}

/// The repo's display name for a toast body, or a `repo {id}` fallback if the read
/// fails (e.g. the repo was removed mid-cycle). Only the rare failure path calls
/// this, so the extra read is cheap in aggregate.
async fn repo_name_or_fallback(pool: &SqlitePool, id: RepoId) -> String {
    reposync_core::store::repo_get(pool, id)
        .await
        .map(|d| d.local_name)
        .unwrap_or_else(|_| format!("repo {}", id.0))
}

// =============================================================================
// Manual-path release detection (the release firing path).
// =============================================================================

/// Whether a metadata refresh brought in a release worth toasting, given the
/// release tag BEFORE and AFTER the refresh: `Some(new_tag)` only when the tag is
/// now present AND differs from what was cached (a first-seen or advanced release);
/// `None` when there is no release, or the same release is re-observed, or a
/// release was removed upstream. Pure, so the "is this a new release" rule is
/// unit-tested without a webview or a network.
pub fn release_change<'a>(before: Option<&str>, after: Option<&'a str>) -> Option<&'a str> {
    match after {
        Some(tag) if before != Some(tag) => Some(tag),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn note_kind_maps_status_to_notifiable_kind() {
        // A successful run raises nothing; a transient retry and the 3-strikes
        // auto-pause are failures; an auth pause is the distinct Auth kind. (AC2 +
        // BL-NI-17: all failure kinds share the notify_on_failure toggle in core.)
        assert!(note_kind_for(RepoStatus::Active).is_none());
        assert_eq!(
            note_kind_for(RepoStatus::Retry {
                consecutive_failures: 1
            }),
            Some(NoteKind::Failure)
        );
        assert_eq!(
            note_kind_for(RepoStatus::AutoPaused),
            Some(NoteKind::Failure)
        );
        assert_eq!(
            note_kind_for(RepoStatus::PausedOnAuth),
            Some(NoteKind::Auth)
        );
    }

    #[test]
    fn status_error_code_maps_status_to_the_frozen_vocabulary() {
        // A healthy run carries no error code; an auth pause and a transient/auto-pause
        // failure map to their frozen AppError codes (BL-NI-31 state-changed hint).
        assert!(status_error_code(RepoStatus::Active).is_none());
        assert_eq!(
            status_error_code(RepoStatus::PausedOnAuth).as_deref(),
            Some("git.auth_failed")
        );
        assert_eq!(
            status_error_code(RepoStatus::Retry {
                consecutive_failures: 2
            })
            .as_deref(),
            Some("git.fetch_failed")
        );
        assert_eq!(
            status_error_code(RepoStatus::AutoPaused).as_deref(),
            Some("git.fetch_failed")
        );
    }

    #[test]
    fn release_change_fires_only_on_a_new_tag() {
        // First-seen release fires; an advanced tag fires; the SAME tag re-observed
        // is silent; no-release-now and a removed release are silent (AC1: a
        // completed refresh that DETECTS a new release raises one toast).
        assert_eq!(release_change(None, Some("v1.0.0")), Some("v1.0.0"));
        assert_eq!(
            release_change(Some("v1.0.0"), Some("v1.1.0")),
            Some("v1.1.0")
        );
        assert_eq!(release_change(Some("v1.0.0"), Some("v1.0.0")), None);
        assert_eq!(release_change(None, None), None);
        assert_eq!(release_change(Some("v1.0.0"), None), None);
    }

    #[test]
    fn cycle_notifications_drains_then_empties() {
        // The buffer hands the tick loop exactly the cycle's events and resets, so
        // the next cycle starts clean (no cross-cycle leakage into coalescing).
        let buf = CycleNotifications::default();
        assert!(buf.drain().is_empty(), "starts empty");
        buf.push(NotifiableEvent {
            kind: NoteKind::Failure,
            repo_id: 1,
            repo_name: "a".into(),
            detail: None,
        });
        buf.push(NotifiableEvent {
            kind: NoteKind::Auth,
            repo_id: 2,
            repo_name: "b".into(),
            detail: None,
        });
        let drained = buf.drain();
        assert_eq!(drained.len(), 2, "drain returns the buffered events");
        assert!(buf.drain().is_empty(), "drain leaves the buffer empty");
    }
}
