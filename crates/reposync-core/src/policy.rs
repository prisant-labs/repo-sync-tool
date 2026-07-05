//! policy - owned by E-07 (the update-policy engine).
//!
//! The product's safety story expressed as pure logic. The engine answers one
//! question and only that question: given a repo's observed git state and the
//! policy configured for it, what should happen - an intended git action, or a
//! skip with a typed reason. It performs **no I/O**: no git, no network, no DB,
//! no clock (AC1). That purity makes it exhaustively testable against the seven
//! E-04 fixture states in plain `cargo test`, and it keeps the decision that
//! governs whether RepoSync touches a working tree fully decoupled from
//! rendering, scheduling, and persistence.
//!
//! Two bodies of logic live here:
//!
//!   1. The **decision table** ([`decide`]): the 7 repo states (clean, dirty,
//!      ahead, behind, detached HEAD, deleted-upstream, no-upstream), with the
//!      diverged (ahead-and-behind) case resolved explicitly, crossed with the 3
//!      V1 update modes (`check_only`, `fetch_only`, `pull_ff_only`). Each cell
//!      resolves to a concrete [`Action`] or a typed [`SkipReason`]. The grid in
//!      the E-07 spec is the contract; the unit tests are that grid as
//!      assertions.
//!
//!   2. The **failure-handling state machine** ([`classify_failure`]): given the
//!      prior `consecutive_failures` count (read by E-08 from
//!      `repo_local_state.consecutive_failures`) and a run outcome, computes the
//!      new repo status - active, retry, paused-on-auth, or auto-paused when the
//!      count reaches 3 - resetting the counter on success. The engine READS the
//!      count and SIGNALS auto-pause; it does NOT persist (E-08 writes
//!      `auto_paused`/`consecutive_failures`).
//!
//! Reason codes cross-reference the stable [`crate::error::AppError`] codes from E-05 so the
//! contract is machine-readable: every [`SkipReason`] and failure outcome maps
//! to a code (AC7).

use crate::ipc::UpdateMode;

/// The three V1 update modes, the closed subset of [`UpdateMode`] that the V1
/// engine can actually execute. Any other [`UpdateMode`] variant is a non-V1
/// mode and is never silently run as one of these (AC6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum V1Mode {
    /// Report status only; never fetch, never mutate.
    CheckOnly,
    /// Update remote-tracking refs only; never touch the working tree.
    FetchOnly,
    /// Fast-forward `pull` (the one mutating action), only on a clean,
    /// fast-forwardable behind repo.
    PullFfOnly,
}

impl V1Mode {
    /// Narrow a frozen [`UpdateMode`] to its V1 counterpart, or `None` for a
    /// non-V1 mode (the closed-enum invariant, AC6). This is the ONLY place the
    /// V1/non-V1 boundary is decided, so a future mode going live is a single
    /// deliberate edit here, never a silent default.
    pub fn from_update_mode(mode: &UpdateMode) -> Option<V1Mode> {
        match mode {
            UpdateMode::CheckOnly => Some(V1Mode::CheckOnly),
            UpdateMode::FetchOnly => Some(V1Mode::FetchOnly),
            UpdateMode::PullFfOnly => Some(V1Mode::PullFfOnly),
            // Non-V1 modes: never executed as a V1 mode.
            UpdateMode::PullStandard | UpdateMode::PullRebase => None,
        }
    }
}

/// The upstream relationship of HEAD's branch, the distinction the decision
/// table needs that a bare ahead/behind cannot carry.
///
/// `None` ahead/behind (no comparison base) is produced by THREE different
/// states - detached, no-upstream, and deleted-upstream - that the grid treats
/// differently, so the caller classifies the upstream explicitly rather than
/// leaving the engine to guess from `None`/`None`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpstreamState {
    /// A tracking branch is configured and its remote-tracking ref resolves, so
    /// ahead/behind is a real comparison.
    Tracking,
    /// No tracking branch is configured at all (the no-upstream state).
    None,
    /// A tracking branch is configured but its remote-tracking ref no longer
    /// resolves - it was pruned/deleted (the deleted-upstream state).
    Deleted,
}

