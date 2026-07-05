-- 0001_init.sql - the v1 core registry and per-repo state (E-02).
--
-- Migration discipline (see migrations/README.md for the full policy):
--   * PRE-V1: this migration may be edited and the database reset freely.
--   * POST-V1: this file is FROZEN. All schema changes ship as new, additive
--     migration files (new tables, new columns with defaults), never by editing
--     this one and never with a destructive rename or drop.
--
-- These three tables mirror docs/internal/strategy-and-roadmap.md Section 4.2
-- (the authoritative full DDL) EXACTLY, INCLUDING the four ratified additions,
-- all of which land here in the INITIAL migration while the schema is still
-- freely resettable:
--   * repos.scoped_bookmark_blob             (macOS App Store scoped bookmark slot)
--   * repo_local_state.consecutive_failures  (3-strikes auto-pause; E-07 reads)
--   * repo_local_state.auto_paused           (3-strikes auto-pause; E-08 sets)
--   * repo_remote_meta.etag                  (HTTP If-None-Match cache; E-10)
--
-- The activity_records audit table, its indexes, the groups/repo_groups grouping
-- tables, and the settings singleton live in the additive 0002 migration.

-- Core repo registry.
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

-- Cached local git state, refreshed each check (1:1 with repos).
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

-- Cached remote / host metadata, refreshed less often (1:1 with repos).
CREATE TABLE repo_remote_meta (
    repo_id INTEGER PRIMARY KEY REFERENCES repos(id) ON DELETE CASCADE,
    description TEXT,
    topics_json TEXT,
    latest_release_tag TEXT,
    latest_release_at INTEGER,
    latest_release_url TEXT,
    is_archived INTEGER NOT NULL DEFAULT 0,
    last_remote_sha TEXT,
    last_fetched_at INTEGER,
    etag TEXT
);
