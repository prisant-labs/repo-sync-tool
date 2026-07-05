//! db - owned by E-02 (SQLite pool + migrations).
//!
//! Week-1 tracer slice: open a WAL-mode SQLite pool with sane pragmas and run
//! the embedded migrations. Uses the sqlx RUNTIME API throughout (no
//! compile-time macros, no DATABASE_URL).

use std::path::{Path, PathBuf};
use std::time::Duration;
use std::time::{SystemTime, UNIX_EPOCH};

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous};
use sqlx::SqlitePool;

use crate::error::AppError;
use crate::paths::AppPaths;

/// Open (creating if missing) the SQLite pool for `db_path`.
///
/// Configures WAL journaling, a 5s busy timeout, NORMAL synchronous (safe under
/// WAL), and enforced foreign keys. Caps the pool at 5 connections.
pub async fn open_pool(db_path: &Path) -> Result<SqlitePool, AppError> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal)
        .busy_timeout(Duration::from_secs(5))
        .synchronous(SqliteSynchronous::Normal)
        .foreign_keys(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;

    Ok(pool)
}

/// Run the embedded migrations (`./migrations`) against `pool`.
///
/// The migration directory is embedded into the binary at compile time, so the
/// shipped app needs no external `.sql` files.
pub async fn run_migrations(pool: &SqlitePool) -> Result<(), AppError> {
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .map_err(|e| AppError::MigrationFailed {
            cause: e.to_string(),
        })?;
    Ok(())
}

/// A ready-to-use database handle plus the one-time migration-recovery notice.
///
/// `recovered` is `true` exactly when the initial migration failed and the old
/// database was moved aside and replaced with a fresh one (AC7). The shell reads
/// this once to surface a non-blocking notice; the data itself is in
/// `backup_path` for the user to recover manually.
#[derive(Debug)]
pub struct DbInit {
    /// The live, migrated pool.
    pub pool: SqlitePool,
    /// Whether migration-failure recovery ran (the one-time notice flag).
    pub recovered: bool,
    /// Where the corrupt database was moved, when `recovered` is true.
    pub backup_path: Option<PathBuf>,
}

/// Open the pool and apply migrations, recovering from a migration failure
/// instead of crashing (AC7).
///
/// Happy path: open the db at `paths.db_path()`, run migrations, return a
/// non-recovered [`DbInit`]. On a migration error: log it, CLOSE the pool (so the
/// `-wal`/`-shm` sidecars are released on Windows), move the database and its
/// sidecars to `corrupt-backups/reposync-<timestamp>.db`, create a FRESH database,
/// re-run migrations on it, and return a recovered [`DbInit`] carrying the backup
/// path and the notice flag. If the move fails (e.g. a locked file), we fall back
/// to a uniquely-named fresh database rather than crashing. Data is never silently
/// deleted.
pub async fn init_pool_with_recovery(paths: &AppPaths) -> Result<DbInit, AppError> {
    // The data + log dirs must exist before we touch the db file.
    paths.ensure_dirs().map_err(|e| AppError::Db {
        cause: format!("failed to create data directory: {e}"),
    })?;

    let db_path = paths.db_path();

    // Try the happy path: open the pool, then migrate. EITHER step can fail on a
    // corrupt/incompatible database (a non-SQLite file fails at connect with
    // "file is not a database"; a half-applied or checksum-mismatched migration
    // fails in the runner), and both are recoverable the same way: move the bad
    // file aside and start fresh. So we treat any error from this block uniformly.
    //
    // When the pool opened but migration failed, we hold the pool in
    // `opened_pool` so it can be CLOSED before the move: a held -wal/-shm keeps
    // the file locked on Windows, which would fail the rename.
    let mut opened_pool: Option<SqlitePool> = None;
    let result: Result<SqlitePool, AppError> = match open_pool(&db_path).await {
        Ok(pool) => match run_migrations(&pool).await {
            Ok(()) => Ok(pool),
            Err(e) => {
                opened_pool = Some(pool);
                Err(e)
            }
        },
        Err(e) => Err(e),
    };

    match result {
        Ok(pool) => Ok(DbInit {
            pool,
            recovered: false,
            backup_path: None,
        }),
        Err(err) => {
            eprintln!(
                "warning: database open/migration failed ({err}); moving the existing \
                 database aside and creating a fresh one"
            );

            // Release file handles before moving (Windows lock release).
            if let Some(pool) = opened_pool {
                pool.close().await;
            }

            let backup_dir = paths.corrupt_backups_dir();
            let backup_path = move_db_aside(&db_path, &backup_dir);
            match &backup_path {
                Some(moved) => eprintln!(
                    "info: the previous database was preserved at {}",
                    moved.display()
                ),
                None => eprintln!(
                    "warning: could not move the existing database aside (it may be \
                     locked); starting a fresh database under a unique name instead"
                ),
            }

            // If the move failed, the original file is still in place and likely
            // locked, so a fresh pool at the SAME path would re-hit the bad file.
            // Open the fresh database at a unique sibling path in that case.
            let fresh_path = if backup_path.is_some() {
                db_path.clone()
            } else {
                unique_fresh_db_path(&db_path)
            };

            let fresh_pool = open_pool(&fresh_path).await?;
            run_migrations(&fresh_pool).await?;

            Ok(DbInit {
                pool: fresh_pool,
                recovered: true,
                backup_path,
            })
        }
    }
}

