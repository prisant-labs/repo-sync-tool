-- 0002_activity_settings.sql - the audit trail, grouping, and settings (E-02).
--
-- The second of the two v1 migrations. It carries the audit table and its two
-- query indexes, the N:M grouping tables, and the singleton settings row. All
-- names mirror docs/internal/strategy-and-roadmap.md Section 4.2 EXACTLY.
--
-- Same migration discipline as 0001 (see migrations/README.md): freely
-- resettable pre-V1, FROZEN and additive-only after V1 ships.

-- Audit trail of every operation. raw_command / raw_stdout / raw_stderr keep the
-- full git invocation so the activity log can show exactly what ran.
CREATE TABLE activity_records (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    timestamp INTEGER NOT NULL,
    action_type TEXT NOT NULL, -- check | fetch | pull_ff | pull | rebase | open | enable | disable | manual_retry
    status TEXT NOT NULL,      -- success | skipped | warning | failed
    reason_code TEXT,
    summary TEXT,
    commit_range TEXT,
    raw_command TEXT,
    raw_stdout TEXT,
    raw_stderr TEXT,
    exit_code INTEGER,
    duration_ms INTEGER
);
CREATE INDEX idx_activity_repo_time ON activity_records(repo_id, timestamp DESC);
CREATE INDEX idx_activity_time ON activity_records(timestamp DESC);

-- Grouping / tagging (N:M between repos and groups).
CREATE TABLE groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL UNIQUE,
    color TEXT
);
CREATE TABLE repo_groups (
    repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE,
    group_id INTEGER NOT NULL REFERENCES groups(id) ON DELETE CASCADE,
    PRIMARY KEY (repo_id, group_id)
);

-- Settings (singleton: the CHECK (id = 1) guard makes a second row impossible).
CREATE TABLE settings (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    global_check_minutes INTEGER NOT NULL DEFAULT 360,
    quiet_hours_start INTEGER, -- minutes since midnight
    quiet_hours_end INTEGER,
    notify_on_release INTEGER NOT NULL DEFAULT 1,
    notify_on_failure INTEGER NOT NULL DEFAULT 1,
    git_executable_path TEXT,
    editor_command TEXT,
    terminal_command TEXT,
    autostart INTEGER NOT NULL DEFAULT 0,
    activity_retention_d INTEGER NOT NULL DEFAULT 90,
    github_token_present INTEGER NOT NULL DEFAULT 0
);
