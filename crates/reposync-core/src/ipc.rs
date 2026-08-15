//! ipc - owned by E-06 (IPC payload structs shared with the Tauri shell).
//!
//! The full IPC payload surface: every command parameter/return type and every
//! event payload that crosses the Tauri boundary. These derive serde +
//! specta::Type ONLY - this module (and the whole crate) must never import tauri
//! or tauri-*. The `tauri_specta::Event` wrappers and `#[tauri::command]`
//! adapters live in `src-tauri`.
//!
//! Field types trace to the v1 SQLite schema (strategy-and-roadmap.md Section
//! 4.2): SQLite INTEGER -> i64, nullable columns -> Option, bool-as-INTEGER ->
//! bool. The structs serialize camelCase; the string-valued enums serialize
//! snake_case to match the values stored in the DB columns.

use serde::{Deserialize, Serialize};

use crate::error::AppErrorPayload;

/// Stable identifier for a tracked repo (its `repos.id`).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
pub struct RepoId(pub i64);

/// Result of a "check now" run, returned to the caller.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckResult {
    pub repo_id: i64,
    pub decision: String,
    pub reason: Option<String>,
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
    pub is_dirty: bool,
    pub is_detached: bool,
    pub checked_at: i64,
    /// Whether the check FAILED operationally, as opposed to completing with a
    /// policy skip (BL-NI-04).
    ///
    /// Additive, and it exists so no consumer has to re-derive the distinction.
    /// A failed fetch and a deliberate skip both arrive as
    /// `decision == "skip-with-reason"`, and they must not read the same to a
    /// user: "skipped, working tree is dirty" is the safety rule doing its job,
    /// while "could not reach the remote" is a problem. Telling them apart from
    /// `reason` alone means matching the operational reason codes as strings,
    /// which is a classification the backend already performs and which a second
    /// copy in TypeScript could only drift from.
    ///
    /// `true` for an auth, network, or otherwise non-zero fetch. `false` for
    /// every policy decision, including every skip.
    pub failed: bool,
}

/// Event payload emitted when a check completes (the slimmer broadcast form).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckCompletedPayload {
    pub repo_id: i64,
    pub decision: String,
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
    pub checked_at: i64,
    /// The typed reason, when there is one, so a window that did not initiate the
    /// check learns WHY it ended that way rather than only that it ended.
    pub reason: Option<String>,
    /// Whether the check failed operationally. See [`CheckResult::failed`].
    ///
    /// Before BL-NI-04 this payload could not carry either field for the simple
    /// reason that it was never emitted for a failure at all: the command
    /// short-circuited on the error first.
    pub failed: bool,
}

// =============================================================================
// Query / list payloads
// =============================================================================

/// One row of the `activity_records` audit trail. Maps verbatim to the schema
/// (4.2): every git/check/update operation is recorded with its raw output.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivityRecord {
    pub id: i64,
    pub repo_id: i64,
    pub timestamp: i64,
    pub action_type: String,
    pub status: String,
    pub reason_code: Option<String>,
    pub summary: Option<String>,
    pub commit_range: Option<String>,
    pub raw_command: Option<String>,
    pub raw_stdout: Option<String>,
    pub raw_stderr: Option<String>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<i64>,
}

