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
//!   * the resident scheduler's per-cycle completion, coalesced
//!     ([`fire_cycle_from_collector`], fed by [`CollectingOutcomeWriter`] which
//!     records each failed job into a per-cycle [`CycleNotifications`] buffer); and
//!   * the manual metadata refresh, when it brings in a genuinely new release
//!     ([`fire_one`] with a single [`decide`]).
//!
//! The cycle path drains its buffer ONLY when it is actually going to fire
//! (BL-NI-80). Inside quiet hours, or when the settings read fails, the buffer is
//! left intact and the once-a-minute tick reconsiders it, so a cycle that straddles
//! the start of the quiet window has its alerts withheld rather than destroyed.
//!
//! NOTE: this module is not the only place in the app that raises an OS toast.
//! `updates.rs` raises the launch-time "update available" toast directly, outside
//! this gate entirely (BL-NI-82). Establish that list with a search for the
//! CAPABILITY, not by tracing calls from here:
//!
//! ```text
//! grep -rnE 'notification\(\)|tauri_plugin_notification|NotificationExt' --include=*.rs crates/ src-tauri/
//! ```

use std::sync::{Arc, Mutex};

use sqlx::SqlitePool;
use tauri::AppHandle;
use tauri_plugin_notification::{NotificationExt, PermissionState};
use tauri_specta::Event;

