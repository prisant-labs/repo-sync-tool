---
effort: E-02
tracking-issue: 4
title: Persistence and Paths
status: ready
tier: MUST
scope: V1 (non-GUI)
depends_on: [E-01]
status_detail: complete (2026-06-24)
source: docs/internal/v1-architecture-and-decisions.md (Sections 4.5, 4.10b/c); docs/internal/strategy-and-roadmap.md Section 4.2 (authoritative full DDL)
---

# E-02 - Persistence and Paths

## Task Summary

> Agents keep this block current as work proceeds.

- **State:** complete. All 9 ACs implemented test-first and green. The v1 schema ships as two numbered migrations (`0001_init.sql` = repos + repo_local_state + repo_remote_meta; `0002_activity_settings.sql` = activity_records + its two indexes, groups + repo_groups, settings singleton), with all four ratified additions in `0001`. The pre-V1 `0001_tracer.sql` was renamed to `0001_init.sql` (exactly one 0001; the tracer `repo::add`/`check_now` still pass). `paths.rs` is the sole path seam (per-OS data/db/log dirs + corrupt-backups helper + OneDrive-root detection). `db.rs` gained `init_pool_with_recovery` (WAL pool + migrate! + move-aside-and-recover on failure). A new `store.rs` holds the Tauri-free data access for the persistence-backed commands; the `repo_list`/`repo_get`/`repo_remove`/`repo_set_enabled`/`settings_get`/`settings_set`/`repo_scan_parent` handlers now call it. sqlx posture: runtime query API (no compile-time DATABASE_URL/macros). Migrations README documents the additive-only post-V1 policy.
- **Review fixes (2026-06-24):** three verified code-review findings applied. M-3: `repo::check_now` now uses `fetch_optional` and maps a missing repo id to `AppError::NotFound` (was a generic `db.query_failed`), mirroring `store::repo_get`; new failing-first test `repo::tests::check_now_missing_repo_is_not_found`. M-2: the `init_pool_with_recovery` recovery notice is now carried into `src-tauri` `AppState` via new `db_recovered: bool` / `db_backup_path: Option<PathBuf>` fields (pure shell wiring) so a later UI/command can surface AC7's one-time notice instead of it being logged and dropped. L-2: corrupt-backup filenames now get sub-second uniqueness (a `-N` suffix loop in a new `unique_backup_dest` helper) so two recoveries in the same whole second no longer collide; asserted by `db::tests::move_db_aside_twice_produces_distinct_paths`. The warn-only OneDrive-rooted-data-dir gap (M-1) was filed as backlog `BL-NI-12`, not changed in code.
- **Next:** none for E-02. Downstream: E-08 scheduler writes `repo_local_state`; E-09 activity writer + retention; E-10 GitHub client writes `repo_remote_meta`/`etag`.
- **Blockers:** none.

## Context

This effort owns the durable floor everything else stands on: the v1 SQLite schema, the migration runner, the connection pool, and the single platform seam that resolves where data, database, and logs live. The schema authored here is the **contract** consumed by E-08 (scheduler), E-09 (activity writer), and E-10 (GitHub client); freezing it correctly is what unblocks those workstreams.

Two non-obvious decisions are load-bearing and both come from the brief. First, the `scoped_bookmark_blob TEXT` column must be added to `repos` in the **first** migration, while the schema is still freely resettable, because the post-V1 policy forbids destructive changes and the macOS App Store sandbox door is cheap to keep open now and expensive to open later. Second, the database and logs must live in `%LOCALAPPDATA%` (never Roaming, never a OneDrive-synced folder), because a SQLite file in WAL mode with `-wal`/`-shm` sidecars corrupts when a cloud sync agent snapshots it mid-write.

This effort writes the schema and the path/pool/migration plumbing. It does NOT write the activity-writer logic (E-09), the scheduler reads/writes (E-08), or the GitHub cache writes (E-10); those consume the tables defined here.

## In scope