/// A read-only snapshot of where RepoSync keeps its files and how it is
/// currently configured, for the Settings -> Diagnostics card.
///
/// Every field is DERIVED, not stored: this is a view over the resolved paths,
/// the logging configuration actually installed at startup, the live git probe,
/// and the since-launch scheduler counters. Nothing here is settable, which is
/// why it has no write counterpart.
///
/// Paths are rendered strings rather than structured values because their only
/// consumer is a label the user reads or copies into a bug report. Note that on
/// Windows they contain the account name; the card says so next to the copy
/// action rather than mangling the paths, since a redacted path is useless for
/// the thing a user opens this card to do - find the folder.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    /// The running app version (the crate version baked in at build time).
    pub app_version: String,
    /// The resolved app-data directory.
    pub data_dir: String,
    /// The SQLite database file inside it.
    pub db_path: String,
    /// The rolling-log directory.
    pub log_dir: String,
    /// Whether the logging subscriber was installed successfully AT STARTUP.
    ///
    /// Deliberately named for what it proves. `tracing_appender` writes on a
    /// background worker thread and exposes no error channel, so a disk that
    /// fills or an ACL that changes AFTER startup produces no signal here: this
    /// stays `true` while nothing reaches disk. Pair it with `log_dir_readable`
    /// and `log_file_count`, which are measured live - "started, but the
    /// directory has no files in it" is the observable contradiction, and the
    /// UI flags exactly that. Durable write-failure telemetry is BL-NI-63.
    pub logging_active: bool,
    /// The effective maximum level, e.g. `"INFO"`. `None` when logging is off.
    pub log_level: Option<String>,
    /// Daily files retained (the AGE half of retention). `None` when logging is off.
    pub log_max_files: Option<i64>,
    /// The directory size budget in bytes (the SIZE half). `None` when off.
    pub log_max_bytes: Option<i64>,
    /// Whether the log directory could be read at all. `false` is a distinct,
    /// worse state than "readable and empty": a directory the app cannot read
    /// is one it most likely cannot write to either. The counts below mean
    /// nothing when this is `false`.
    pub log_dir_readable: bool,
    /// Log files present right now (measured, not remembered).
    pub log_file_count: i64,
    /// Bytes those files occupy right now.
    pub log_bytes: i64,
    /// Write or flush errors the log writer has hit since launch (BL-NI-63).
    ///
    /// The counterpart `logging_active` cannot provide. That flag reports that the
    /// subscriber INSTALLED, a fact about one moment during startup; this reports
    /// what the writer has done since. Non-zero means events have been lost, and
    /// the log is by definition not the place to find out about it.
    pub log_write_failures: i64,
    /// Unix seconds of the most recent write failure, or `None` for none.
    /// Distinguishes "broken since launch" from "broke ten minutes ago".
    pub log_last_write_failure_at: Option<i64>,
    /// Bytes the writer has successfully written since launch.
    ///
    /// The POSITIVE evidence, and the reason this is not just a failure counter.
    /// Zero failures is equally consistent with "working" and "nothing was ever
    /// written", and only the second is a problem. A non-zero value here is proof
    /// the whole path (subscriber, worker thread, file) carried something, which
    /// is the claim `logging_active` looks like it is making and is not.
    pub log_bytes_written: i64,
    /// Log lines the non-blocking queue DISCARDED because its buffer was full.
    ///
    /// A third state, distinct from both counters above and reported separately
    /// because it means something different to whoever reads it. A write failure
    /// is the disk refusing; a dropped line is RepoSync choosing to lose the line
    /// rather than block the work that produced it. Only the first suggests the
    /// machine is broken. The queue is lossy on purpose: the alternative exerts
    /// backpressure on the emitting thread, which here would mean stalling a git
    /// operation so a log line could be written.
    pub log_dropped_lines: i64,
    /// Whether the data directory sits under a OneDrive-synced tree, where a sync
    /// agent can corrupt the SQLite WAL sidecars mid-write (BL-NI-12).
    pub onedrive_rooted: bool,
    /// The resolved `git` executable, or `None` when none was found.
    pub git_path: Option<String>,
    /// The probed git version, or `None` when git is unavailable.
    pub git_version: Option<String>,
    /// Whether a git executable was resolved at all. `false` is the only state
    /// in which RepoSync will not run git.
    pub git_resolved: bool,
    /// The git path the user CONFIGURED in Settings, or `None` if they set none.
    ///
    /// Reported alongside the RESOLVED `git_path` so the two can be compared,
    /// which nothing did before BL-NI-39: Settings showed the configured value,
    /// Diagnostics showed the resolved one, and a wrong configured path fell back
    /// silently to a working git that was not the one named on screen.
    pub git_explicit_path: Option<String>,
    /// Whether the configured path is the one actually running, or `None` when
    /// the question does not apply or cannot be answered.
    ///
    /// `Option` rather than `bool` deliberately. A bare `bool` had `true` meaning
    /// four different things - honored, nothing configured, settings unreadable,
    /// and configured-but-no-git-at-all - which is the same overclaiming this
    /// field was added to fix, one level up. `None` says "no comparison was made"
    /// and leaves the reason to the fields that already carry it
    /// (`git_explicit_path` for whether anything is set, `git_resolved` for
    /// whether a git was found at all).
    ///
    /// `Some(false)` is the condition worth surfacing and means exactly one
    /// thing: the user configured a git, RepoSync could not use it, and it is
    /// running a different one.
    pub git_explicit_path_honored: Option<bool>,
    /// Whether the resolved git is at or above the supported >= 2.30 floor.
    ///
    /// Split from `git_resolved` rather than folded into one "available" flag,
    /// because [`crate::git::GitAvailability`] has THREE states and a single
    /// boolean can only carry two. A below-floor git is explicitly "usable but
    /// flagged - operations are still attempted" (E-03 AC7), so collapsing it
    /// into "not available" would tell a user reading this panel that RepoSync
    /// has stopped running git when it has not.
    pub git_meets_floor: bool,
    /// Scheduler cycles completed since launch.
    pub scheduler_cycles: i64,
    /// Repos run across those cycles.
    pub scheduler_repos_checked: i64,
    /// Jobs that ran but could not persist their outcome (BL-NI-14). Non-zero
    /// means some checks silently retried; the log carries the reason under
    /// `scheduler.outcome_persist_failed`.
    pub scheduler_outcome_persist_failures: i64,
    /// Whether the startup migration failed and the previous database was moved
    /// aside (E-02 AC7).
    pub db_recovered: bool,
}