/// The observed git state the engine decides over. Built by the caller from the
/// E-03 inspect reads (`InspectResult` + `AheadBehind`); the engine itself reads
/// no git.
///
/// `ahead`/`behind` mirror E-03's contract (E-03 AC11): `None` is "no comparison
/// base", deliberately distinct from `Some(0)` ("level with upstream").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RepoState {
    /// Whether the working tree has uncommitted changes.
    pub is_dirty: bool,
    /// Whether HEAD is detached (not on a branch).
    pub is_detached: bool,
    /// The upstream relationship of HEAD's branch.
    pub upstream: UpstreamState,
    /// Commits on HEAD not in upstream, or `None` with no comparison base.
    pub ahead: Option<i64>,
    /// Commits in upstream not on HEAD, or `None` with no comparison base.
    pub behind: Option<i64>,
}

impl RepoState {
    /// Build a [`RepoState`] from the E-03 inspect reads.
    ///
    /// The caller passes the [`UpstreamState`] it determined (the no-upstream vs
    /// deleted-upstream distinction is not recoverable from ahead/behind alone),
    /// plus the dirtiness, detached flag, and ahead/behind counts straight from
    /// `InspectResult` / `AheadBehind`. Pure: no I/O.
    pub fn new(
        is_dirty: bool,
        is_detached: bool,
        upstream: UpstreamState,
        ahead: Option<i64>,
        behind: Option<i64>,
    ) -> RepoState {
        RepoState {
            is_dirty,
            is_detached,
            upstream,
            ahead,
            behind,
        }
    }
}

/// A concrete git action the engine intends. The caller (scheduler in E-08, or a
/// manual command) executes it through the git engine; the engine never executes
/// anything itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Report local status with no git operation at all (`check_only`, and the
    /// non-mutating `pull_ff_only` cells where there is nothing to do).
    ReportStatus,
    /// Fetch remote-tracking refs only; the working tree is never touched
    /// (`fetch_only`).
    Fetch,
    /// `git pull --ff-only` - the single mutating action, reached only on a
    /// clean, fast-forwardable behind repo under `pull_ff_only`.
    PullFastForward,
}

/// Why a mode produced no action for a given state. Each reason maps to a stable
/// [`crate::error::AppError`] code (AC7), so the UI and callers key off machine codes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The working tree is dirty and the mode would write it - never mutate a
    /// dirty tree (AC3). Code: `git.dirty_tree`.
    Dirty,
    /// HEAD is detached, so there is no branch to update (AC4). Code:
    /// `git.detached_head`.
    DetachedHead,
    /// The branch has no configured upstream (AC4). Code: `git.no_upstream`.
    NoUpstream,
    /// The configured upstream no longer resolves - it was deleted/pruned (AC4).
    /// Code: `git.deleted_upstream`.
    DeletedUpstream,
    /// The branch cannot fast-forward (diverged: ahead-and-behind) under the
    /// strictly fast-forward-only pull (AC4). Code: `git.ff_not_possible`.
    FfNotPossible,
    /// The requested mode is not one of the three V1 modes, so it is not
    /// available in V1 (AC6). Code: `config.invalid_policy`.
    ModeNotAvailableInV1,
}

impl SkipReason {
    /// The stable [`crate::error::AppError`] code this skip-reason corresponds to (AC7).
    ///
    /// Returns the code string directly (not an `AppError` value) because some
    /// skip-reasons are not failures - "dirty" or "no upstream" under a
    /// fast-forward mode is a normal, expected skip, not an error to surface -
    /// yet they still carry the stable code so a caller can render the reason
    /// consistently with the error taxonomy.
    pub fn code(self) -> &'static str {
        match self {
            SkipReason::Dirty => "git.dirty_tree",
            SkipReason::DetachedHead => "git.detached_head",
            SkipReason::NoUpstream => "git.no_upstream",
            SkipReason::DeletedUpstream => "git.deleted_upstream",
            SkipReason::FfNotPossible => "git.ff_not_possible",
            SkipReason::ModeNotAvailableInV1 => "config.invalid_policy",
        }
    }
}

/// The engine's verdict for one `(state, policy)` pair: either an intended
/// [`Action`] or a [`Skip`](PolicyDecision::Skip) with a typed reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PolicyDecision {
    /// Do this git action (the caller executes it).
    Act(Action),
    /// Take no action, for this typed reason.
    Skip(SkipReason),
}

