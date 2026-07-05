//! notify - owned by E-14 (desktop notifications: the firing decision + coalescing).
//!
//! The PURE, Tauri-free half of notifications (AC6): given a notifiable event (a new
//! release, a check/update failure, an auth failure) plus the settings and the local
//! time, decide whether a toast should fire and what it should say. The OS toast call
//! and the `notification:fired` emit live in `src-tauri` (the thin edge); this module
//! has no plugin or UI dependency, so the whole firing matrix is unit-testable.
//!
//! Two entry points share one gate:
//!   * [`decide`] - the per-EVENT decision (one completed check that detects a release
//!     or a failure raises one toast, AC1/AC2), suppressed by the toggles and during
//!     quiet hours (AC3).
//!   * [`coalesce`] - the per-CYCLE reducer: a scheduler cycle touching many repos raises
//!     a BOUNDED number of toasts (each kind shown individually up to a cap, then one
//!     summary for the overflow), never one per repo (AC4).
//!
//! Quiet-hours evaluation reuses the scheduler's tested [`in_quiet_hours`] predicate,
//! so notifications and scheduling agree on the window semantics.

use crate::ipc::{NotificationFiredPayload, Settings};
use crate::scheduler::in_quiet_hours;

/// The kind of notifiable event. Auth failure is distinct so the toast can be
/// specific, but it is gated by the same `notify_on_failure` toggle as a failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoteKind {
    /// A new upstream release was detected.
    Release,
    /// A check or update failed (fetch/update error).
    Failure,
    /// Authentication failed (a failure subtype, gated by `notify_on_failure`).
    Auth,
}

/// One thing worth (maybe) telling the user about, produced at a check/update
/// completion. The `detail` carries the release tag (for [`NoteKind::Release`]) or the
/// failure reason (for [`NoteKind::Failure`] / [`NoteKind::Auth`]).
#[derive(Debug, Clone)]
pub struct NotifiableEvent {
    pub kind: NoteKind,
    pub repo_id: i64,
    pub repo_name: String,
    pub detail: Option<String>,
}

/// Local wall-clock minutes since midnight (`0..=1439`), the unit quiet hours needs.
///
/// A newtype (Codex review finding 4) so a caller cannot accidentally pass unix seconds,
/// a raw timestamp, or UTC where LOCAL time is required - the name and the validated
/// range make the contract explicit at the boundary that is about to be wired. The edge
/// MUST source this from the SAME offset-aware scheduler clock used for scheduling
/// ([`crate::scheduler::Clock::local_minutes_of_day`]); the UTC-to-local offset is owned
/// there (the deferred `SystemClock` offset), not here, so notifications and the
/// scheduler agree on "now".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalMinute(i64);

impl LocalMinute {
    /// Construct from minutes-since-midnight, validating `0..=1439`. Returns `None` for
    /// an out-of-range value (a sign the caller passed seconds or a raw timestamp).
    #[allow(clippy::manual_range_contains)]
    pub const fn new(minute: i64) -> Option<LocalMinute> {
        if 0 <= minute && minute <= 1439 {
            Some(LocalMinute(minute))
        } else {
            None
        }
    }

    /// The wrapped minute-of-day.
    fn get(self) -> i64 {
        self.0
    }
}

/// The max INDIVIDUAL toasts of EACH kind (release, failure) one cycle raises before the
/// overflow folds into a single summary. Identity is preserved for the common small
/// cycle, and a large cycle stays bounded (AC4) without dropping or double-reporting any
/// event.
pub const MAX_INDIVIDUAL_TOASTS: usize = 3;

/// Whether an event passes the gate: not during quiet hours (AC3), and its kind's
/// toggle is on (AC1/AC2). Quiet hours reuse the scheduler's tested predicate.
fn passes_gate(event: &NotifiableEvent, settings: &Settings, now: LocalMinute) -> bool {
    if in_quiet_hours(
        now.get(),
        settings.quiet_hours_start,
        settings.quiet_hours_end,
    ) {
        return false;
    }
    // NOTE (Codex review finding 4): auth is gated by `notify_on_failure`, per the spec
    // (E-14 AC2: "a failed check/update or an auth failure raises a toast when
    // notify_on_failure is on, and none when off"); the frozen settings schema has no
    // separate auth toggle. A dedicated critical-/auth-notification policy (always-on, or
    // its own toggle) is a V1.1 enhancement tracked as BL-NI-17.
    match event.kind {
        NoteKind::Release => settings.notify_on_release,
        NoteKind::Failure | NoteKind::Auth => settings.notify_on_failure,
    }
}

