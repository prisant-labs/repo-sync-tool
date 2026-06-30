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
//!   * [`coalesce`] - the per-CYCLE reducer: a scheduler cycle touching many repos
//!     raises a BOUNDED number of toasts (a summary plus a capped set of individual
//!     failure toasts), never one per repo (AC4).
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

/// The max number of INDIVIDUAL failure toasts one cycle raises; beyond this the cycle
/// summary covers the rest, so a large cycle never floods the user (AC4).
pub const MAX_FAILURE_TOASTS: usize = 3;

/// Whether an event passes the gate: not during quiet hours (AC3), and its kind's
/// toggle is on (AC1/AC2). Quiet hours reuse the scheduler's tested predicate.
fn passes_gate(event: &NotifiableEvent, settings: &Settings, now_min: i64) -> bool {
    if in_quiet_hours(
        now_min,
        settings.quiet_hours_start,
        settings.quiet_hours_end,
    ) {
        return false;
    }
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

/// The single cycle-summary toast tallying the releases and failures, omitting a
/// zero category so the body reads naturally.
fn summary_payload(releases: usize, failures: usize) -> NotificationFiredPayload {
    let mut parts: Vec<String> = Vec::new();
    if releases > 0 {
        parts.push(format!(
            "{releases} new release{}",
            if releases == 1 { "" } else { "s" }
        ));
    }
    if failures > 0 {
        parts.push(format!(
            "{failures} failure{}",
            if failures == 1 { "" } else { "s" }
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
    now_min: i64,
) -> Option<NotificationFiredPayload> {
    passes_gate(event, settings, now_min).then(|| build_payload(event))
}

/// Reduce one scheduler cycle's events into a BOUNDED set of toasts (AC4): individual
/// failure toasts capped at [`MAX_FAILURE_TOASTS`] (failures are the actionable ones),
/// plus ONE cycle summary when releases are present (releases are never individual in a
/// cycle) or when failures overflow the cap, so nothing is silently dropped. The total
/// is bounded by `MAX_FAILURE_TOASTS + 1` regardless of cycle size, never one per repo.
pub fn coalesce(
    events: &[NotifiableEvent],
    settings: &Settings,
    now_min: i64,
) -> Vec<NotificationFiredPayload> {
    let mut releases = 0usize;
    let mut failures: Vec<&NotifiableEvent> = Vec::new();
    for e in events {
        if !passes_gate(e, settings, now_min) {
            continue;
        }
        match e.kind {
            NoteKind::Release => releases += 1,
            NoteKind::Failure | NoteKind::Auth => failures.push(e),
        }
    }
    if releases == 0 && failures.is_empty() {
        return Vec::new();
    }

    let mut out: Vec<NotificationFiredPayload> = Vec::new();
    for &e in failures.iter().take(MAX_FAILURE_TOASTS) {
        out.push(build_payload(e));
    }
    if releases > 0 || failures.len() > MAX_FAILURE_TOASTS {
        out.push(summary_payload(releases, failures.len()));
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
    const MIDDAY: i64 = 600;

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
        // repo. 20 failures + 5 releases must collapse to at most the cap + 1 summary.
        let mut events: Vec<NotifiableEvent> = (0..20).map(failure).collect();
        events.extend((100..105).map(release));
        let out = coalesce(&events, &settings(true, true, None), MIDDAY);
        assert!(
            !out.is_empty(),
            "a cycle with notifiable events raises something"
        );
        assert!(
            out.len() <= MAX_FAILURE_TOASTS + 1,
            "bounded to the failure cap plus one summary, got {}",
            out.len()
        );
    }

    #[test]
    fn coalesce_caps_failures_and_summarizes_releases() {
        // The default policy: individual failure toasts capped at MAX_FAILURE_TOASTS,
        // plus one cycle summary that tallies the releases (never individual) and the
        // full failure count.
        let mut events: Vec<NotifiableEvent> = (0..5).map(failure).collect();
        events.extend((100..105).map(release));
        let out = coalesce(&events, &settings(true, true, None), MIDDAY);

        let failures = out.iter().filter(|p| p.kind == "failure").count();
        let summaries = out.iter().filter(|p| p.kind == "summary").count();
        assert_eq!(failures, MAX_FAILURE_TOASTS, "failures capped at the bound");
        assert_eq!(summaries, 1, "exactly one cycle summary");
        let summary = out.iter().find(|p| p.kind == "summary").unwrap();
        assert!(
            summary.body.contains('5'),
            "summary tallies the 5 releases / failures"
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