/// Move a database file and its `-wal`/`-shm` sidecars into `backup_dir`, named
/// `reposync-<timestamp>.db` (sidecars keep their suffix). Returns the path the
/// `.db` was moved to, or `None` if the primary `.db` could not be moved (a
/// locked file on Windows, a permissions error). Best effort on the sidecars: a
/// failed sidecar move is logged but does not fail the whole operation.
fn move_db_aside(db_path: &Path, backup_dir: &Path) -> Option<PathBuf> {
    if std::fs::create_dir_all(backup_dir).is_err() {
        return None;
    }
    let stamp = timestamp();
    let dest = unique_backup_dest(backup_dir, &stamp);

    // Move the primary database first; if THIS fails, the whole move failed.
    if db_path.exists() {
        if let Err(e) = std::fs::rename(db_path, &dest) {
            eprintln!("warning: could not move {} aside: {e}", db_path.display());
            return None;
        }
    } else {
        // No primary file to move (e.g. an empty/absent db). Nothing to preserve,
        // but the caller can safely reuse the path, so report success-with-dest.
        return Some(dest);
    }

    // Move the sidecars best-effort, preserving their suffix next to the backup.
    for suffix in ["-wal", "-shm"] {
        let side = sidecar(db_path, suffix);
        if side.exists() {
            let side_dest = sidecar(&dest, suffix);
            if let Err(e) = std::fs::rename(&side, &side_dest) {
                eprintln!(
                    "warning: could not move sidecar {} aside: {e}",
                    side.display()
                );
            }
        }
    }

    Some(dest)
}

/// A non-colliding backup destination under `backup_dir` for stamp `stamp`.
///
/// The stamp is whole-seconds, so two recoveries in the same second would derive
/// the same `reposync-<stamp>.db` name and the second would overwrite (or, with
/// the locked-file fallback, be lost). The first candidate keeps the clean
/// `reposync-<stamp>.db` shape; on a collision we append an incrementing `-N`
/// suffix until a free path is found, so consecutive backups are always distinct.
fn unique_backup_dest(backup_dir: &Path, stamp: &str) -> PathBuf {
    let first = backup_dir.join(format!("reposync-{stamp}.db"));
    if !first.exists() {
        return first;
    }
    let mut n: u32 = 1;
    loop {
        let candidate = backup_dir.join(format!("reposync-{stamp}-{n}.db"));
        if !candidate.exists() {
            return candidate;
        }
        n += 1;
    }
}