- The v1 SQLite schema as numbered `.sql` files under `crates/reposync-core/migrations/`, covering the six logical areas below. The authoritative DDL is `docs/internal/strategy-and-roadmap.md` Section 4.2 (the full DDL), NOT the abbreviated "key fields" table in the architecture brief Section 4.5. The full per-table column list is:
  - **`repos`**: `id INTEGER PK AUTOINCREMENT`, `local_name TEXT NOT NULL`, `local_path TEXT NOT NULL UNIQUE`, `remote_origin_url TEXT`, `host_type TEXT NOT NULL DEFAULT 'unknown'`, `default_branch TEXT`, `update_mode TEXT NOT NULL DEFAULT 'fetch_only'`, `check_frequency_min INTEGER NOT NULL DEFAULT 360`, `enabled INTEGER NOT NULL DEFAULT 1`, `created_at INTEGER NOT NULL`, `notes TEXT`, `scoped_bookmark_blob TEXT` (ratified addition).
  - **`repo_local_state`**: `repo_id INTEGER PK REFERENCES repos(id) ON DELETE CASCADE`, `active_branch TEXT`, `head_sha TEXT`, `upstream_branch TEXT`, `ahead_count INTEGER`, `behind_count INTEGER`, `is_dirty INTEGER NOT NULL DEFAULT 0`, `is_detached INTEGER NOT NULL DEFAULT 0`, `last_local_commit_at INTEGER`, `last_checked_at INTEGER`, `last_updated_at INTEGER`, `last_attempted_at INTEGER`, `last_error_code TEXT`, `next_check_at INTEGER`, `consecutive_failures INTEGER NOT NULL DEFAULT 0` (ratified addition), `auto_paused INTEGER NOT NULL DEFAULT 0` (ratified addition).
  - **`repo_remote_meta`**: `repo_id INTEGER PK REFERENCES repos(id) ON DELETE CASCADE`, `description TEXT`, `topics_json TEXT`, `latest_release_tag TEXT`, `latest_release_at INTEGER`, `latest_release_url TEXT`, `is_archived INTEGER NOT NULL DEFAULT 0`, `last_remote_sha TEXT`, `last_fetched_at INTEGER`, `etag TEXT` (ratified addition).
  - **`activity_records`**: `id INTEGER PK AUTOINCREMENT`, `repo_id INTEGER NOT NULL REFERENCES repos(id) ON DELETE CASCADE`, `timestamp INTEGER NOT NULL`, `action_type TEXT NOT NULL` (`check|fetch|pull_ff|pull|rebase|open|enable|disable|manual_retry`), `status TEXT NOT NULL` (`success|skipped|warning|failed`), `reason_code TEXT`, `summary TEXT`, `commit_range TEXT`, `raw_command TEXT`, `raw_stdout TEXT`, `raw_stderr TEXT`, `exit_code INTEGER`, `duration_ms INTEGER`. Indexes: `idx_activity_repo_time ON (repo_id, timestamp DESC)`; `idx_activity_time ON (timestamp DESC)`.
  - **`groups`**: `id INTEGER PK AUTOINCREMENT`, `name TEXT NOT NULL UNIQUE`, `color TEXT`. **`repo_groups`**: `repo_id`, `group_id`, `PRIMARY KEY (repo_id, group_id)`, both FK `ON DELETE CASCADE`.
  - **`settings`**: `id INTEGER PK CHECK (id=1)`, `global_check_minutes INTEGER NOT NULL DEFAULT 360`, `quiet_hours_start INTEGER`, `quiet_hours_end INTEGER`, `notify_on_release INTEGER NOT NULL DEFAULT 1`, `notify_on_failure INTEGER NOT NULL DEFAULT 1`, `git_executable_path TEXT`, `editor_command TEXT`, `terminal_command TEXT`, `autostart INTEGER NOT NULL DEFAULT 0`, `activity_retention_d INTEGER NOT NULL DEFAULT 90`, `github_token_present INTEGER NOT NULL DEFAULT 0`.