/// Build the per-event toast payload (the frozen `ipc::NotificationFiredPayload`).
fn build_payload(event: &NotifiableEvent) -> NotificationFiredPayload {
    let (kind, title, body) = match event.kind {
        NoteKind::Release => {
            let body = match &event.detail {
                Some(tag) => format!("{} published {}", event.repo_name, tag),
                None => format!("{} has a new release", event.repo_name),
            };
            ("release", "New release", body)
        }
        NoteKind::Failure => {
            let body = match &event.detail {
                Some(reason) => format!("{}: {}", event.repo_name, reason),
                None => format!("{} failed to update", event.repo_name),
            };
            ("failure", "Check failed", body)
        }
        NoteKind::Auth => (
            "auth",
            "Authentication needed",
            format!("{} could not authenticate", event.repo_name),
        ),
    };
    NotificationFiredPayload {
        kind: kind.to_string(),
        repo_id: Some(event.repo_id),
        title: title.to_string(),
        body,
    }
}

/// The single OVERFLOW-summary toast for the events beyond the per-kind caps, omitting
/// a zero category so the body reads naturally ("2 more new releases, 5 more failures").
/// It tallies only the overflow, never events already shown individually, so no event is
/// double-reported (Codex review finding 2).
fn summary_payload(more_releases: usize, more_failures: usize) -> NotificationFiredPayload {
    let mut parts: Vec<String> = Vec::new();
    if more_releases > 0 {
        parts.push(format!(
            "{more_releases} more new release{}",
            if more_releases == 1 { "" } else { "s" }
        ));
    }
    if more_failures > 0 {
        parts.push(format!(
            "{more_failures} more failure{}",
            if more_failures == 1 { "" } else { "s" }
        ));
    }
    NotificationFiredPayload {
        kind: "summary".to_string(),
        repo_id: None,
        title: "RepoSync".to_string(),
        body: parts.join(", "),
    }
}

/// Decide whether a single event fires a toast (AC1/AC2/AC3/AC6). Pure: gate, then
/// build the payload. The caller raises the OS toast and emits `notification:fired`.
pub fn decide(
    event: &NotifiableEvent,
    settings: &Settings,
    now: LocalMinute,
) -> Option<NotificationFiredPayload> {
    passes_gate(event, settings, now).then(|| build_payload(event))
}

