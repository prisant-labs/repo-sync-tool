---
effort: E-02
plan_for: spec.md
status: ready
---

# E-02 Implementation Plan

## Approach

Build the floor in dependency order: resolve paths first (everything else needs a directory), then author the schema as numbered migrations, then wire the pool + runner that applies them, then harden the startup story (OneDrive warning, migration-failure recovery). Treat the schema as a frozen contract from the moment it lands: it is the interface E-08/E-09/E-10 code against, so getting the exact column and index names right is the whole job. Test against a real on-disk SQLite file in a tempdir so WAL behavior and the migration runner are exercised exactly as they run in production.

## Steps

1. **Resolve the path seam (`paths.rs`).** Replace the E-01 stub with the real resolver. Return resolved `PathBuf`s for the data dir, db file, and log dir: `local_data_dir()/RepoSync` on Windows (`%LOCALAPPDATA%`), `~/Library/Application Support/RepoSync` on macOS. This is the ONLY module that computes a path. Add a `corrupt-backups/` subdir helper for step 7. Create the data dir on first resolve if absent.
2. **OneDrive-root detection.** In `paths.rs` (or a small helper it calls), check the resolved data dir against known OneDrive roots (the `OneDrive` / `OneDriveConsumer` env vars and path prefix). If it matches, log a warning and prefer a non-synced location. Never resolve to Documents or Desktop. This is defense in depth; `%LOCALAPPDATA%` already avoids the sync path.
3. **Author migration 0001 (core registry + state).** Migration-file split decision: two files, `0001_init.sql` (registry + state) and `0002_activity_settings.sql` (activity + grouping + settings), grouped one logical-area-cluster per file to keep the freeze legible. All four ratified additions land in the INITIAL migration 0001: `repos.scoped_bookmark_blob`, `repo_local_state.consecutive_failures`, `repo_local_state.auto_paused`, and `repo_remote_meta.etag` (the schema is still freely resettable pre-V1, so they must be present from the first migration, not bolted on later). `crates/reposync-core/migrations/0001_init.sql`: `CREATE TABLE repos (...)` with the full column list - `id INTEGER PK AUTOINCREMENT`, `local_name TEXT NOT NULL`, `local_path TEXT NOT NULL UNIQUE`, `remote_origin_url`, `host_type` (default `'unknown'`), `default_branch`, `update_mode` (default `'fetch_only'`), `check_frequency_min` (default 360), `enabled` (default 1), `created_at INTEGER NOT NULL`, `notes`, and the nullable `scoped_bookmark_blob TEXT` (AC2). Then `repo_local_state` (1:1 with `repos`, `ON DELETE CASCADE`: `active_branch`, `head_sha`, `upstream_branch`, `ahead_count`, `behind_count`, `is_dirty`, `is_detached`, `last_local_commit_at`, `last_checked_at`, `last_updated_at`, `last_attempted_at`, `last_error_code`, `next_check_at`, `consecutive_failures` default 0, `auto_paused` default 0 - the last two back the 3-strikes auto-pause, AC2) and `repo_remote_meta` (1:1, `ON DELETE CASCADE`: `description`, `topics_json`, `latest_release_tag`, `latest_release_at`, `latest_release_url`, `is_archived`, `last_remote_sha`, `last_fetched_at`, and the nullable `etag TEXT`, AC9).
4. **Author migration 0002 (activity + grouping + settings).** `activity_records` with `id INTEGER PK AUTOINCREMENT`, `repo_id` (FK `ON DELETE CASCADE`), `timestamp INTEGER NOT NULL`, `action_type` (`check|fetch|pull_ff|pull|rebase|open|enable|disable|manual_retry`), `status` (`success|skipped|warning|failed`), `reason_code`, `summary`, `commit_range`, `raw_command`, `raw_stdout`, `raw_stderr`, `exit_code`, `duration_ms`, plus indexes `idx_activity_repo_time ON (repo_id, timestamp DESC)` and `idx_activity_time ON (timestamp DESC)` (AC3). `groups` (`id`, `name TEXT NOT NULL UNIQUE`, `color`) + `repo_groups` join table (`PRIMARY KEY (repo_id, group_id)`, both FK `ON DELETE CASCADE`, N:M). `settings` as a singleton (`CHECK (id = 1)`) with `global_check_minutes` (default 360), `quiet_hours_start`/`quiet_hours_end`, `notify_on_release`/`notify_on_failure` (default 1), `git_executable_path`, `editor_command`/`terminal_command`, `autostart` (default 0), `activity_retention_d` (default 90), and `github_token_present` (default 0). Numbering one-area-cluster-per-file vs. one-table-per-file is a judgment call; group by logical area to keep the freeze legible.
5. **Connection pool + WAL.** Build a single shared `SqlitePool` against the resolved db path with `journal_mode=WAL` (and sensible `busy_timeout`/`synchronous` pragmas) set at connect time. Expose the pool as the managed handle `src-tauri` will register; `reposync-core` stays Tauri-free and just hands back the pool/init function.
6. **Migration runner.** Apply migrations at startup with `sqlx::migrate!` (embedded at compile time from `crates/reposync-core/migrations/`). The runner is called once during app init, before any command can touch the DB.
7. **Migration-failure recovery.** Wrap the runner: on error, (a) log via the logging seam, (b) move the existing DB file (and `-wal`/`-shm` sidecars) to `corrupt-backups/reposync-<timestamp>.db`, (c) create a fresh DB and re-run migrations, (d) raise a one-time notice flag the shell can surface later. Do not silently delete data; do not crash to a blank window (AC7).
8. **Document the additive-only policy.** Add a short `README.md` (or header comment in the migrations dir) stating: post-V1 migrations are additive-only - new columns with defaults, new tables, never destructive renames or drops; pre-V1 the schema may be reset freely (AC8).
9. **Verify.** Run the integration tests below on Windows; confirm a fresh DB is created with WAL active, all tables/indexes/the singleton exist, and a corrupted-DB scenario recovers without crashing.

