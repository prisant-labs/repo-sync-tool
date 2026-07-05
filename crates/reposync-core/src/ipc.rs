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