/// The singleton `settings` row. `github_token_present` is a derived boolean -
/// the token itself lives in the OS keychain, never on the wire.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub global_check_minutes: i64,
    pub quiet_hours_start: Option<i64>,
    pub quiet_hours_end: Option<i64>,
    pub notify_on_release: bool,
    pub notify_on_failure: bool,
    pub git_executable_path: Option<String>,
    pub editor_command: Option<String>,
    pub terminal_command: Option<String>,
    pub autostart: bool,
    pub activity_retention_d: i64,
    pub github_token_present: bool,
    /// Whether RepoSync checks for an app update on launch (E-18). Default `true`.
    /// This gates ONLY the on-launch check; the manual "Check for updates" action
    /// runs regardless, and no update ever installs without the user confirming.
    /// Provisional-additive per E-06's additive-revision rule; mirrors the
    /// `settings.auto_update_check` column added in migration `0006_auto_update.sql`.
    pub auto_update_check: bool,
    /// Whether the window's close (X) button HIDES the app to the tray (`true`,
    /// the default and prior hardcoded behavior) or QUITS it (`false`). Read live
    /// by the close handler via a mirrored `AtomicBool` in the shell's AppState.
    /// Mirrors the `settings.close_minimizes_to_tray` column added in migration
    /// `0007_close_minimizes_to_tray.sql`.
    pub close_minimizes_to_tray: bool,
}