- The `repos.scoped_bookmark_blob TEXT` nullable column in the initial migration (reserved for the macOS App Store security-scoped bookmark; `NULL` everywhere in V1).
- The `repo_local_state.consecutive_failures INTEGER NOT NULL DEFAULT 0` and `repo_local_state.auto_paused INTEGER NOT NULL DEFAULT 0` columns in the initial migration. These back the 3-strikes auto-pause: E-07 (policy) reads `consecutive_failures`; E-08 (scheduler) writes `consecutive_failures` and sets `auto_paused` when the strike threshold is hit. Ratified 2026-06-19.
- The `repo_remote_meta.etag TEXT` nullable column for HTTP `If-None-Match` conditional requests (consumed by E-10). The strategy DDL's `last_remote_sha`/`last_fetched_at` are the commit SHA and the fetch clock, not the HTTP ETag, so the ETag needs its own column. Added in the initial migration while the schema is still resettable.
- The `activity_records` indexes `idx_activity_repo_time ON (repo_id, timestamp DESC)` and `idx_activity_time ON (timestamp DESC)`.
- The `settings` singleton row guarded by `CHECK (id = 1)`, including `activity_retention_d` (default 90) and `git_executable_path`.
- The `sqlx::migrate!` runner, embedding the migrations at compile time and applying them at startup.
- WAL mode enabled on the connection and a single shared `SqlitePool`.
- The `paths.rs` seam: the ONLY place in the codebase that computes a data, db, or log path. Resolves `%LOCALAPPDATA%\RepoSync` on Windows and `~/Library/Application Support/RepoSync` on macOS, returning resolved `PathBuf`s.
- OneDrive-root detection: on startup, detect whether the resolved data dir falls under a known OneDrive root and log a warning, preferring a non-synced location.
- Migration-failure startup recovery: on migration error, log it, move the existing DB aside to `RepoSync\corrupt-backups\reposync-<timestamp>.db`, create a fresh DB, re-run migrations on the fresh DB, and surface a one-time notice.

## Out of scope

- The activity-writer logic and retention sweep (E-09); this effort only defines the `activity_records` table and the `settings.activity_retention_d` column they use.
- Scheduler reads/writes of `next_check_at`, `last_checked_at`, etc. (E-08); this effort only defines `repo_local_state`.
- GitHub metadata cache writes (E-10); this effort only defines `repo_remote_meta`.
- The `AppError` variants for DB/migration failures (E-05); use placeholder error types until E-05 lands, then map.
- Any UI surfacing of the migration-failure notice; this effort emits the signal, the screen renders it later.

## Contract / deliverables

1. Numbered `.sql` migrations under `crates/reposync-core/migrations/` define all six logical areas with the exact table and column names from the authoritative full DDL in `docs/internal/strategy-and-roadmap.md` Section 4.2 (NOT the abbreviated brief Section 4.5 table), including the ratified additions `repos.scoped_bookmark_blob`, `repo_local_state.consecutive_failures`, `repo_local_state.auto_paused`, and `repo_remote_meta.etag`.
2. `repos` includes the nullable `scoped_bookmark_blob TEXT` column; `repo_local_state` includes `consecutive_failures INTEGER NOT NULL DEFAULT 0` and `auto_paused INTEGER NOT NULL DEFAULT 0`.
3. `activity_records` carries the two indexes `idx_activity_repo_time ON (repo_id, timestamp DESC)` and `idx_activity_time ON (timestamp DESC)`.
4. `settings` is a singleton (`CHECK (id = 1)`) with `activity_retention_d` defaulting to 90 and a `git_executable_path` column.
5. `sqlx::migrate!` applies the migrations at startup against a WAL-mode, single-`SqlitePool` connection.
6. `paths.rs` is the sole path resolver, returning the correct per-OS data/db/log dirs, and warns when the data dir sits under a OneDrive root.
7. A migration failure does not crash the app: the DB is moved to `corrupt-backups/`, a fresh DB is created, migrations are re-run on the fresh DB, and a one-time notice is raised.

## Acceptance criteria