impl PolicyDecision {
    /// The intended action, if this is an [`Act`](PolicyDecision::Act).
    pub fn action(self) -> Option<Action> {
        match self {
            PolicyDecision::Act(a) => Some(a),
            PolicyDecision::Skip(_) => None,
        }
    }

    /// The skip reason, if this is a [`Skip`](PolicyDecision::Skip).
    pub fn skip_reason(self) -> Option<SkipReason> {
        match self {
            PolicyDecision::Skip(r) => Some(r),
            PolicyDecision::Act(_) => None,
        }
    }
}

/// Decide what to do for a repo, given its observed state and the configured
/// update mode (AC1, AC2). Pure: no git, network, DB, or clock I/O.
///
/// This is the contract grid from the E-07 spec, expressed as an explicit match
/// over `(mode, state)`. A non-V1 mode short-circuits to
/// [`SkipReason::ModeNotAvailableInV1`] (AC6) before any state logic runs.
pub fn decide(state: &RepoState, mode: &UpdateMode) -> PolicyDecision {
    // AC6: the closed-enum invariant. Any mode that is not one of the three V1
    // modes is never executed as a V1 mode - it is rejected up front with a
    // typed reason, before any state-specific logic.
    let Some(v1) = V1Mode::from_update_mode(mode) else {
        return PolicyDecision::Skip(SkipReason::ModeNotAvailableInV1);
    };
    decide_v1(state, v1)
}

/// The decision table over the three V1 modes. Split out so [`decide`] owns the
/// non-V1 guard and this owns the grid.
///
/// Expressed as an explicit match over `(mode, state)` so every cell of the
/// spec's grid is visible; dirty handling and branch policy are named guards so
/// they read as the safety rules they are. No catch-all hides a missing case.
fn decide_v1(state: &RepoState, mode: V1Mode) -> PolicyDecision {
    use Action::*;
    use PolicyDecision::{Act, Skip};

    match mode {
        // check_only NEVER mutates or fetches for ANY state - it only reports
        // local status. The whole column is one cell.
        V1Mode::CheckOnly => Act(ReportStatus),

        // fetch_only updates remote-tracking refs only and NEVER touches the
        // working tree, so a dirty tree is irrelevant here (it still fetches).
        // It skips ONLY when the branch's upstream config names no fetchable
        // comparison base: no-upstream and deleted-upstream resolve to their
        // typed reasons (the grid's per-spec default: a fetch with no resolvable
        // upstream is a known no-op, so skip up front).
        //
        // A DETACHED HEAD is the exception the grid pins explicitly: it has no
        // branch upstream (so `upstream` is `None`), yet it still has remotes to
        // fetch, so the grid says "fetch". Detached therefore takes precedence
        // over the upstream match here. (Detached is distinct from a standalone
        // no-upstream repo, which has no remote at all.)
        V1Mode::FetchOnly => {
            if state.is_detached {
                Act(Fetch)
            } else {
                match state.upstream {
                    UpstreamState::None => Skip(SkipReason::NoUpstream),
                    UpstreamState::Deleted => Skip(SkipReason::DeletedUpstream),
                    UpstreamState::Tracking => Act(Fetch),
                }
            }
        }

        // pull_ff_only is the ONLY mode that can mutate, and only on a clean,
        // fast-forwardable behind repo. The guards run in safety order: dirty
        // first (never mutate a dirty tree, AC3), then branch policy (detached /
        // upstream, AC4), then the fast-forward decision.
        V1Mode::PullFfOnly => decide_pull_ff(state),
    }
}