/// The at-a-glance form of a tracked repo (list view). A flattened join of
/// `repos` + `repo_local_state` + the latest release tag.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoSummary {
    pub id: i64,
    pub local_name: String,
    pub host_type: String,
    pub ahead_count: Option<i64>,
    pub behind_count: Option<i64>,
    pub is_dirty: bool,
    pub is_detached: bool,
    pub enabled: bool,
    pub auto_paused: bool,
    pub last_checked_at: Option<i64>,
    pub last_error_code: Option<String>,
    pub latest_release_tag: Option<String>,
    /// Open pull-request count (E-17). `None` = un-refreshed or non-GitHub repo (a
    /// clean unknown, never a fabricated zero). Rides the single `repo_list` join.
    pub open_pr_count: Option<i64>,
    /// The HEAD commit's committer time (E-17), distinct from `last_checked_at`
    /// ("when RepoSync last looked"). `None` when the inspect never read it.
    pub last_local_commit_at: Option<i64>,
    /// The upstream relationship as the policy engine last classified it
    /// (BL-NI-77), carried so the UI can tell a repo that is genuinely in sync
    /// from one whose upstream was deleted and therefore cannot sync at all.
    ///
    /// `None` means NOT YET OBSERVED: the row predates migration `0008` and has
    /// not been checked since. It is a fourth fact, distinct from all three
    /// states, and consumers must render it as unknown rather than assuming
    /// `Tracking` - assuming the reassuring value is the bug this field exists to
    /// fix. It resolves itself the first time the repo is checked.
    pub upstream_state: Option<crate::policy::UpstreamState>,
}

/// The full detail of a tracked repo (detail view). Repeats every
/// [`RepoSummary`] field verbatim (NOT `serde(flatten)` - it is fragile with
/// specta rc.25) and adds the rest of `repos` + `repo_local_state` +
/// `repo_remote_meta`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoDetail {
    // --- RepoSummary fields (repeated, not flattened) ---
    pub id: i64,
    pub local_name: String,
    pub host_type: String,
    pub ahead_count: Option<i64>,
    pub behind_count: Option<i64>,
    pub is_dirty: bool,
    pub is_detached: bool,
    pub enabled: bool,
    pub auto_paused: bool,
    pub last_checked_at: Option<i64>,
    pub last_error_code: Option<String>,
    pub latest_release_tag: Option<String>,
    // --- repos ---
    pub local_path: String,
    pub remote_origin_url: Option<String>,
    pub default_branch: Option<String>,
    pub update_mode: String,
    pub check_frequency_min: i64,
    pub created_at: i64,
    pub notes: Option<String>,
    // --- repo_local_state ---
    pub active_branch: Option<String>,
    pub head_sha: Option<String>,
    pub upstream_branch: Option<String>,
    /// See [`RepoSummary::upstream_state`]. Repeated here because the detail
    /// drawer derives its own status badge from this type, so omitting it would
    /// leave the drawer saying "In sync" about a repo the list already knows
    /// cannot sync.
    pub upstream_state: Option<crate::policy::UpstreamState>,
    pub last_local_commit_at: Option<i64>,
    pub last_updated_at: Option<i64>,
    pub last_attempted_at: Option<i64>,
    pub next_check_at: Option<i64>,
    pub consecutive_failures: i64,
    // --- repo_remote_meta ---
    pub description: Option<String>,
    pub topics_json: Option<String>,
    pub latest_release_at: Option<i64>,
    pub latest_release_url: Option<String>,
    pub is_archived: bool,
    pub last_remote_sha: Option<String>,
    pub last_fetched_at: Option<i64>,
    // --- repo_remote_meta: branch and PR intelligence (E-17) ---
    /// Open pull-request count. `None` = un-refreshed / non-GitHub / unknown (a
    /// 404-or-403 on a private repo preserves the cache; it is NEVER a fabricated 0).
    pub open_pr_count: Option<i64>,
    /// Open pull requests targeting the default branch (a subset of `open_pr_count`).
    pub default_branch_pr_count: Option<i64>,
    /// When the PR counts were last confirmed against GitHub, for the drawer's
    /// "as of <time>" staleness marker when offline / rate-limited (E-17 AC8).
    pub pr_last_checked_at: Option<i64>,
}

// =============================================================================
// Scan payloads
// =============================================================================

/// A candidate repository found while scanning a parent folder.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanCandidate {
    pub local_path: String,
    pub local_name: String,
    pub already_tracked: bool,
    pub remote_origin_url: Option<String>,
}

/// The result of scanning a parent folder for git repositories.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub parent_path: String,
    pub discovered: Vec<ScanCandidate>,
}

