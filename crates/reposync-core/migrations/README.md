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