- [x] AC1: The schema defines `repos`, `repo_local_state`, `repo_remote_meta`, `activity_records`, `groups` + `repo_groups`, and `settings` with the exact column names from the full DDL (including all ratified additions). Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (the authoritative full DDL). Verified by `db::tests::migrations_create_all_v1_tables`.
- [x] AC2: `repos` contains a nullable `scoped_bookmark_blob TEXT` column, and `repo_local_state` contains `consecutive_failures INTEGER NOT NULL DEFAULT 0` and `auto_paused INTEGER NOT NULL DEFAULT 0`, all added in the initial migration. The `consecutive_failures`/`auto_paused` pair backs the 3-strikes auto-pause (E-07 reads `consecutive_failures`; E-08 writes it and sets `auto_paused`; ratified 2026-06-19). Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (the authoritative full DDL) and decision ledger ("`scoped_bookmark_blob` column", "3-strikes auto-pause"). Verified by `db::tests::ratified_columns_present`.
- [x] AC3: `activity_records` has indexes `idx_activity_repo_time ON (repo_id, timestamp DESC)` and `idx_activity_time ON (timestamp DESC)`, and `settings` enforces a singleton via `CHECK (id = 1)` with `activity_retention_d` default 90. Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (the authoritative full DDL). Verified by `db::tests::activity_indexes_present`, `settings_singleton_rejects_second_row`, `settings_defaults_match_schema`.
- [x] AC4: Migrations are embedded via `sqlx::migrate!` and applied at startup against a WAL-mode single `SqlitePool`. Source: `docs/internal/strategy-and-roadmap.md` Section 4.2 (the authoritative full DDL, migration strategy) and brief Section 4 (System overview). Verified by `db::tests::journal_mode_is_wal` + `init_pool_with_recovery_clean_start_does_not_recover`; wired into `src-tauri` startup.
- [x] AC5: `paths.rs` is the only module that computes a path; it returns `%LOCALAPPDATA%\RepoSync` on Windows and `~/Library/Application Support/RepoSync` on macOS. Source: brief Section 4.10b and Section 4.2 (path seam). Verified by `paths::tests` (Windows branch asserted on Windows; macOS branch verified by code review only - no Mac access).
- [x] AC6: On a OneDrive-rooted data dir, the app logs a warning and prefers a non-synced location; the DB is never placed under Documents or Desktop. Source: brief Section 4.10c. Detection verified by `paths::tests::onedrive_detection_*`; the startup warning is wired in `src-tauri` (logic tested, the emitted warning is code-review-only).
- [x] AC7: A migration failure logs, moves the DB to `corrupt-backups/reposync-<timestamp>.db`, creates a fresh DB, re-runs the migrations on the fresh DB, and surfaces a one-time notice instead of crashing. Source: brief Section 4.10b (migration-failure recovery). Verified by `db::tests::init_pool_with_recovery_moves_corrupt_db_aside` + `move_db_aside_relocates_db_and_sidecars`.
- [x] AC8: The post-V1 migration policy is documented as additive-only (new columns with defaults, new tables; never destructive renames or drops). Source: brief Section 4.5 ("additive-only"). Documented in `crates/reposync-core/migrations/README.md`.
- [x] AC9: `repo_remote_meta` includes a nullable `etag TEXT` column for conditional GitHub requests, added in the initial migration. Source: reconciliation with E-10 (GitHub client) ETag caching (the brief does not mandate `etag`). Verified by `db::tests::ratified_columns_present`.

## Dependencies

- Upstream: E-01 (workspace, `reposync-core` crate, empty `migrations/`, `paths.rs` stub).
- Downstream: E-08 (scheduler reads/writes `repo_local_state`), E-09 (activity writer + retention against `activity_records` and `settings`), E-10 (GitHub client writes `repo_remote_meta`), E-12 (tracer bullet writes through this schema).

## V1.1 extension points

- `scoped_bookmark_blob` becomes live when macOS App Store sandboxing is pursued; the column already exists, so no destructive migration is needed.
- New tables and additive columns for V1.1 features (groups/tags surfacing, saved filters) extend the schema under the additive-only policy.
- A Linux data-dir branch in `paths.rs` (XDG dirs) if a Linux WebKitGTK canary is ever added (brief Section 4.2).

## Open questions

- sqlx macro posture: RESOLVED to the runtime query API (no compile-time `DATABASE_URL`, no `.sqlx` offline cache, no codegen CI check). Every query in `db.rs` / `store.rs` / `repo.rs` uses `sqlx::query(...).bind(...)` with `try_get` row reads; the only macro is `sqlx::migrate!` (which needs the `macros` feature but does not require `DATABASE_URL`). This matches the agent default flagged in the brief and the dependency hygiene comment already in `Cargo.toml`. If jp later prefers compile-time-checked queries, the migration is mechanical (swap `query` for `query!`, commit the `.sqlx` cache, add a CI check) but is NOT planned for V1. Source: brief decision ledger ("sqlx macro posture").