// =============================================================================
// Update payloads
// =============================================================================

/// The result of an app self-update availability check (E-18 auto-update).
///
/// Returned by the `app_check_for_update` command (a thin wrapper over the Tauri
/// updater plugin, so the on-launch check and the Settings button share one typed
/// path). The three UI states are distinguished WITHOUT throwing: an update is
/// available (`available == true`, `new_version`/`notes` set), the app is up to
/// date (`available == false`, `error == None`), or the update server could not be
/// reached (`available == false`, `error == Some`) - the last case also covers the
/// inert private-repo endpoint (a 404 while the repo is private) and the ship-dark
/// state (no production signing key configured yet), both rendered as "could not
/// reach the update server." `current_version` is always the running app version,
/// so the Settings "Updates" section can show it. Tauri-free: `error` reuses the
/// frozen [`AppErrorPayload`] wire shape, never a `tauri` type.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAvailability {
    pub current_version: String,
    pub available: bool,
    pub new_version: Option<String>,
    pub notes: Option<String>,
    pub error: Option<AppErrorPayload>,
}

/// The outcome of an "update now" run for a single repo.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResult {
    pub repo_id: i64,
    pub mode: String,
    pub outcome: String,
    pub commit_range: Option<String>,
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
    pub updated_at: i64,
}

// =============================================================================
// Summary payloads
// =============================================================================

/// One repo's line in a summary bucket (updated / new release / attention).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SummaryItem {
    pub repo_id: i64,
    pub local_name: String,
    pub detail: Option<String>,
}

/// A daily roll-up of repo activity.
///
/// E-11 (summaries) owns field authority for this type; any change here is
/// additive (new fields/buckets), never a rename or removal, so the binding and
/// downstream consumers stay stable.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DailySummary {
    pub date: String,
    pub updated_count: i64,
    pub releases_count: i64,
    pub attention_count: i64,
    pub no_change_count: i64,
    pub updated: Vec<SummaryItem>,
    pub new_releases: Vec<SummaryItem>,
    pub attention: Vec<SummaryItem>,
}

/// A weekly roll-up: a window of [`DailySummary`] days. V1.1 stub (E-11 / V1.1).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct WeeklySummary {
    pub week_start: String,
    pub days: Vec<DailySummary>,
}

// =============================================================================
// Group / tag payloads
// =============================================================================

/// A repo group (tag) with its current member count. A flattened read of
/// `groups` + a COUNT of `repo_groups` memberships, for the group-management view.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GroupSummary {
    pub id: i64,
    pub name: String,
    pub color: Option<String>,
    pub repo_count: i64,
}

/// One repo's group memberships: the repo id and the ascending, de-duplicated ids
/// of the groups it belongs to. The bulk read (`repo_group_memberships`) returns
/// one of these per repo that has at least one membership, so the Repos screen can
/// build its `repoId -> groupId[]` map in a SINGLE IPC round-trip instead of
/// fanning `groups_for_repo` out per visible repo (BL-NI-22). A repo with no
/// memberships is simply absent from the list.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoGroupMembership {
    pub repo_id: i64,
    pub group_ids: Vec<i64>,
}

// =============================================================================
// Startup-state payloads
// =============================================================================

/// The one-time database-recovery notice (E-02 AC7 / BL-NI-33).
///
/// When the startup migration failed and the old database had to be moved aside,
/// `recovered` is true and `backup_path` is where the previous database was
/// preserved (a display string). The frontend reads this once at launch (the
/// `db_recovery_notice` command) to surface a dismissible banner. On a normal
/// launch `recovered` is false and `backup_path` is `None`. Before this type
/// existed the parked `db_recovered` / `db_backup_path` fields had no reader, so
/// the AC7 notice could never reach the UI.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DbRecoveryNotice {
    pub recovered: bool,
    pub backup_path: Option<String>,
}

// =============================================================================
// Filter payloads (command parameters)
// =============================================================================