/// Reduce one scheduler cycle's events into a BOUNDED set of toasts (AC4): each kind
/// (release, failure) is shown individually up to [`MAX_INDIVIDUAL_TOASTS`], so the common
/// small cycle keeps full event identity (Codex review finding 1); then ONE summary covers
/// only the OVERFLOW beyond the caps, so nothing already toasted is repeated (finding 2)
/// and nothing is dropped. The total is bounded by `2 * MAX_INDIVIDUAL_TOASTS + 1`
/// regardless of cycle size, never one per repo.
pub fn coalesce(
    events: &[NotifiableEvent],
    settings: &Settings,
    now: LocalMinute,
) -> Vec<NotificationFiredPayload> {
    let mut releases: Vec<&NotifiableEvent> = Vec::new();
    let mut failures: Vec<&NotifiableEvent> = Vec::new();
    for e in events {
        if !passes_gate(e, settings, now) {
            continue;
        }
        match e.kind {
            NoteKind::Release => releases.push(e),
            NoteKind::Failure | NoteKind::Auth => failures.push(e),
        }
    }
    if releases.is_empty() && failures.is_empty() {
        return Vec::new();
    }

    // Individual toasts up to the per-kind cap, so each event keeps its identity (repo +
    // detail) for the common small cycle - the edge can build a faithful toast and click
    // action (Codex review finding 1).
    let mut out: Vec<NotificationFiredPayload> = Vec::new();
    for &e in releases.iter().take(MAX_INDIVIDUAL_TOASTS) {
        out.push(build_payload(e));
    }
    for &e in failures.iter().take(MAX_INDIVIDUAL_TOASTS) {
        out.push(build_payload(e));
    }
    // One summary for ONLY the overflow beyond the caps, so a large cycle stays bounded
    // (AC4) without repeating an already-toasted event (finding 2) or dropping one.
    let release_overflow = releases.len().saturating_sub(MAX_INDIVIDUAL_TOASTS);
    let failure_overflow = failures.len().saturating_sub(MAX_INDIVIDUAL_TOASTS);
    if release_overflow > 0 || failure_overflow > 0 {
        out.push(summary_payload(release_overflow, failure_overflow));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a Settings with the two toggles and an optional quiet window; the other
    /// fields are defaults that do not affect the firing decision.
    fn settings(notify_release: bool, notify_failure: bool, quiet: Option<(i64, i64)>) -> Settings {
        let (quiet_hours_start, quiet_hours_end) = match quiet {
            Some((s, e)) => (Some(s), Some(e)),
            None => (None, None),
        };
        Settings {
            global_check_minutes: 360,
            quiet_hours_start,
            quiet_hours_end,
            notify_on_release: notify_release,
            notify_on_failure: notify_failure,
            git_executable_path: None,
            editor_command: None,
            terminal_command: None,
            autostart: false,
            activity_retention_d: 90,
            github_token_present: false,
            auto_update_check: true,
        }
    }

    fn release(id: i64) -> NotifiableEvent {
        NotifiableEvent {
            kind: NoteKind::Release,
            repo_id: id,
            repo_name: format!("repo{id}"),
            detail: Some("v1.0.0".into()),
        }
    }
    fn failure(id: i64) -> NotifiableEvent {
        NotifiableEvent {
            kind: NoteKind::Failure,
            repo_id: id,
            repo_name: format!("repo{id}"),
            detail: Some("fetch failed".into()),
        }
    }
    fn auth(id: i64) -> NotifiableEvent {
        NotifiableEvent {
            kind: NoteKind::Auth,
            repo_id: id,
            repo_name: format!("repo{id}"),
            detail: None,
        }
    }

    // 10:00 (600 minutes) - outside the test quiet window (09:00-17:00).
    const MIDDAY: LocalMinute = match LocalMinute::new(600) {
        Some(m) => m,
        None => panic!("600 is a valid minute-of-day"),
    };

    #[test]
    fn local_minute_validates_range() {
        // Codex review finding 4: the type rejects an out-of-range value so a caller
        // cannot pass seconds or a raw timestamp where local minutes-of-day is required.
        assert!(LocalMinute::new(0).is_some());
        assert!(LocalMinute::new(1439).is_some());
        assert!(
            LocalMinute::new(1440).is_none(),
            "minutes-of-day is 0..=1439"
        );
        assert!(LocalMinute::new(-1).is_none());
        assert!(
            LocalMinute::new(86_400).is_none(),
            "a seconds value is rejected"
        );
    }

    #[test]
    fn release_notifies_only_when_enabled() {
        // AC1: a detected release toasts when notify_on_release is on, and is silent
        // when off. The payload carries the kind, the repo, and non-empty copy (AC5).
        let on = decide(&release(7), &settings(true, true, None), MIDDAY).expect("on -> Some");
        assert_eq!(on.kind, "release");
        assert_eq!(on.repo_id, Some(7));
        assert!(!on.title.is_empty() && !on.body.is_empty());

        assert!(
            decide(&release(7), &settings(false, true, None), MIDDAY).is_none(),
            "notify_on_release off -> no release toast"
        );
    }

    #[test]
    fn failure_and_auth_notify_only_when_failure_enabled() {
        // AC2: a failure and an auth failure toast when notify_on_failure is on, silent
        // when off. Auth is a distinct kind but gated by the same toggle.
        let f = decide(&failure(1), &settings(true, true, None), MIDDAY).expect("failure on");
        assert_eq!(f.kind, "failure");
        let a = decide(&auth(1), &settings(true, true, None), MIDDAY).expect("auth on");
        assert_eq!(a.kind, "auth");

        assert!(decide(&failure(1), &settings(true, false, None), MIDDAY).is_none());
        assert!(decide(&auth(1), &settings(true, false, None), MIDDAY).is_none());
    }

    #[test]
    fn quiet_hours_suppress_every_kind() {
        // AC3: during quiet hours NO toast fires, even with both toggles on (the work
        // still ran and was logged; only the toast is withheld).
        let q = settings(true, true, Some((540, 1020))); // 09:00-17:00; MIDDAY is inside
        assert!(decide(&release(1), &q, MIDDAY).is_none());
        assert!(decide(&failure(1), &q, MIDDAY).is_none());
        assert!(decide(&auth(1), &q, MIDDAY).is_none());
    }

    #[test]
    fn toggles_are_independent() {
        // The release and failure toggles gate only their own kinds.
        let rel_off = settings(false, true, None);
        assert!(decide(&release(1), &rel_off, MIDDAY).is_none());
        assert!(decide(&failure(1), &rel_off, MIDDAY).is_some());

        let fail_off = settings(true, false, None);
        assert!(decide(&release(1), &fail_off, MIDDAY).is_some());
        assert!(decide(&failure(1), &fail_off, MIDDAY).is_none());
    }

    #[test]
    fn coalesce_bounds_a_large_cycle() {
        // AC4: a cycle over many repos raises a BOUNDED number of toasts, not one per
        // repo. 20 failures + 5 releases stay within the per-kind caps plus one overflow
        // summary, nowhere near 25.
        let mut events: Vec<NotifiableEvent> = (0..20).map(failure).collect();
        events.extend((100..105).map(release));
        let out = coalesce(&events, &settings(true, true, None), MIDDAY);
        assert!(
            !out.is_empty(),
            "a cycle with notifiable events raises something"
        );
        assert!(
            out.len() <= 2 * MAX_INDIVIDUAL_TOASTS + 1,
            "bounded to the per-kind caps plus one overflow summary, got {}",
            out.len()
        );
    }

    #[test]
    fn coalesce_small_cycle_keeps_individual_identity() {
        // Codex review findings 1 + 2: a small cycle shows each event individually with
        // FULL identity (repo + detail) and never collapses to an anonymous summary or
        // double-reports a failure.
        let out = coalesce(
            &[release(7), failure(9)],
            &settings(true, true, None),
            MIDDAY,
        );
        assert_eq!(
            out.len(),
            2,
            "two events -> two individual toasts, no summary"
        );
        let rel = out
            .iter()
            .find(|p| p.kind == "release")
            .expect("a release toast");
        assert_eq!(rel.repo_id, Some(7), "the release keeps its repo identity");
        assert!(
            rel.body.contains("v1.0.0"),
            "the release keeps its tag detail"
        );
        let fail = out
            .iter()
            .find(|p| p.kind == "failure")
            .expect("a failure toast");
        assert_eq!(fail.repo_id, Some(9));
        assert!(
            out.iter().all(|p| p.kind != "summary"),
            "no redundant summary for a small cycle"
        );
    }

    #[test]
    fn coalesce_large_mixed_cycle_caps_each_kind_and_summarizes_overflow() {
        // Codex review findings 1 + 2: individual toasts per kind up to the cap, then a
        // summary ONLY for the overflow beyond the caps - so nothing already toasted is
        // repeated (no double-report) and nothing is dropped.
        let mut events: Vec<NotifiableEvent> = (0..5).map(release).collect();
        events.extend((100..105).map(failure));
        let out = coalesce(&events, &settings(true, true, None), MIDDAY);

        let releases = out.iter().filter(|p| p.kind == "release").count();
        let failures = out.iter().filter(|p| p.kind == "failure").count();
        let summaries = out.iter().filter(|p| p.kind == "summary").count();
        assert_eq!(
            releases, MAX_INDIVIDUAL_TOASTS,
            "releases shown individually up to the cap"
        );
        assert_eq!(
            failures, MAX_INDIVIDUAL_TOASTS,
            "failures shown individually up to the cap"
        );
        assert_eq!(summaries, 1, "one overflow summary");
        let summary = out.iter().find(|p| p.kind == "summary").unwrap();
        assert!(
            summary.body.contains('2'),
            "the summary tallies the 2-each overflow, got {:?}",
            summary.body
        );
    }

    #[test]
    fn coalesce_suppressed_in_quiet_hours() {
        // AC3 + AC4: quiet hours suppress the whole cycle's toasts.
        let events: Vec<NotifiableEvent> = (0..5).map(failure).collect();
        let out = coalesce(&events, &settings(true, true, Some((540, 1020))), MIDDAY);
        assert!(out.is_empty(), "no toasts during quiet hours");
    }

    #[test]
    fn coalesce_single_failure_has_no_redundant_summary() {
        // A lone failure is shown as itself, with no extra summary toast.
        let out = coalesce(&[failure(1)], &settings(true, true, None), MIDDAY);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].kind, "failure");
    }
}