## Test strategy

- **Migration apply test.** Point `paths` at a tempdir, run the migration runner against a fresh file, and assert every table, the ratified-addition columns (`repos.scoped_bookmark_blob`, `repo_local_state.consecutive_failures`, `repo_local_state.auto_paused`, `repo_remote_meta.etag`), both `activity_records` indexes (`idx_activity_repo_time`, `idx_activity_time`), and the `settings` singleton (`CHECK (id = 1)` rejects a second row) exist via `PRAGMA`/`sqlite_master` queries.
- **WAL mode test.** Assert `PRAGMA journal_mode` returns `wal` on the live connection.
- **Path resolution test.** Inject env so the resolver is deterministic; assert the Windows and macOS branches return the expected `RepoSync` subpaths and that nothing else in the crate computes a path (a grep-style guard, or a single public path API that all callers must use).
- **OneDrive-warning test.** Simulate a data dir under a OneDrive root and assert a warning is logged and a non-synced location is preferred.
- **Migration-failure recovery test.** Seed a deliberately corrupt or partially-migrated DB file, run startup, and assert the file is moved into `corrupt-backups/`, a fresh usable DB exists, and the one-time notice flag is set, with no panic.
- These run in plain `cargo test` against an on-disk tempdir SQLite, no Tauri host, consistent with the headless-core rule.

## Files / modules touched

- `crates/reposync-core/src/paths.rs` (real implementation replacing the E-01 stub).
- `crates/reposync-core/migrations/0001_init.sql`, `crates/reposync-core/migrations/0002_activity_settings.sql` (numbering may split differently, additive only).
- `crates/reposync-core/migrations/README.md` (additive-only policy note).
- A DB init/pool module in `reposync-core` (e.g. `db.rs` or within `lib.rs`) owning the `SqlitePool`, WAL pragmas, the `sqlx::migrate!` call, and the failure-recovery wrapper.
- `src-tauri/src/main.rs` (register the pool as managed state; call init at startup) - thin wiring only.
- `crates/reposync-core/Cargo.toml` (sqlx with the sqlite + runtime features; confirm no `tauri`).

## Risks and mitigations

- **WAL sidecar corruption on synced folders.** Mitigated structurally by `%LOCALAPPDATA%` + the OneDrive-root check; the path choice is the primary defense, the warning is the backstop.
- **sqlx macro posture friction.** If compile-time macros are chosen, a missing `DATABASE_URL`/`.sqlx` cache breaks CI offline builds. Default to the runtime query API to avoid this; if macros are chosen, commit the `.sqlx` cache and add a CI check. Flag the choice to jp (spec open question).
- **Schema churn after freeze.** Because E-08/E-09/E-10 code against these names, a late rename is expensive. Mitigate by reviewing the column list against the authoritative full DDL in `docs/internal/strategy-and-roadmap.md` Section 4.2 (not the abbreviated brief Section 4.5 table) before the first downstream effort starts, and by the additive-only policy thereafter.
- **Migration-failure recovery moving a locked file.** On Windows a `-wal`/`-shm` file may be locked. Ensure the pool is closed before the move; retry briefly, and if the move fails, fall back to a uniquely-named fresh DB rather than crashing.

## Definition of done

All nine acceptance criteria checked, the integration tests green in `cargo test` on Windows and in CI, `reposync-core` still has no `tauri` in its dependency tree, the schema reviewed against the authoritative full DDL in `docs/internal/strategy-and-roadmap.md` Section 4.2 for exact names, and the branch ready for self-merge per `EXECUTION.md`.