/// Filter for `repo_list`. All fields optional; absent means "no constraint".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RepoFilter {
    pub enabled_only: Option<bool>,
    pub host_type: Option<String>,
    pub query: Option<String>,
}

/// Filter for `activity_list`. All fields optional; absent means "no constraint".
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ActivityFilter {
    pub repo_id: Option<i64>,
    pub action_type: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

// =============================================================================
// Policy enums
// =============================================================================

/// How a repo is updated. snake_case on the wire to match the `update_mode`
/// column values.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum UpdateMode {
    CheckOnly,
    FetchOnly,
    PullFfOnly,
    PullStandard,
    PullRebase,
}

/// What to do when the working tree is dirty at update time.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DirtyHandling {
    Skip,
    WarnAndBlock,
    AutoStash,
    FetchOnlyWhenDirty,
}

/// Which branches a repo is allowed to update.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum BranchPolicy {
    DefaultBranchOnly,
    TrackedUpstreamOnly,
    ApprovedBranches,
    AnyBranch,
}

/// The full per-repo update policy (E-07).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePolicy {
    pub mode: UpdateMode,
    pub dirty_handling: DirtyHandling,
    pub branch_policy: BranchPolicy,
}

// =============================================================================
// Event payloads
// =============================================================================

/// Payload for the `repo:state-changed` event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StateChangedPayload {
    pub repo_id: i64,
    pub last_error_code: Option<String>,
}

/// Payload for the `repo:check-started` event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CheckStartedPayload {
    pub repo_id: i64,
}

/// Payload for the `repo:update-started` event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStartedPayload {
    pub repo_id: i64,
    pub mode: String,
}

/// Payload for the `repo:update-completed` event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateCompletedPayload {
    pub repo_id: i64,
    pub outcome: String,
}

/// Payload for the `scheduler:tick` event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SchedulerTickPayload {
    pub checked: i64,
    pub due: i64,
    pub at: i64,
}

/// Payload for the `notification:fired` event.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NotificationFiredPayload {
    pub kind: String,
    pub repo_id: Option<i64>,
    pub title: String,
    pub body: String,
}

/// Payload for the `repo:metadata-refreshed` event (E-17 finding 3): the background
/// GitHub metadata + branch/PR refresh pass wrote fresh data for one or more repos.
///
/// The shell emits this ONCE per pass (only when at least one repo actually changed),
/// so the aggregate list view (dashboard, repos) refetches EXACTLY ONCE per pass -
/// never an N+1 refetch storm (the Phase-3 F3 batching discipline). It is deliberately
/// distinct from `scheduler:tick`: a metadata refresh is not a git check, so reusing
/// the tick would falsely imply a check ran. `changed_count` is how many repos moved
/// this pass; `at` is the pass's unix-second timestamp. Per-repo `repo:state-changed`
/// events (one per changed repo) drive the focused drawer separately.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MetadataRefreshedPayload {
    pub changed_count: i64,
    pub at: i64,
}

/// Payload for the `navigate:requested` event (E-13 tray): the shell asks the
/// frontend to switch to a named view. `target` is a view id the app-shell router understands
/// (`"dashboard"` / `"repos"` / `"activity"` / `"settings"`); an unknown target is
/// ignored by the frontend. Used by the tray "Settings" item to open + focus the
/// window on the settings view.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NavigateRequestedPayload {
    pub target: String,
}