use reposync_core::error::AppError;
use reposync_core::ipc::{NotificationFiredPayload, RepoId, Settings};
use reposync_core::notify::{coalesce, decide, is_quiet, LocalMinute, NoteKind, NotifiableEvent};
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
        tracing::warn!(
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
///
/// PRIVATE on purpose (BL-NI-80). [`fire_cycle_from_collector`] is the only caller,
/// because it is the only place that can decide to HOLD a cycle's events instead of
/// firing them. Reaching this directly means the events have already been drained,
/// and [`coalesce`] filters rather than queues - so a second caller would silently
/// destroy any event the gate rejects, which is exactly the defect fixed here.
fn fire_cycle(app: &AppHandle, settings: &Settings, events: &[NotifiableEvent]) {
    for payload in coalesce(events, settings, local_minute_now()) {
        raise(app, &payload);
    }
}

/// Fire a cycle's collected failures as coalesced toasts, or HOLD them for a later
/// tick. A no-op when the cycle produced no notifiable events, so the common quiet
/// cycle never touches the DB or the plugin.
///
/// Every path that declines to fire leaves the buffer INTACT, which is the fix for
/// BL-NI-80. The tick loop calls this once a minute for the life of the process
/// (`lib.rs`, in the unconditional `Ok(report)` arm), so anything still held is
/// simply reconsidered a minute later.
///
/// The consuming step goes through [`CycleNotifications::drain_if`] rather than a
/// bare drain, so "decide before consuming" is enforced by the type rather than by
/// the order the statements happen to be written in.
pub async fn fire_cycle_from_collector(
    app: &AppHandle,
    pool: &SqlitePool,
    collector: &CycleNotifications,
) {
    // Peek rather than drain: the settings read and the quiet-hours check below both
    // have to be able to bail out without consuming anything.
    if collector.is_empty() {
        return;
    }
    let settings = match reposync_core::store::settings_get(pool).await {
        Ok(settings) => settings,
        Err(e) => {
            // HOLD, do not drop. The previous order drained before this read, so a
            // transient settings failure destroyed the cycle's notifications outright
            // with nothing to retry - the same permanent-loss shape as the quiet-hours
            // boundary, reached through a different door.
            tracing::warn!(
                "notify: could not read settings to fire cycle notifications, holding {} event(s) for the next tick: {e}",
                collector.len()
            );
            return;
        }
    };
    // BL-NI-80: the quiet-hours decision gates the DRAIN, not the raise.
    //
    // The scheduler gates a cycle at its START (`run_due` returns early inside the
    // window), but a cycle that began at 21:59 can finish at 22:00. Judging at fire
    // time meant `coalesce` saw a quiet clock and FILTERED every event, and the drain
    // had already emptied the buffer, so those alerts were gone permanently and the
    // next cycle had nothing left to re-report.
    //
    // Holding satisfies AC3 ("no toast is raised during quiet hours") without losing
    // the alert: the first tick after the window ends fires these alongside that
    // cycle's own events, coalesced together into one bounded batch.
    //
    // The held buffer cannot grow while the window is open, which is what makes this
    // a hold rather than an unbounded queue: inside the window `run_due` returns
    // before running any job, so `CollectingOutcomeWriter::record` never pushes. What
    // is held is exactly the one straddling cycle's events.
    //
    // Held events are in-memory and do not survive a restart. That is the intended
    // trade: the failure itself is persisted (`last_error_code`, the activity log,
    // and the Repos + attention surfaces read from it), so what a restart costs is
    // the alert, never the record.
    let Some(events) = collector.drain_if(|| !is_quiet(&settings, local_minute_now())) else {
        return;
    };
    fire_cycle(app, &settings, &events);
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
            Ok(other) => tracing::warn!(
                "notify: notification permission not granted ({other:?}); \
                 OS toasts will be suppressed until the user enables them"
            ),
            Err(e) => tracing::warn!("notify: could not request notification permission: {e}"),
        },
        Err(e) => tracing::warn!("notify: could not read notification permission state: {e}"),
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
    /// Take the buffered events ONLY if `may_fire` approves, and otherwise leave them
    /// held for a later tick. Returns `None` when the buffer is empty or firing was
    /// refused.
    ///
    /// This is the ONLY way to consume the buffer. There is deliberately no
    /// unconditional `drain`, because an unconditional drain is the BL-NI-80 defect:
    /// the scheduler joins all of a cycle's jobs before the tick loop returns, so
    /// whoever consumes here holds exactly one cycle's events, and if the reason not
    /// to fire them arrives after the drain they are simply gone.
    ///
    /// The decision and the drain are ONE operation on purpose (BL-NI-80). Expressed
    /// as two statements they can be written in the wrong order, and the wrong order
    /// is precisely the bug: draining first and asking afterwards discards whatever
    /// the answer refuses, because [`coalesce`] filters rather than queues. Fusing
    /// them means no caller can express the broken sequence, and the guarantee
    /// becomes unit-testable here rather than resting on a comment.
    ///
    /// `may_fire` runs while the buffer lock is held, so it must stay cheap and must
    /// not await - the same rule [`push`](CycleNotifications::push) follows. The real
    /// caller passes a clock read plus a settings comparison, which qualifies.
    pub fn drain_if(&self, may_fire: impl FnOnce() -> bool) -> Option<Vec<NotifiableEvent>> {
        let mut held = self
            .events
            .lock()
            .expect("cycle-notifications map poisoned");
        if held.is_empty() || !may_fire() {
            return None;
        }
        Some(std::mem::take(&mut *held))
    }

    /// Whether the buffer currently holds nothing, WITHOUT consuming it.
    ///
    /// The firing path needs to know there is work before it reads settings, and it
    /// needs to be able to abandon the attempt afterwards without having consumed
    /// anything (BL-NI-80). `drain().is_empty()` cannot answer this question: asking
    /// it is what destroys the answer.
    pub fn is_empty(&self) -> bool {
        self.events
            .lock()
            .expect("cycle-notifications map poisoned")
            .is_empty()
    }

    /// How many events are currently held, WITHOUT consuming them. Used to say how
    /// much is being carried forward when firing is postponed.
    pub fn len(&self) -> usize {
        self.events
            .lock()
            .expect("cycle-notifications map poisoned")
            .len()
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
        //
        // That last sentence was FALSE for failures until BL-NI-04: the manual path
        // returned `Err` on a failed fetch, which short-circuited its own
        // `check-completed` emit, so the scheduled path was the only one that said
        // anything at all when a check went wrong. It is true now.
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

    /// Delegates to the inner DB writer, which is the only participant that can
    /// read the row (BL-NI-72). This wrapper adds notifications, not persistence,
    /// so it has nothing of its own to contribute to the answer.
    async fn current_failures(&self, repo: &DueRepo) -> Result<i64, AppError> {
        self.inner.current_failures(repo).await
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
/// finished job, derived from its persisted [`RepoStatus`].
///
/// This DELEGATES to `reposync_core::policy::status_error_code` rather than
/// deciding anything itself, and that is the whole point. The same classifier now
/// writes `repo_local_state.last_error_code`, so the hint on the event and the
/// value the frontend reads back on its refetch are the same fact by construction.
/// This function previously owned a private copy of the mapping while nothing
/// wrote the column at all, which meant the event was right and every subsequent
/// read of the same thing returned `NULL`.
fn status_error_code(status: RepoStatus) -> Option<String> {
    reposync_core::policy::status_error_code(status).map(str::to_string)
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
        assert!(buf.drain_if(|| true).is_none(), "starts empty");
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
        let drained = buf.drain_if(|| true).expect("the cycle's events");
        assert_eq!(drained.len(), 2, "drain returns the buffered events");
        assert!(
            buf.drain_if(|| true).is_none(),
            "drain leaves the buffer empty"
        );
    }

    #[test]
    fn cycle_notifications_peeks_without_consuming() {
        // BL-NI-80. The firing path asks `is_empty` BEFORE it reads settings, and
        // may then abandon the attempt - settings unreadable - without having
        // consumed anything, so the next tick can reconsider. The old code asked with
        // `drain().is_empty()`, which cannot serve that purpose: it destroys the
        // answer in the act of asking, and that is the whole defect.
        let buf = CycleNotifications::default();
        assert!(buf.is_empty(), "starts empty");
        assert_eq!(buf.len(), 0, "starts empty");

        buf.push(NotifiableEvent {
            kind: NoteKind::Failure,
            repo_id: 1,
            repo_name: "straddler".into(),
            detail: None,
        });

        // The tick loop asks once a minute for as long as the events stay held, so
        // repeated peeking must be free of side effects.
        for attempt in 1..=3 {
            assert!(!buf.is_empty(), "peek {attempt} still sees the held event");
            assert_eq!(buf.len(), 1, "peek {attempt} did not consume");
        }

        assert_eq!(
            buf.drain_if(|| true).expect("still there").len(),
            1,
            "the event survived every peek and is still there to fire"
        );
        assert!(buf.is_empty(), "draining is the only thing that empties it");
    }

    #[test]
    fn drain_if_holds_the_buffer_when_firing_is_refused() {
        // BL-NI-80, the load-bearing guarantee. A refused fire must leave the events
        // exactly where they were, because `coalesce` filters rather than queues: an
        // event drained and then refused is not deferred, it is destroyed. That is
        // the whole defect, and this is the assertion that would catch its return.
        //
        // Refusal here stands for the real caller's `is_quiet(...)` verdict; the
        // clock and the settings are the core's business and are pinned there.
        let buf = CycleNotifications::default();

        // An empty buffer yields nothing even when firing is allowed, so the common
        // quiet cycle stays a no-op.
        assert!(
            buf.drain_if(|| true).is_none(),
            "an empty buffer has nothing to fire"
        );

        buf.push(NotifiableEvent {
            kind: NoteKind::Failure,
            repo_id: 1,
            repo_name: "straddler".into(),
            detail: None,
        });

        // The tick loop asks once a minute for as long as the window stays open, so
        // a refusal has to be repeatable AND non-destructive every single time.
        for minute in 1..=3 {
            assert!(
                buf.drain_if(|| false).is_none(),
                "refusal {minute} yields no events to fire"
            );
            assert_eq!(
                buf.len(),
                1,
                "refusal {minute} left the event HELD, not consumed"
            );
        }

        // The first tick after the window ends gets them, intact.
        let fired = buf
            .drain_if(|| true)
            .expect("approval yields the held events");
        assert_eq!(fired.len(), 1, "every held event survived to be reported");
        assert_eq!(fired[0].repo_name, "straddler");
        assert!(buf.is_empty(), "approval is what consumes the buffer");
    }
}