/// The `pull_ff_only` column: the only mutating mode, gated by the safety rules
/// in priority order. Split out so each guard is named and the ordering is
/// explicit.
fn decide_pull_ff(state: &RepoState) -> PolicyDecision {
    use PolicyDecision::{Act, Skip};

    // AC3: a dirty working tree is NEVER mutated. This guard runs first so a
    // dirty + fast-forwardable repo still skips-dirty rather than pulling over
    // uncommitted work.
    if state.is_dirty {
        return Skip(SkipReason::Dirty);
    }

    // AC4: branch policy. Detached HEAD has no branch to update; a missing or
    // deleted upstream has no fast-forward source. Each is a distinct typed
    // skip-reason, not an error.
    if state.is_detached {
        return Skip(SkipReason::DetachedHead);
    }
    match state.upstream {
        UpstreamState::None => return Skip(SkipReason::NoUpstream),
        UpstreamState::Deleted => return Skip(SkipReason::DeletedUpstream),
        UpstreamState::Tracking => {}
    }

    // A real comparison base exists. The fast-forward decision keys off
    // ahead/behind:
    //   - behind == 0           -> nothing to fast-forward (level, or ahead-only):
    //                              report status, no action.
    //   - behind > 0, ahead 0   -> fast-forwardable: the one mutating action.
    //   - behind > 0, ahead > 0 -> diverged: a fast-forward is impossible.
    //   - behind unknown (None)  -> no comparison value despite a tracking
    //                              upstream; conservatively report (never mutate
    //                              on an unknown comparison).
    let ahead = state.ahead.unwrap_or(0);
    match state.behind {
        Some(behind) if behind > 0 => {
            if ahead > 0 {
                // Diverged (ahead-and-behind, e.g. the mockup's `up 2 down 5`):
                // local history diverged, so a fast-forward cannot apply (AC4).
                Skip(SkipReason::FfNotPossible)
            } else {
                // Clean, behind, no local divergence: fast-forward.
                Act(Action::PullFastForward)
            }
        }
        // behind == 0 (level or ahead-only) or unknown: nothing to fast-forward,
        // so no action - report the current status.
        _ => Act(Action::ReportStatus),
    }
}

/// A run's classified outcome, the input to the failure-handling state machine.
/// These mirror the [`crate::git::FetchClass`] split plus the pull-specific
/// fast-forward failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunOutcome {
    /// The run succeeded (fetch advanced or was a no-op; pull fast-forwarded).
    Success,
    /// A fast-forward was not possible (diverged history). Code:
    /// `git.ff_not_possible`.
    FfNotPossible,
    /// Authentication failed. Code: `git.auth_failed`.
    AuthFailure,
    /// A transport/connectivity failure (transient). Code: `net.offline`.
    NetworkFailure,
}

impl RunOutcome {
    /// The stable [`crate::error::AppError`] code this outcome corresponds to, or `None` for a
    /// success (AC7).
    pub fn code(self) -> Option<&'static str> {
        match self {
            RunOutcome::Success => None,
            RunOutcome::FfNotPossible => Some("git.ff_not_possible"),
            RunOutcome::AuthFailure => Some("git.auth_failed"),
            RunOutcome::NetworkFailure => Some("net.offline"),
        }
    }
}

/// The repo status the failure-handling state machine computes from a run
/// outcome and the prior consecutive-failure count (AC5).
///
/// The engine SIGNALS these transitions; E-08 persists them (incrementing or
/// resetting `repo_local_state.consecutive_failures`, and setting
/// `auto_paused = 1` on [`AutoPaused`](RepoStatus::AutoPaused)).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RepoStatus {
    /// The repo is healthy; the consecutive-failure counter is reset to 0.
    Active,
    /// A transient failure: retry later without pausing. Carries the new
    /// consecutive-failure count (the prior count + 1).
    Retry { consecutive_failures: i64 },
    /// An auth failure paused the repo immediately, independent of the count.
    PausedOnAuth,
    /// The consecutive-failure count reached the 3-strikes threshold; the engine
    /// signals auto-pause (E-08 sets `auto_paused = 1` and resets the counter
    /// per policy).
    AutoPaused,
}

/// The consecutive-failure count at which a repo auto-pauses (AC5: "3 consecutive
/// failures -> auto-pause").
pub const AUTO_PAUSE_THRESHOLD: i64 = 3;