/// Append a SQLite sidecar suffix (`-wal` / `-shm`) to a db path's file name.
fn sidecar(db_path: &Path, suffix: &str) -> PathBuf {
    let mut name = db_path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// A unique fresh-db path next to `db_path`, used when the corrupt original could
/// not be moved aside (so reusing its name would re-open the bad file).
fn unique_fresh_db_path(db_path: &Path) -> PathBuf {
    let stamp = timestamp();
    let stem = db_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("reposync");
    let parent = db_path.parent().unwrap_or_else(|| Path::new("."));
    parent.join(format!("{stem}-fresh-{stamp}.db"))
}

/// Current unix time in whole seconds, as a string suitable for a filename.
fn timestamp() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;
    use tempfile::TempDir;

    async fn fresh_pool() -> (TempDir, SqlitePool) {
        let dir = TempDir::new().expect("tempdir");
        let db = dir.path().join("test.db");
        let pool = open_pool(&db).await.expect("open_pool");
        run_migrations(&pool).await.expect("run_migrations");
        (dir, pool)
    }

    #[tokio::test]
    async fn migrations_create_all_v1_tables() {
        let (_dir, pool) = fresh_pool().await;
        // The full v1 table set (strategy-and-roadmap.md Section 4.2): the core
        // registry + state, the audit trail, the grouping pair, and settings.
        for table in [
            "repos",
            "repo_local_state",
            "repo_remote_meta",
            "activity_records",
            "groups",
            "repo_groups",
            "settings",
        ] {
            let row =
                sqlx::query("SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?")
                    .bind(table)
                    .fetch_optional(&pool)
                    .await
                    .expect("query sqlite_master");
            assert!(row.is_some(), "expected table {table} to exist");
        }
    }

    #[tokio::test]
    async fn ratified_columns_present() {
        let (_dir, pool) = fresh_pool().await;

        // AC2: repos.scoped_bookmark_blob.
        assert!(
            column_exists(&pool, "repos", "scoped_bookmark_blob").await,
            "repos.scoped_bookmark_blob missing"
        );
        // AC2: repo_local_state.consecutive_failures + auto_paused.
        assert!(
            column_exists(&pool, "repo_local_state", "consecutive_failures").await,
            "repo_local_state.consecutive_failures missing"
        );
        assert!(
            column_exists(&pool, "repo_local_state", "auto_paused").await,
            "repo_local_state.auto_paused missing"
        );
        // AC9: repo_remote_meta.etag.
        assert!(
            column_exists(&pool, "repo_remote_meta", "etag").await,
            "repo_remote_meta.etag missing"
        );
    }

    #[tokio::test]
    async fn repos_default_cadence_is_inherit_after_0004() {
        // BL-NI-34: migration 0004 changes the repos.check_frequency_min schema
        // DEFAULT from 360 to 0, so an INSERT that omits the column inherits the
        // global cadence instead of silently creating a 6-hour per-repo override.
        let (_dir, pool) = fresh_pool().await;
        sqlx::query(
            "INSERT INTO repos (local_name, local_path, created_at) VALUES ('x', 'C:/x', 0)",
        )
        .execute(&pool)
        .await
        .expect("insert repo relying on the column default");
        let freq: i64 =
            sqlx::query("SELECT check_frequency_min FROM repos WHERE local_path = 'C:/x'")
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("check_frequency_min")
                .unwrap();
        assert_eq!(
            freq, 0,
            "the schema default must be 0 (inherit), not 360 (a silent 6-hour override)"
        );
    }

    #[tokio::test]
    async fn repos_fk_cascade_survives_0004_rebuild() {
        // The 0004 table rebuild (create-copy-drop-rename with foreign keys off)
        // must preserve the inbound ON DELETE CASCADE foreign keys: deleting a repo
        // still clears its repo_local_state row.
        let (_dir, pool) = fresh_pool().await;
        let id = sqlx::query(
            "INSERT INTO repos (local_name, local_path, created_at) VALUES ('x', 'C:/x', 0)",
        )
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid();
        sqlx::query("INSERT INTO repo_local_state (repo_id) VALUES (?)")
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert child local-state row");

        sqlx::query("DELETE FROM repos WHERE id = ?")
            .bind(id)
            .execute(&pool)
            .await
            .expect("delete the repo");

        let remaining: i64 =
            sqlx::query("SELECT COUNT(*) AS c FROM repo_local_state WHERE repo_id = ?")
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap()
                .try_get("c")
                .unwrap();
        assert_eq!(
            remaining, 0,
            "ON DELETE CASCADE to repos must still fire after the 0004 table rebuild"
        );
    }

    #[tokio::test]
    async fn activity_indexes_present() {
        // AC3: the two activity_records query indexes.
        let (_dir, pool) = fresh_pool().await;
        for index in ["idx_activity_repo_time", "idx_activity_time"] {
            let row =
                sqlx::query("SELECT name FROM sqlite_master WHERE type = 'index' AND name = ?")
                    .bind(index)
                    .fetch_optional(&pool)
                    .await
                    .expect("query sqlite_master for index");
            assert!(row.is_some(), "expected index {index} to exist");
        }
    }

    #[tokio::test]
    async fn settings_singleton_rejects_second_row() {
        // AC3: the settings CHECK (id = 1) guard makes a second row impossible.
        let (_dir, pool) = fresh_pool().await;

        // The id = 1 row inserts fine.
        sqlx::query("INSERT INTO settings (id) VALUES (1)")
            .execute(&pool)
            .await
            .expect("the id = 1 settings row must insert");

        // Any other id is rejected by the CHECK constraint.
        let second = sqlx::query("INSERT INTO settings (id) VALUES (2)")
            .execute(&pool)
            .await;
        assert!(
            second.is_err(),
            "a settings row with id != 1 must be rejected by CHECK (id = 1)"
        );

        // And re-inserting id = 1 collides on the primary key, so the row is a
        // true singleton.
        let dup = sqlx::query("INSERT INTO settings (id) VALUES (1)")
            .execute(&pool)
            .await;
        assert!(
            dup.is_err(),
            "a second id = 1 settings row must collide on the primary key"
        );
    }

    #[tokio::test]
    async fn settings_defaults_match_schema() {
        // AC3: activity_retention_d defaults to 90; the other documented defaults
        // hold too. Insert the bare singleton and read the defaults back.
        let (_dir, pool) = fresh_pool().await;
        sqlx::query("INSERT INTO settings (id) VALUES (1)")
            .execute(&pool)
            .await
            .expect("insert settings singleton");

        let row = sqlx::query(
            "SELECT global_check_minutes, activity_retention_d, autostart, \
             notify_on_release, notify_on_failure, github_token_present \
             FROM settings WHERE id = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("read settings defaults");

        let retention: i64 = row.try_get("activity_retention_d").unwrap();
        let global: i64 = row.try_get("global_check_minutes").unwrap();
        assert_eq!(retention, 90, "activity_retention_d default must be 90");
        assert_eq!(global, 360, "global_check_minutes default must be 360");
    }

    #[tokio::test]
    async fn journal_mode_is_wal() {
        let (_dir, pool) = fresh_pool().await;
        let row = sqlx::query("PRAGMA journal_mode")
            .fetch_one(&pool)
            .await
            .expect("pragma journal_mode");
        let mode: String = row.try_get(0).expect("journal_mode column");
        assert_eq!(mode.to_lowercase(), "wal");
    }

    async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> bool {
        // PRAGMA table_info does not accept bind parameters for the table name,
        // so interpolate the (test-only, trusted) table identifier. sqlx 0.9
        // requires an explicit AssertSqlSafe wrapper for non-'static SQL.
        let sql = format!("PRAGMA table_info({table})");
        let rows = sqlx::query(sqlx::AssertSqlSafe(sql))
            .fetch_all(pool)
            .await
            .expect("table_info");
        rows.iter().any(|r| {
            let name: String = r.try_get("name").expect("name column");
            name == column
        })
    }

    #[tokio::test]
    async fn init_pool_with_recovery_clean_start_does_not_recover() {
        // Happy path: a fresh data dir migrates cleanly and reports no recovery.
        let tmp = TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().join("RepoSync"));

        let init = init_pool_with_recovery(&paths)
            .await
            .expect("clean init must succeed");
        assert!(!init.recovered, "a clean start must not flag recovery");
        assert!(init.backup_path.is_none(), "no backup on a clean start");

        // The pool is usable: the migrated tables exist.
        let row = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='repos'")
            .fetch_optional(&init.pool)
            .await
            .expect("query");
        assert!(row.is_some(), "the fresh db must be migrated");
    }

    #[tokio::test]
    async fn init_pool_with_recovery_moves_corrupt_db_aside() {
        // AC7: a corrupt database at the db path is moved into corrupt-backups/, a
        // fresh usable db replaces it, the notice flag is set, and nothing panics.
        let tmp = TempDir::new().expect("tempdir");
        let paths = AppPaths::new(tmp.path().join("RepoSync"));
        paths.ensure_dirs().expect("ensure dirs");

        // Seed a deliberately corrupt "database": a non-SQLite file at the db path.
        // open_pool() will open it, but the first migration query fails the magic
        // check, which is exactly the migration-failure path AC7 covers.
        let db_path = paths.db_path();
        std::fs::write(&db_path, b"this is definitely not a sqlite database\n")
            .expect("seed corrupt db");

        let init = init_pool_with_recovery(&paths)
            .await
            .expect("recovery must not return an error");

        // The notice flag is set and points at a real backup file.
        assert!(init.recovered, "recovery must set the one-time notice flag");
        let backup = init.backup_path.expect("a backup path must be recorded");
        assert!(backup.exists(), "the corrupt db must be preserved on disk");
        assert!(
            backup.starts_with(paths.corrupt_backups_dir()),
            "the backup must live under corrupt-backups/"
        );

        // The fresh database is usable and migrated.
        let row = sqlx::query("SELECT name FROM sqlite_master WHERE type='table' AND name='repos'")
            .fetch_optional(&init.pool)
            .await
            .expect("query the recovered db");
        assert!(
            row.is_some(),
            "the recovered db must be migrated and usable"
        );
    }

    #[test]
    fn move_db_aside_twice_produces_distinct_paths() {
        // L-2: two recoveries in the same whole second must not collide. The stamp
        // is whole-seconds, so without sub-second uniqueness the second move would
        // reuse the first dest path. Assert two consecutive backups land on
        // distinct, both-present files.
        let tmp = TempDir::new().expect("tempdir");
        let backup_dir = tmp.path().join("corrupt-backups");

        let db1 = tmp.path().join("reposync.db");
        std::fs::write(&db1, b"db one").unwrap();
        let first = move_db_aside(&db1, &backup_dir).expect("first move should succeed");

        // Recreate the db at the same source path and move it again immediately;
        // these two calls almost certainly share a whole-second timestamp.
        let db2 = tmp.path().join("reposync.db");
        std::fs::write(&db2, b"db two").unwrap();
        let second = move_db_aside(&db2, &backup_dir).expect("second move should succeed");

        assert_ne!(
            first, second,
            "two consecutive backups must not collide on the same path"
        );
        assert!(first.exists(), "the first backup must still be present");
        assert!(
            second.exists(),
            "the second backup must be present, not overwritten"
        );
    }

    #[test]
    fn move_db_aside_relocates_db_and_sidecars() {
        // The move helper relocates the .db and its -wal/-shm sidecars into the
        // backup dir, naming the primary reposync-<timestamp>.db.
        let tmp = TempDir::new().expect("tempdir");
        let db = tmp.path().join("reposync.db");
        std::fs::write(&db, b"db").unwrap();
        std::fs::write(sidecar(&db, "-wal"), b"wal").unwrap();
        std::fs::write(sidecar(&db, "-shm"), b"shm").unwrap();

        let backup_dir = tmp.path().join("corrupt-backups");
        let moved = move_db_aside(&db, &backup_dir).expect("move should succeed");

        assert!(moved.exists(), "the .db was moved into the backup dir");
        assert!(
            moved.starts_with(&backup_dir),
            "moved under corrupt-backups/"
        );
        assert!(
            !db.exists(),
            "the original .db no longer sits at the db path"
        );
        assert!(
            sidecar(&moved, "-wal").exists(),
            "the -wal sidecar moved alongside"
        );
        assert!(
            sidecar(&moved, "-shm").exists(),
            "the -shm sidecar moved alongside"
        );
    }
}
