-- no-transaction
-- 0004_default_cadence_inherit.sql - align the repos.check_frequency_min SCHEMA
-- DEFAULT with the INHERIT cadence model (backlog BL-NI-34).
--
-- 0003 migrated existing rows to check_frequency_min = 0 (the "inherit the global
-- cadence" sentinel), and repo::add inserts 0 explicitly. But the column DEFAULT
-- set in 0001 was still 360, so any future INSERT that omits check_frequency_min
-- (a code path relying on the default) would silently create a 6-hour per-repo
-- OVERRIDE instead of inheriting. This migration changes the schema default from
-- 360 to 0 so the default matches the model.
--
-- SQLite cannot ALTER a column's default in place, so this is the documented
-- table-rebuild: create the table with the new default, copy every row forward
-- verbatim, drop the old table, rename the new one into place. `repos` is the
-- parent of four ON DELETE CASCADE foreign keys (repo_local_state,
-- repo_remote_meta, activity_records, repo_groups), so foreign keys MUST be
-- disabled for the rebuild: with them ON, `DROP TABLE repos` would fire the
-- cascades and delete every child row. Disabling foreign keys is only legal
-- OUTSIDE a transaction, which is why this migration carries the `-- no-transaction`
-- directive (sqlx then runs it without wrapping it in its own transaction); the
-- rebuild is instead made atomic by an explicit BEGIN/COMMIT.
--
-- Migration discipline (see migrations/README.md): additive and non-destructive -
-- no column is dropped, renamed, or retyped, and every row is preserved with the
-- same id, so all inbound foreign-key relationships stay valid. 0001-0003 are
-- FROZEN; this is the only new file.

PRAGMA foreign_keys = OFF;

BEGIN;

CREATE TABLE repos_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    local_name TEXT NOT NULL,
    local_path TEXT NOT NULL UNIQUE,
    remote_origin_url TEXT,
    host_type TEXT NOT NULL DEFAULT 'unknown',
    default_branch TEXT,
    update_mode TEXT NOT NULL DEFAULT 'fetch_only',
    check_frequency_min INTEGER NOT NULL DEFAULT 0,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    notes TEXT,
    scoped_bookmark_blob TEXT
);

INSERT INTO repos_new (
    id, local_name, local_path, remote_origin_url, host_type, default_branch,
    update_mode, check_frequency_min, enabled, created_at, notes, scoped_bookmark_blob
)
SELECT
    id, local_name, local_path, remote_origin_url, host_type, default_branch,
    update_mode, check_frequency_min, enabled, created_at, notes, scoped_bookmark_blob
FROM repos;

DROP TABLE repos;

ALTER TABLE repos_new RENAME TO repos;

COMMIT;

PRAGMA foreign_keys = ON;
