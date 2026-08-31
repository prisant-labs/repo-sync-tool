# RepoSync database migrations

These numbered `.sql` files are the v1 SQLite schema. They are embedded into the
binary at compile time by `sqlx::migrate!("./migrations")` (in `src/db.rs`) and
applied in order at app startup, against a single WAL-mode `SqlitePool`.

The authoritative schema is `docs/internal/strategy-and-roadmap.md` Section 4.2
(the full DDL). These files match it exactly, including the four ratified
additions (`repos.scoped_bookmark_blob`, `repo_local_state.consecutive_failures`,
`repo_local_state.auto_paused`, `repo_remote_meta.etag`).

## Files

- `0001_init.sql` - core registry (`repos`) and per-repo state
  (`repo_local_state`, `repo_remote_meta`).
- `0002_activity_settings.sql` - audit trail (`activity_records` + its two
  indexes), grouping (`groups`, `repo_groups`), and the `settings` singleton.
- `0003_cadence_inherit.sql` - data migration (BL-NI-20) that rewrites every
  `repos.check_frequency_min` to `0`, the INHERIT sentinel, so existing repos
  follow the global cadence (`settings.global_check_minutes`). Additive and
  data-only.
- `0004_default_cadence_inherit.sql` - aligns the `repos.check_frequency_min`
  schema DEFAULT with the inherit model (BL-NI-34): `0`, not the old `360`, so a
  future INSERT relying on the column default inherits the global cadence instead
  of silently creating a 6-hour override. SQLite cannot alter a column default in
  place, so this is a table rebuild (create-copy-drop-rename); it carries the
  `-- no-transaction` directive because the rebuild disables foreign keys (only
  legal outside a transaction) so `DROP TABLE repos` does not cascade-delete the
  child rows, and wraps the rebuild in its own `BEGIN`/`COMMIT` for atomicity.
  Additive and non-destructive: every column and row is preserved with the same
  id, so all inbound foreign keys stay valid.
- `0005_branch_intel.sql` - additive branch/PR-intelligence columns on
  `repo_remote_meta` (E-17): the open-PR counts, their own ETag + last-checked
  staleness marker, and the decoupled release ETag + last-checked (BL-NI-15b).
  Every column is NULLable, so existing rows backfill to NULL ("unknown", never a
  fabricated zero); a plain `ALTER TABLE ADD COLUMN`, no rebuild.
- `0006_auto_update.sql` - additive `settings.auto_update_check` column (E-18):
  the on-launch app-update-check toggle, `NOT NULL DEFAULT 1` (on). `settings` has
  no inbound foreign keys, so a plain `ALTER TABLE ADD COLUMN` is safe.
- `0007_close_minimizes_to_tray.sql` - additive
  `settings.close_minimizes_to_tray` column (E-13): whether the window's close
  button hides to the tray or quits, `NOT NULL DEFAULT 1` (hide), which preserves
  the previously hardcoded behavior for every existing install.
- `0008_upstream_state.sql` - additive `repo_local_state.upstream_state` column
  (BL-NI-77): the three-state upstream classification (`tracking` / `none` /
  `deleted`) that `policy::UpstreamState` already produced on every check and
  that nothing persisted. `upstream_branch` is NULL for both `none` and
  `deleted`, so without this the UI could not tell a repo that is genuinely in
  sync from one whose upstream was deleted, and it defaulted to the reassuring
  answer. **NULLABLE with no default, deliberately:** a backfilled value would be
  a classification no check ever made for that row, so NULL means "not observed
  since this column existed" and resolves on the repo's next check.
- `0009_editor_terminal_defaults.sql` - data migration that backfills
  `settings.editor_command` and `settings.terminal_command`, which 0002 declared
  with no DEFAULT and which have therefore been NULL on every install. Both
  `repo_open_editor` and `repo_open_terminal` return `InvalidSetting` on NULL, so
  "Open in -> Editor" and "-> Terminal" have never worked out of the box while the
  Settings placeholders ("code", "default") read like configured values. Sets
  `code` and `wt` only where the value is NULL or blank, so a deliberate choice is
  never overwritten. SQLite cannot add a DEFAULT to an existing column without
  rebuilding the table, so fresh installs get the same values from the seeding
  INSERT in `store::settings_get` instead: the singleton row does not exist yet
  when this migration runs.
- `0010_remote_metadata.sql` - additive GitHub repo-resource columns on
  `repo_remote_meta`: `stars`, `forks`, `license`, `size`, `visibility`,
  `homepage`. All six already arrive on every GitHub repo-resource response
  that `github.rs` fetches and were discarded rather than persisted; they
  unblock the gated Repos table columns and the "open homepage" link glyph in
  the UI finalization roadmap's DataTable slice. Every column is NULLable with
  no default (unknown or absent is NULL, never a fabricated zero or empty
  string), so a plain `ALTER TABLE ADD COLUMN`, no rebuild. Also includes a
  data-migration statement, `UPDATE repo_remote_meta SET etag = NULL`, found
  necessary by a Codex adversarial review of this PR: the repo-resource ETag
  cached before this migration would otherwise keep answering 304 Not
  Modified forever on an upgraded database, and the 304 path only bumps
  `last_fetched_at` - it never rewrites the six columns just added - so an
  unchanged repo's new columns would stay NULL indefinitely. Clearing the
  ETag forces exactly one full 200 fetch per repo on its next due pass, which
  self-heals every column with no further code change. `release_etag` and
  `pr_etag` are deliberately left alone; they are the release and
  pull-request sub-resources' own independently-cached ETags (BL-NI-15b) and
  have no bearing on the six repo-resource columns this migration adds.

## Migration policy

### Pre-V1 (now): freely resettable

Until V1 ships, the schema is not yet a frozen contract. Any migration here may
be edited and the database reset (delete the file, restart) without ceremony.
This window is what lets the four ratified columns land in the INITIAL migration
rather than as later additive bolt-ons.

### Post-V1: additive-only, never destructive

Once V1 ships, every file in this directory is FROZEN. You may never edit an
existing migration (sqlx tracks each file's checksum and refuses to run if a
previously-applied migration's content changed). All schema evolution ships as a
NEW, higher-numbered migration file, and it must be additive:

- Allowed: new tables; new columns with a `DEFAULT` (so existing rows backfill);
  new indexes.
- Forbidden: dropping a table or column; renaming a table or column; changing a
  column type; adding a `NOT NULL` column without a default; any change that
  loses or invalidates existing data.

This rule exists because users carry their database forward across upgrades. A
destructive change would corrupt or discard a user's tracked-repo registry and
activity history. When a column truly must change shape, add a new column, copy
forward in a data migration, and leave the old column in place (deprecated, but
present).

There is exactly ONE migration at each version number. Never leave two `0001_*`
files in this directory: `sqlx::migrate!` keys off the numeric prefix and two
files sharing a version break the runner.
