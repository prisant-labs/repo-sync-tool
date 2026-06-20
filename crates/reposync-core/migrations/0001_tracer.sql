-- 0001_tracer.sql - the week-1 tracer schema.
--
-- Migration discipline:
--   * PRE-V1: this migration may be edited and the database reset freely. The
--     three tracer tables here can change shape until V1 ships.
--   * POST-V1: this file is frozen. All schema changes ship as new, additive
--     migration files (e.g. the two activity_records indexes deferred from full
--     E-02 land as a later 0002_*.sql, never by editing this one).
--
-- These three tables mirror docs/internal/strategy-and-roadmap.md Section 4.2,
-- INCLUDING the ratified additions: repos.scoped_bookmark_blob,
-- repo_local_state.consecutive_failures, and repo_local_state.auto_paused.
-- The activity_records indexes (idx_activity_repo_time / idx_activity_time) and
-- the remaining V1 tables (repo_remote_meta, groups, settings, ...) are
-- intentionally deferred to the full E-02 effort as additive migrations.

CREATE TABLE repos (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    local_name TEXT NOT NULL,
    local_path TEXT NOT NULL UNIQUE,
    remote_origin_url TEXT,
    host_type TEXT NOT NULL DEFAULT 'unknown',
    default_branch TEXT,
    update_mode TEXT NOT NULL DEFAULT 'fetch_only',
    check_frequency_min INTEGER NOT NULL DEFAULT 360,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    notes TEXT,
    scoped_bookmark_blob TEXT
);

CREATE TABLE repo_local_state (
    repo_id INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
    active_branch TEXT,
    head_sha TEXT,
    upstream_branch TEXT,
    ahead_count INTEGER,
    behind_count INTEGER,
    is_dirty INTEGER NOT NULL DEFAULT 0,
    is_detached INTEGER NOT NULL DEFAULT 0,
    last_local_commit_at INTEGER,
    last_checked_at INTEGER,
    last_updated_at INTEGER,
    last_attempted_at INTEGER,
    last_error_code TEXT,
    next_check_at INTEGER,
    consecutive_failures INTEGER NOT NULL DEFAULT 0,
    auto_paused INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE activity_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    timestamp INTEGER NOT NULL,
    action_type TEXT NOT NULL,
    status TEXT NOT NULL,
    reason_code TEXT,
    summary TEXT,
    commit_range TEXT,
    raw_command TEXT,
    raw_stdout TEXT,
    raw_stderr TEXT,
    exit_code INTEGER,
    duration_ms INTEGER
);
