-- 0005_branch_intel.sql - branch and pull-request intelligence columns (E-17).
--
-- Additive, non-destructive columns on repo_remote_meta (E-02 owns the table; E-17
-- specifies the columns it needs and coordinates the migration). Two groups:
--
--   * The pull-request intelligence E-17 adds: open_pr_count / default_branch_pr_count
--     (the counts surfaced in the row + drawer), pr_etag (the pulls-endpoint
--     If-None-Match cache), pr_last_checked_at (the "as of <time>" staleness marker).
--   * The release sub-resource's OWN etag + last-checked (BL-NI-15b): release_etag /
--     release_last_checked_at, so a repo-resource 304 never hides a new release for the
--     24h window. This decoupling is the same own-cache discipline the PR columns get.
--
-- Every column is NULLable with no default, so existing rows backfill to NULL (which
-- the code reads as "unknown / never fetched", never a fabricated zero - AC5). No
-- column is dropped, renamed, or retyped; the repo_remote_meta primary key and its
-- ON DELETE CASCADE foreign key to repos are untouched, so a plain ALTER TABLE ADD
-- COLUMN is safe and needs no table rebuild (unlike 0004).
--
-- Migration discipline (see migrations/README.md): additive-only. 0001-0004 are
-- FROZEN; this is the only new file. The additive-migration sequence is coordinated
-- across 0004 (P1-C cadence default, BL-NI-34), 0005 (this effort, E-17), and 0006
-- (E-18 auto-update and distribution).

ALTER TABLE repo_remote_meta ADD COLUMN open_pr_count INTEGER;
ALTER TABLE repo_remote_meta ADD COLUMN default_branch_pr_count INTEGER;
ALTER TABLE repo_remote_meta ADD COLUMN pr_etag TEXT;
ALTER TABLE repo_remote_meta ADD COLUMN pr_last_checked_at INTEGER;
ALTER TABLE repo_remote_meta ADD COLUMN release_etag TEXT;
ALTER TABLE repo_remote_meta ADD COLUMN release_last_checked_at INTEGER;