/// Classify a run outcome into the new repo status, given the prior
/// consecutive-failure count (AC5). Pure: no I/O.
///
/// `prior_failures` is the value E-08 reads from
/// `repo_local_state.consecutive_failures` BEFORE this run. The rules:
///   - **success** -> [`RepoStatus::Active`], counter reset to 0;
///   - **auth failure** -> [`RepoStatus::PausedOnAuth`] immediately, regardless
///     of the count;
///   - **network / ff-not-possible failure** -> increment the counter; if it
///     reaches [`AUTO_PAUSE_THRESHOLD`] signal [`RepoStatus::AutoPaused`], else
///     [`RepoStatus::Retry`] with the new count.
pub fn classify_failure(prior_failures: i64, outcome: RunOutcome) -> RepoStatus {
    match outcome {
        // Success clears the streak: the repo is healthy, counter reset to 0.
        RunOutcome::Success => RepoStatus::Active,

        // Auth failures pause immediately and unconditionally - a bad credential
        // will not heal on its own, so there is no point retrying or counting
        // toward the network 3-strikes path (AC5: "auth -> pause").
        RunOutcome::AuthFailure => RepoStatus::PausedOnAuth,

        // Network and ff-not-possible failures increment the consecutive count.
        // At the 3-strikes threshold the engine SIGNALS auto-pause (E-08 sets
        // auto_paused = 1 and resets the counter); below it, retry.
        RunOutcome::NetworkFailure | RunOutcome::FfNotPossible => {
            let new_count = prior_failures.saturating_add(1);
            if new_count >= AUTO_PAUSE_THRESHOLD {
                RepoStatus::AutoPaused
            } else {
                RepoStatus::Retry {
                    consecutive_failures: new_count,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppError;

    // =========================================================================
    // RepoState builders for the seven fixture states (mirrors E-04's grid).
    // These are the engine's test inputs: the state values matching what the
    // E-04 harness fabricates (see git::fixtures::FixtureState). The integration
    // cross-check (tests/) drives the REAL inspect engine over the real fixtures
    // and feeds the result here; these unit builders pin the same facts.
    // =========================================================================

    /// clean: on a branch, clean tree, tracking, level (ahead 0 / behind 0).
    fn st_clean() -> RepoState {
        RepoState::new(false, false, UpstreamState::Tracking, Some(0), Some(0))
    }
    /// dirty: clean upstream relationship, but the working tree is dirty.
    fn st_dirty() -> RepoState {
        RepoState::new(true, false, UpstreamState::Tracking, Some(0), Some(0))
    }
    /// ahead: local commits not pushed; nothing to pull (behind 0).
    fn st_ahead() -> RepoState {
        RepoState::new(false, false, UpstreamState::Tracking, Some(2), Some(0))
    }
    /// behind (fast-forwardable): upstream has commits we lack, no local
    /// divergence (ahead 0, behind > 0).
    fn st_behind() -> RepoState {
        RepoState::new(false, false, UpstreamState::Tracking, Some(0), Some(1))
    }
    /// diverged (ahead-and-behind, the brief mockup's `up 2 down 5`): cannot
    /// fast-forward. Tested explicitly within the behind row.
    fn st_diverged() -> RepoState {
        RepoState::new(false, false, UpstreamState::Tracking, Some(2), Some(5))
    }
    /// detached HEAD: no branch, no comparison base.
    fn st_detached() -> RepoState {
        RepoState::new(false, true, UpstreamState::None, None, None)
    }
    /// deleted-upstream: config names an upstream that no longer resolves.
    fn st_deleted_upstream() -> RepoState {
        RepoState::new(false, false, UpstreamState::Deleted, None, None)
    }
    /// no-upstream: a local branch with no tracking branch configured.
    fn st_no_upstream() -> RepoState {
        RepoState::new(false, false, UpstreamState::None, None, None)
    }

    fn act(d: PolicyDecision) -> Option<Action> {
        d.action()
    }
    fn skip(d: PolicyDecision) -> Option<SkipReason> {
        d.skip_reason()
    }

    // =========================================================================
    // The decision table - one assertion per cell (AC2). The grid in the E-07
    // spec is reproduced verbatim as assertions: 7 states x 3 modes = 21 cells,
    // plus the diverged case crossed with all 3 modes = 3 more = 24 assertions
    // total, covering the explicit grid and its diverged row.
    // =========================================================================

    // --- check_only: never mutates or fetches for ANY state ------------------

    #[test]
    fn check_only_clean_reports_status() {
        assert_eq!(
            act(decide(&st_clean(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_dirty_reports_status() {
        assert_eq!(
            act(decide(&st_dirty(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_ahead_reports_status() {
        assert_eq!(
            act(decide(&st_ahead(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_behind_reports_status() {
        assert_eq!(
            act(decide(&st_behind(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_diverged_reports_status() {
        assert_eq!(
            act(decide(&st_diverged(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_detached_reports_status() {
        assert_eq!(
            act(decide(&st_detached(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_deleted_upstream_reports_status() {
        assert_eq!(
            act(decide(&st_deleted_upstream(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn check_only_no_upstream_reports_status() {
        assert_eq!(
            act(decide(&st_no_upstream(), &UpdateMode::CheckOnly)),
            Some(Action::ReportStatus)
        );
    }

    // --- fetch_only: fetch where an upstream resolves; typed upstream skip
    //     otherwise. NEVER touches the working tree, so dirty still fetches. ---

    #[test]
    fn fetch_only_clean_fetches() {
        assert_eq!(
            act(decide(&st_clean(), &UpdateMode::FetchOnly)),
            Some(Action::Fetch)
        );
    }
    #[test]
    fn fetch_only_dirty_fetches_refs_only() {
        // Dirty tree, but fetch only updates refs and never writes the tree, so
        // it is allowed (the grid: "fetch (refs only, tree untouched)").
        assert_eq!(
            act(decide(&st_dirty(), &UpdateMode::FetchOnly)),
            Some(Action::Fetch)
        );
    }
    #[test]
    fn fetch_only_ahead_fetches() {
        assert_eq!(
            act(decide(&st_ahead(), &UpdateMode::FetchOnly)),
            Some(Action::Fetch)
        );
    }
    #[test]
    fn fetch_only_behind_fetches() {
        assert_eq!(
            act(decide(&st_behind(), &UpdateMode::FetchOnly)),
            Some(Action::Fetch)
        );
    }
    #[test]
    fn fetch_only_diverged_fetches() {
        assert_eq!(
            act(decide(&st_diverged(), &UpdateMode::FetchOnly)),
            Some(Action::Fetch)
        );
    }
    #[test]
    fn fetch_only_detached_fetches() {
        // Detached HEAD still has remotes to fetch; the grid says fetch.
        assert_eq!(
            act(decide(&st_detached(), &UpdateMode::FetchOnly)),
            Some(Action::Fetch)
        );
    }
    #[test]
    fn fetch_only_deleted_upstream_skips() {
        assert_eq!(
            skip(decide(&st_deleted_upstream(), &UpdateMode::FetchOnly)),
            Some(SkipReason::DeletedUpstream)
        );
    }
    #[test]
    fn fetch_only_no_upstream_skips() {
        assert_eq!(
            skip(decide(&st_no_upstream(), &UpdateMode::FetchOnly)),
            Some(SkipReason::NoUpstream)
        );
    }

    // --- pull_ff_only: the only mutating mode. Mutates ONLY a clean,
    //     fast-forwardable behind repo. ---

    #[test]
    fn pull_ff_clean_reports_up_to_date() {
        // Clean and level: nothing to fast-forward, no action (report).
        assert_eq!(
            act(decide(&st_clean(), &UpdateMode::PullFfOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn pull_ff_dirty_skips_dirty() {
        assert_eq!(
            skip(decide(&st_dirty(), &UpdateMode::PullFfOnly)),
            Some(SkipReason::Dirty)
        );
    }
    #[test]
    fn pull_ff_ahead_reports_status() {
        // Ahead with nothing behind: no remote commits to fast-forward, no
        // action (the grid: "report ahead (no remote commits to ff)").
        assert_eq!(
            act(decide(&st_ahead(), &UpdateMode::PullFfOnly)),
            Some(Action::ReportStatus)
        );
    }
    #[test]
    fn pull_ff_behind_fast_forwards() {
        // The one mutating cell: clean + behind + fast-forwardable.
        assert_eq!(
            act(decide(&st_behind(), &UpdateMode::PullFfOnly)),
            Some(Action::PullFastForward)
        );
    }
    #[test]
    fn pull_ff_diverged_skips_ff_not_possible() {
        // Diverged (ahead-and-behind): a fast-forward is impossible.
        assert_eq!(
            skip(decide(&st_diverged(), &UpdateMode::PullFfOnly)),
            Some(SkipReason::FfNotPossible)
        );
    }
    #[test]
    fn pull_ff_detached_skips_detached() {
        assert_eq!(
            skip(decide(&st_detached(), &UpdateMode::PullFfOnly)),
            Some(SkipReason::DetachedHead)
        );
    }
    #[test]
    fn pull_ff_deleted_upstream_skips() {
        assert_eq!(
            skip(decide(&st_deleted_upstream(), &UpdateMode::PullFfOnly)),
            Some(SkipReason::DeletedUpstream)
        );
    }
    #[test]
    fn pull_ff_no_upstream_skips() {
        assert_eq!(
            skip(decide(&st_no_upstream(), &UpdateMode::PullFfOnly)),
            Some(SkipReason::NoUpstream)
        );
    }

    // --- dirty precedence on pull_ff (AC3): a dirty + behind repo must STILL
    //     skip-dirty, never fast-forward over an uncommitted tree. ---

    #[test]
    fn pull_ff_dirty_and_behind_still_skips_dirty() {
        let dirty_behind = RepoState::new(true, false, UpstreamState::Tracking, Some(0), Some(3));
        assert_eq!(
            skip(decide(&dirty_behind, &UpdateMode::PullFfOnly)),
            Some(SkipReason::Dirty),
            "a dirty tree is never mutated, even when fast-forwardable"
        );
    }

    // --- AC6: the closed-enum non-V1 guard. A non-V1 mode is NEVER executed as
    //     a V1 mode, for any state. ---

    #[test]
    fn non_v1_mode_pull_standard_is_rejected() {
        for st in [
            st_clean(),
            st_dirty(),
            st_behind(),
            st_detached(),
            st_no_upstream(),
        ] {
            assert_eq!(
                skip(decide(&st, &UpdateMode::PullStandard)),
                Some(SkipReason::ModeNotAvailableInV1),
                "pull_standard is not a V1 mode and must never run as one"
            );
        }
    }
    #[test]
    fn non_v1_mode_pull_rebase_is_rejected() {
        for st in [st_clean(), st_behind(), st_diverged()] {
            assert_eq!(
                skip(decide(&st, &UpdateMode::PullRebase)),
                Some(SkipReason::ModeNotAvailableInV1),
                "pull_rebase is not a V1 mode and must never run as one"
            );
        }
    }
    #[test]
    fn v1_mode_narrowing_is_exhaustive_and_closed() {
        // The three V1 modes narrow; the two non-V1 modes do not.
        assert_eq!(
            V1Mode::from_update_mode(&UpdateMode::CheckOnly),
            Some(V1Mode::CheckOnly)
        );
        assert_eq!(
            V1Mode::from_update_mode(&UpdateMode::FetchOnly),
            Some(V1Mode::FetchOnly)
        );
        assert_eq!(
            V1Mode::from_update_mode(&UpdateMode::PullFfOnly),
            Some(V1Mode::PullFfOnly)
        );
        assert_eq!(V1Mode::from_update_mode(&UpdateMode::PullStandard), None);
        assert_eq!(V1Mode::from_update_mode(&UpdateMode::PullRebase), None);
    }

    // --- AC7: every skip-reason carries the corresponding AppError code. ------

    #[test]
    fn skip_reasons_map_to_app_error_codes() {
        assert_eq!(SkipReason::Dirty.code(), "git.dirty_tree");
        assert_eq!(SkipReason::DetachedHead.code(), "git.detached_head");
        assert_eq!(SkipReason::NoUpstream.code(), "git.no_upstream");
        assert_eq!(SkipReason::DeletedUpstream.code(), "git.deleted_upstream");
        assert_eq!(SkipReason::FfNotPossible.code(), "git.ff_not_possible");
        assert_eq!(
            SkipReason::ModeNotAvailableInV1.code(),
            "config.invalid_policy"
        );

        // Each code must be a real code in the E-05 taxonomy (constructing the
        // matching AppError proves the string is the live one, not a typo).
        assert_eq!(
            AppError::DirtyTree { path: "p".into() }.code(),
            SkipReason::Dirty.code()
        );
        assert_eq!(
            AppError::DetachedHead.code(),
            SkipReason::DetachedHead.code()
        );
        assert_eq!(
            AppError::NoUpstream { branch: "b".into() }.code(),
            SkipReason::NoUpstream.code()
        );
        assert_eq!(
            AppError::DeletedUpstream {
                upstream: "u".into()
            }
            .code(),
            SkipReason::DeletedUpstream.code()
        );
        assert_eq!(
            AppError::FfNotPossible { branch: "b".into() }.code(),
            SkipReason::FfNotPossible.code()
        );
        assert_eq!(
            AppError::InvalidPolicy { detail: "d".into() }.code(),
            SkipReason::ModeNotAvailableInV1.code()
        );
    }

    // =========================================================================
    // The failure-handling state machine (AC5).
    // =========================================================================

    #[test]
    fn success_resets_counter_and_stays_active() {
        // A success after any number of prior failures resets the counter.
        assert_eq!(classify_failure(0, RunOutcome::Success), RepoStatus::Active);
        assert_eq!(classify_failure(2, RunOutcome::Success), RepoStatus::Active);
        assert_eq!(classify_failure(5, RunOutcome::Success), RepoStatus::Active);
    }

    #[test]
    fn auth_failure_pauses_immediately_regardless_of_count() {
        // Auth pauses on the FIRST occurrence and at any prior count.
        assert_eq!(
            classify_failure(0, RunOutcome::AuthFailure),
            RepoStatus::PausedOnAuth
        );
        assert_eq!(
            classify_failure(2, RunOutcome::AuthFailure),
            RepoStatus::PausedOnAuth
        );
    }

    #[test]
    fn first_network_failure_retries_without_pause() {
        assert_eq!(
            classify_failure(0, RunOutcome::NetworkFailure),
            RepoStatus::Retry {
                consecutive_failures: 1
            }
        );
    }

    #[test]
    fn second_consecutive_failure_still_retries() {
        assert_eq!(
            classify_failure(1, RunOutcome::NetworkFailure),
            RepoStatus::Retry {
                consecutive_failures: 2
            }
        );
    }

    #[test]
    fn third_consecutive_failure_auto_pauses() {
        // The 3-strikes threshold: prior count 2, this failure makes 3.
        assert_eq!(
            classify_failure(2, RunOutcome::NetworkFailure),
            RepoStatus::AutoPaused
        );
    }

    #[test]
    fn ff_not_possible_counts_toward_auto_pause() {
        // ff-not-possible is a failure outcome that increments the counter.
        assert_eq!(
            classify_failure(0, RunOutcome::FfNotPossible),
            RepoStatus::Retry {
                consecutive_failures: 1
            }
        );
        assert_eq!(
            classify_failure(2, RunOutcome::FfNotPossible),
            RepoStatus::AutoPaused
        );
    }

    #[test]
    fn success_after_two_failures_resets_then_failure_restarts_count() {
        // Two failures -> retry at 2.
        assert_eq!(
            classify_failure(1, RunOutcome::NetworkFailure),
            RepoStatus::Retry {
                consecutive_failures: 2
            }
        );
        // A success resets to active (counter 0).
        assert_eq!(classify_failure(2, RunOutcome::Success), RepoStatus::Active);
        // A later failure starts the count again from 0 -> retry at 1, NOT
        // straight to auto-pause.
        assert_eq!(
            classify_failure(0, RunOutcome::NetworkFailure),
            RepoStatus::Retry {
                consecutive_failures: 1
            }
        );
    }

    #[test]
    fn run_outcomes_map_to_app_error_codes() {
        assert_eq!(RunOutcome::Success.code(), None);
        assert_eq!(
            RunOutcome::FfNotPossible.code(),
            Some("git.ff_not_possible")
        );
        assert_eq!(RunOutcome::AuthFailure.code(), Some("git.auth_failed"));
        assert_eq!(RunOutcome::NetworkFailure.code(), Some("net.offline"));

        // The codes are live E-05 taxonomy codes.
        assert_eq!(
            AppError::FfNotPossible { branch: "b".into() }.code(),
            RunOutcome::FfNotPossible.code().unwrap()
        );
        assert_eq!(
            AppError::AuthFailed.code(),
            RunOutcome::AuthFailure.code().unwrap()
        );
        assert_eq!(
            AppError::Offline.code(),
            RunOutcome::NetworkFailure.code().unwrap()
        );
    }
}
