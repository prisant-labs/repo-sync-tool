-- 0010_remote_metadata.sql - additive GitHub repo-resource columns on
-- repo_remote_meta: stars, forks, license, size, visibility, homepage.
--
-- These six fields already arrive on every GitHub GET /repos/{owner}/{repo}
-- response (stargazers_count, forks_count, license, size, visibility,
-- homepage) that github.rs's repo-resource fetch already reads, and were
-- discarded on the floor rather than persisted. Unblocks the gated Repos
-- table columns (stars, forks, license, size, visibility) and the "open
-- homepage" link glyph in the upcoming DataTable slice.
--
-- Every column is NULLable with no default, so existing rows backfill to
-- NULL and a field GitHub omits (or a repo that genuinely has none, such as
-- one with no homepage or no detected license) is never written as a
-- fabricated zero or empty string - the same discipline as migration
-- 0005_branch_intel.sql. No column is dropped, renamed, or retyped; the
-- repo_remote_meta primary key and its ON DELETE CASCADE foreign key to
-- repos are untouched, so a plain ALTER TABLE ADD COLUMN is safe and needs
-- no table rebuild (unlike 0004).
--
-- Column notes (mapped in github.rs::map_metadata):
--   * stars / forks: stargazers_count / forks_count, verbatim integers.
--   * license: a single string identifier, the license object's spdx_id
--     (e.g. "MIT"; GitHub returns "NOASSERTION" for a license it detects
--     but cannot classify) when the response nests license as an object;
--     NULL when license is null or absent.
--   * size: the repo size in kilobytes, as GitHub reports it.
--   * visibility: GitHub's own string ("public" / "private" / "internal"),
--     stored verbatim.
--   * homepage: NULL when GitHub returns null OR an empty string, never a
--     fabricated value.
--
-- Migration discipline (see migrations/README.md): additive-only. 0001-0009
-- are FROZEN; this is the only new file.

ALTER TABLE repo_remote_meta ADD COLUMN stars INTEGER;
ALTER TABLE repo_remote_meta ADD COLUMN forks INTEGER;
ALTER TABLE repo_remote_meta ADD COLUMN license TEXT;
ALTER TABLE repo_remote_meta ADD COLUMN size INTEGER;
ALTER TABLE repo_remote_meta ADD COLUMN visibility TEXT;
ALTER TABLE repo_remote_meta ADD COLUMN homepage TEXT;

-- One-time cache invalidation for an UPGRADED database (Codex adversarial
-- review of this migration, confirmed 2026-08-28): a row that already had a
-- repo-resource `etag` before this migration keeps answering 304 Not
-- Modified forever, and refresh_one's 304 path bumps only `last_fetched_at`
-- - it never rewrites the six columns just added. Left alone, an unchanged
-- repo on an upgraded install would carry NULL stars/forks/license/size/
-- visibility/homepage indefinitely, even though the repo has been refreshed
-- many times; a fresh install has no such stale ETag and is unaffected.
-- Clearing `etag` here means the NEXT due refresh pass sends no
-- If-None-Match, so the repo resource fetch gets a full 200 body once,
-- populates the six new columns from it, and stores a fresh ETag - this
-- self-heals in exactly one pass per repo, with no code change needed
-- beyond the schema. `release_etag` and `pr_etag` are deliberately left
-- untouched: the six fields added by this migration come from the repo
-- resource only, and the release/PR sub-resources already keep their own
-- independently-cached ETags by design (BL-NI-15b, the ETag decoupling
-- migration 0005_branch_intel.sql introduced) - clearing them here would
-- force a needless re-fetch of data this migration has no bearing on.
UPDATE repo_remote_meta SET etag = NULL;
