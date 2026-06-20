//! db - owned by E-02 (SQLite pool + migrations).
//!
//! Week-1 tracer slice: open a WAL-mode SQLite pool with sane pragmas and run
//! the embedded migrations. Uses the sqlx RUNTIME API throughout (no
//! compile-time macros, no DATABASE_URL).

use std::path::Path;
use std::time::Duration;

use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous,
};
use sqlx::SqlitePool;

use crate::error::AppError;

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
        .map_err(|e| AppError::Db {
            cause: e.to_string(),
        })?;
    Ok(())
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
    async fn migrations_create_three_tables() {
        let (_dir, pool) = fresh_pool().await;
        for table in ["repos", "repo_local_state", "activity_records"] {
            let row = sqlx::query(
                "SELECT name FROM sqlite_master WHERE type = 'table' AND name = ?",
            )
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

        // repos.scoped_bookmark_blob
        assert!(
            column_exists(&pool, "repos", "scoped_bookmark_blob").await,
            "repos.scoped_bookmark_blob missing"
        );
        // repo_local_state.consecutive_failures + auto_paused
        assert!(
            column_exists(&pool, "repo_local_state", "consecutive_failures").await,
            "repo_local_state.consecutive_failures missing"
        );
        assert!(
            column_exists(&pool, "repo_local_state", "auto_paused").await,
            "repo_local_state.auto_paused missing"
        );
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
}