// NOTE: the `error:raised` event payload is intentionally NOT defined here as a
// separate `ErrorRaisedPayload { error: AppErrorPayload }` struct. The owning
// `ErrorRaised` event in `src-tauri` carries `error: AppErrorPayload` as a NAMED
// single-field struct directly (wire shape `{ "error": { ...AppErrorPayload } }`)
// to dodge a tauri-specta rc.25 transform-codegen defect: a tuple-newtype event
// whose payload transitively carries the semantically-remapped `serde_json::Value`
// (`AppErrorPayload.context`) emits a runtime transform that indexes the payload
// as `v[0]`, which the collapsed TS type cannot index. A named-field event makes
// the transform descend by field name and typecheck. The error wire shape itself
// stays the frozen `AppErrorPayload` from `crate::error`.

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip one instance of each representative payload through
    /// serde_json and assert the deserialized form re-serializes identically.
    /// This guards the wire shape (camelCase keys, snake_case enum variants,
    /// nested Vecs, the AppError error half) against accidental drift.
    fn assert_round_trip<T>(value: &T)
    where
        T: Serialize + for<'de> Deserialize<'de>,
    {
        let json = serde_json::to_string(value).expect("serialize");
        let back: T = serde_json::from_str(&json).expect("deserialize");
        let json2 = serde_json::to_string(&back).expect("re-serialize");
        assert_eq!(json, json2, "round-trip must be lossless");
    }

    #[test]
    fn payloads_round_trip_losslessly() {
        let summary = RepoSummary {
            id: 1,
            local_name: "repo".into(),
            host_type: "github".into(),
            ahead_count: Some(2),
            behind_count: None,
            is_dirty: false,
            is_detached: false,
            enabled: true,
            auto_paused: false,
            last_checked_at: Some(1_700_000_000),
            last_error_code: None,
            latest_release_tag: Some("v1.0.0".into()),
            open_pr_count: Some(3),
            last_local_commit_at: Some(1_699_400_000),
            // Deliberately the Deleted variant rather than Tracking: the round-trip
            // has to prove a non-default variant survives, and Deleted is the one
            // the UI branches on (BL-NI-77).
            upstream_state: Some(crate::policy::UpstreamState::Deleted),
        };
        assert_round_trip(&summary);

        let detail = RepoDetail {
            id: 1,
            local_name: "repo".into(),
            host_type: "github".into(),
            ahead_count: Some(2),
            behind_count: Some(0),
            is_dirty: false,
            is_detached: false,
            enabled: true,
            auto_paused: false,
            last_checked_at: Some(1_700_000_000),
            last_error_code: None,
            latest_release_tag: Some("v1.0.0".into()),
            local_path: "C:/repos/repo".into(),
            remote_origin_url: Some("https://github.com/o/repo".into()),
            default_branch: Some("main".into()),
            update_mode: "fetch_only".into(),
            check_frequency_min: 360,
            created_at: 1_699_000_000,
            notes: Some("a note".into()),
            active_branch: Some("main".into()),
            head_sha: Some("abc123".into()),
            upstream_branch: Some("origin/main".into()),
            upstream_state: Some(crate::policy::UpstreamState::Tracking),
            last_local_commit_at: Some(1_699_500_000),
            last_updated_at: Some(1_700_000_000),
            last_attempted_at: Some(1_700_000_001),
            next_check_at: Some(1_700_021_600),
            consecutive_failures: 0,
            description: Some("desc".into()),
            topics_json: Some("[\"rust\"]".into()),
            latest_release_at: Some(1_698_000_000),
            latest_release_url: Some("https://github.com/o/repo/releases/v1.0.0".into()),
            is_archived: false,
            last_remote_sha: Some("def456".into()),
            last_fetched_at: Some(1_700_000_000),
            open_pr_count: Some(3),
            default_branch_pr_count: Some(1),
            pr_last_checked_at: Some(1_700_000_500),
        };
        assert_round_trip(&detail);

        let settings = Settings {
            global_check_minutes: 360,
            quiet_hours_start: Some(1320),
            quiet_hours_end: Some(420),
            notify_on_release: true,
            notify_on_failure: true,
            git_executable_path: None,
            editor_command: Some("code".into()),
            terminal_command: Some("wt".into()),
            autostart: false,
            activity_retention_d: 90,
            github_token_present: false,
            auto_update_check: true,
            close_minimizes_to_tray: true,
        };
        assert_round_trip(&settings);

        let activity = ActivityRecord {
            id: 1,
            repo_id: 1,
            timestamp: 1_700_000_000,
            action_type: "check".into(),
            status: "success".into(),
            reason_code: None,
            summary: Some("up to date".into()),
            commit_range: None,
            raw_command: Some("git fetch".into()),
            raw_stdout: Some("".into()),
            raw_stderr: Some("".into()),
            exit_code: Some(0),
            duration_ms: Some(123),
        };
        assert_round_trip(&activity);

        let daily = DailySummary {
            date: "2026-06-20".into(),
            updated_count: 1,
            releases_count: 1,
            attention_count: 0,
            no_change_count: 5,
            updated: vec![SummaryItem {
                repo_id: 1,
                local_name: "repo".into(),
                detail: Some("3 commits".into()),
            }],
            new_releases: vec![SummaryItem {
                repo_id: 1,
                local_name: "repo".into(),
                detail: Some("v1.0.0".into()),
            }],
            attention: vec![],
        };
        assert_round_trip(&daily);

        let policy = UpdatePolicy {
            mode: UpdateMode::PullFfOnly,
            dirty_handling: DirtyHandling::Skip,
            branch_policy: BranchPolicy::DefaultBranchOnly,
        };
        assert_round_trip(&policy);

        let group = GroupSummary {
            id: 1,
            name: "backend".into(),
            color: Some("#3b82f6".into()),
            repo_count: 3,
        };
        assert_round_trip(&group);

        let membership = RepoGroupMembership {
            repo_id: 7,
            group_ids: vec![1, 2, 5],
        };
        assert_round_trip(&membership);

        // The db-recovery notice, in both its normal (no recovery) and recovered
        // shapes, so the additive BL-NI-33 payload's wire form is guarded too.
        assert_round_trip(&DbRecoveryNotice {
            recovered: false,
            backup_path: None,
        });
        assert_round_trip(&DbRecoveryNotice {
            recovered: true,
            backup_path: Some("C:/data/reposync.db.corrupt-1700000000".into()),
        });

        // The app-update availability payload (E-18), in all three UI shapes: an
        // update is available, the app is up to date, and the update server could
        // not be reached (the error half carries the frozen AppErrorPayload).
        assert_round_trip(&UpdateAvailability {
            current_version: "0.9.0".into(),
            available: true,
            new_version: Some("0.9.1".into()),
            notes: Some("Bug fixes.".into()),
            error: None,
        });
        assert_round_trip(&UpdateAvailability {
            current_version: "0.9.0".into(),
            available: false,
            new_version: None,
            notes: None,
            error: None,
        });
        assert_round_trip(&UpdateAvailability {
            current_version: "0.9.0".into(),
            available: false,
            new_version: None,
            notes: None,
            error: Some(crate::error::AppError::Offline.to_payload()),
        });

        // The error half of every fallible command: Result<RepoId, AppError>.
        // `AppError` serializes through its frozen `AppErrorPayload` wire shape
        // (and deliberately has no `Deserialize` - a lossy payload cannot
        // reconstruct the variant), so the round trip is verified against the
        // wire form the frontend actually receives: serialize the
        // Result<RepoId, AppError>, read it back as Result<RepoId,
        // AppErrorPayload>, and assert the re-serialization is identical.
        let err: Result<RepoId, crate::error::AppError> = Err(crate::error::AppError::NotFound {
            entity: "repo".into(),
        });
        let json = serde_json::to_string(&err).expect("serialize error result");
        let wire: Result<RepoId, crate::error::AppErrorPayload> =
            serde_json::from_str(&json).expect("deserialize error wire form");
        let json2 = serde_json::to_string(&wire).expect("re-serialize error wire form");
        assert_eq!(json, json2, "error wire round-trip must be lossless");
        // Sanity: the wire payload carries the stable code, not the variant.
        match wire {
            Err(payload) => assert_eq!(payload.code, "db.not_found"),
            Ok(_) => panic!("expected the Err half"),
        }
    }
}
