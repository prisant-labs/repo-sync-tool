//! paths - owned by E-02 (platform path resolution: the path seam).
//!
//! The ONLY module in the codebase that computes a data, db, or log path. Every
//! other module asks this one. Keeping path policy in a single seam is what makes
//! the macOS port a thin edge and the OneDrive-avoidance rule enforceable in one
//! place.
//!
//! Layout:
//!   * Windows: `%LOCALAPPDATA%\RepoSync`
//!   * macOS:   `~/Library/Application Support/RepoSync`
//!   * other:   `$XDG_DATA_HOME/RepoSync` else `~/.local/share/RepoSync`
//!
//! Under the data dir: `reposync.db` (the database), `logs/` (rotating logs), and
//! `corrupt-backups/` (where a DB is moved aside on migration failure).
//!
//! OneDrive avoidance (AC6): a SQLite file in WAL mode corrupts when a cloud sync
//! agent snapshots its `-wal`/`-shm` sidecars mid-write. `%LOCALAPPDATA%` is
//! already outside the synced tree, so the structural defense is the base-dir
//! choice; [`is_onedrive_rooted`] is the backstop that detects a misconfigured
//! environment and lets the caller warn. We never resolve to Documents or
//! Desktop, which ARE commonly synced.

use std::path::{Path, PathBuf};

/// The fixed app folder name under the per-user data root.
const APP_DIR: &str = "RepoSync";

/// Per-user data directory for RepoSync, created on first call.
///
/// Reads the host environment via [`AppPaths::from_env`] and returns the resolved
/// data dir, creating it (best effort) if absent.
pub fn data_dir() -> PathBuf {
    let paths = AppPaths::from_env();
    let dir = paths.data_dir().to_path_buf();
    // Best-effort create; a caller that truly needs the dir surfaces a clear
    // error downstream (e.g. open_pool failing to create the db file).
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Path to the SQLite database file inside [`data_dir`].
pub fn db_path() -> PathBuf {
    data_dir().join("reposync.db")
}

/// Resolved RepoSync paths for a given data root.
///
/// Construction is pure given the inputs ([`AppPaths::new`]); the host-reading
/// wrapper [`AppPaths::from_env`] pulls the per-OS base directory from the
/// environment. Tests build instances directly to stay deterministic.
#[derive(Debug, Clone)]
pub struct AppPaths {
    data_dir: PathBuf,
}

impl AppPaths {
    /// Build from an explicit data root (the `RepoSync` folder itself). Pure: no
    /// environment access, so tests inject a tempdir.
    pub fn new(data_dir: PathBuf) -> AppPaths {
        AppPaths { data_dir }
    }

    /// Build from the host environment, resolving the per-OS data root and
    /// appending the `RepoSync` app folder.
    pub fn from_env() -> AppPaths {
        AppPaths::new(resolve_data_dir())
    }

    /// The data directory (the `RepoSync` folder).
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// The SQLite database file.
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("reposync.db")
    }

    /// The log directory (`logs/` under the data dir).
    pub fn log_dir(&self) -> PathBuf {
        self.data_dir.join("logs")
    }

    /// The corrupt-backups directory (`corrupt-backups/` under the data dir),
    /// where a database is moved aside on migration failure (AC7).
    pub fn corrupt_backups_dir(&self) -> PathBuf {
        self.data_dir.join("corrupt-backups")
    }

    /// Create the data and log directories (best effort is not enough here: a
    /// hard failure to create the data dir is surfaced to the caller).
    pub fn ensure_dirs(&self) -> std::io::Result<()> {
        std::fs::create_dir_all(&self.data_dir)?;
        std::fs::create_dir_all(self.log_dir())?;
        Ok(())
    }

    /// Whether the resolved data dir falls under a known OneDrive root (AC6).
    ///
    /// Reads the `OneDrive` / `OneDriveConsumer` / `OneDriveCommercial` env vars
    /// (set by the OneDrive client) and checks the data dir against them.
    pub fn is_onedrive_rooted(&self) -> bool {
        let roots = onedrive_roots_from_env();
        path_is_under_any(&self.data_dir, &roots)
    }
}

/// Read the OneDrive root paths from the environment.
fn onedrive_roots_from_env() -> Vec<PathBuf> {
    ["OneDrive", "OneDriveConsumer", "OneDriveCommercial"]
        .iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .collect()
}

/// Whether `path` is equal to or nested under any of `roots`. Comparison is a
/// case-insensitive, separator-normalized prefix match so `C:\Users\x\OneDrive`
/// matches `C:/Users/X/OneDrive/RepoSync`.
pub fn path_is_under_any(path: &Path, roots: &[PathBuf]) -> bool {
    let norm = normalize_for_compare(path);
    roots.iter().any(|root| {
        let root = normalize_for_compare(root);
        if root.is_empty() {
            return false;
        }
        // Prefix on a path-segment boundary: either an exact match or the root
        // followed by a separator, so "OneDriveBackup" does not match "OneDrive".
        norm == root || norm.starts_with(&format!("{root}/"))
    })
}

/// Lower-case and forward-slash-normalize a path for prefix comparison. Windows
/// paths are case-insensitive and may mix `\` and `/`, so both are folded.
fn normalize_for_compare(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .trim_end_matches('/')
        .to_lowercase()
}

#[cfg(target_os = "windows")]
fn resolve_data_dir() -> PathBuf {
    // LOCALAPPDATA is set on all supported Windows versions and is OUTSIDE the
    // roaming/OneDrive-synced tree. Fall back to the current directory only if
    // the environment is unexpectedly empty so we never panic.
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(APP_DIR)
}

#[cfg(target_os = "macos")]
fn resolve_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Library")
        .join("Application Support")
        .join(APP_DIR)
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn resolve_data_dir() -> PathBuf {
    // Linux / other: respect XDG_DATA_HOME, else ~/.local/share. Not a V1 target
    // platform, but keeps the crate buildable and testable everywhere.
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join(APP_DIR);
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join(APP_DIR)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subpaths_hang_off_the_data_dir() {
        // The db, log dir, and corrupt-backups dir all live under the data dir
        // and carry the documented names. Pure (injected root), so this asserts
        // the layout on every OS without touching the host environment.
        let root = PathBuf::from("/tmp/some-root/RepoSync");
        let p = AppPaths::new(root.clone());
        assert_eq!(p.data_dir(), root.as_path());
        assert_eq!(p.db_path(), root.join("reposync.db"));
        assert_eq!(p.log_dir(), root.join("logs"));
        assert_eq!(p.corrupt_backups_dir(), root.join("corrupt-backups"));
    }

    #[test]
    fn app_dir_name_is_appended_per_os() {
        // The resolved per-OS data dir always ends in the RepoSync app folder.
        let p = AppPaths::from_env();
        assert_eq!(
            p.data_dir().file_name().and_then(|s| s.to_str()),
            Some(APP_DIR),
            "the resolved data dir must end in the RepoSync app folder"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_resolves_under_localappdata() {
        // The Windows branch must hang off %LOCALAPPDATA% (never Roaming). We do
        // not mutate the process env (other tests run concurrently); instead we
        // assert the resolved dir sits under the live LOCALAPPDATA when present.
        if let Some(lad) = std::env::var_os("LOCALAPPDATA") {
            let p = AppPaths::from_env();
            let lad = PathBuf::from(lad).join("RepoSync");
            assert_eq!(p.data_dir(), lad.as_path());
        }
    }

    #[test]
    fn onedrive_detection_matches_nested_path() {
        // A data dir nested under a OneDrive root is detected (AC6).
        let root = PathBuf::from("C:/Users/jp/OneDrive");
        let under = PathBuf::from("C:/Users/jp/OneDrive/RepoSync");
        assert!(path_is_under_any(&under, std::slice::from_ref(&root)));

        // The same path mixing separators and case still matches (Windows paths
        // are case-insensitive and may use either slash).
        let mixed = PathBuf::from(r"c:\users\jp\onedrive\reposync");
        assert!(path_is_under_any(&mixed, std::slice::from_ref(&root)));
    }

    #[test]
    fn onedrive_detection_rejects_sibling_and_unrelated() {
        let root = PathBuf::from("C:/Users/jp/OneDrive");
        // A sibling folder that merely shares a prefix string is NOT under the
        // root (segment-boundary match, not a raw string prefix).
        let sibling = PathBuf::from("C:/Users/jp/OneDriveBackup/RepoSync");
        assert!(!path_is_under_any(&sibling, std::slice::from_ref(&root)));
        // A genuinely unrelated, non-synced location is not flagged.
        let local = PathBuf::from("C:/Users/jp/AppData/Local/RepoSync");
        assert!(!path_is_under_any(&local, std::slice::from_ref(&root)));
    }

    #[test]
    fn onedrive_detection_handles_exact_root() {
        // An exact match (data dir == the root) counts as under it.
        let root = PathBuf::from("C:/OneDrive");
        let same = PathBuf::from("C:/OneDrive");
        assert!(path_is_under_any(&same, std::slice::from_ref(&root)));
    }

    #[test]
    fn empty_root_never_matches() {
        // An empty/blank OneDrive env value must not match every path.
        let roots = vec![PathBuf::from("")];
        assert!(!path_is_under_any(&PathBuf::from("C:/anything"), &roots));
    }

    #[test]
    fn ensure_dirs_creates_data_and_log_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let root = tmp.path().join("RepoSync");
        let p = AppPaths::new(root.clone());
        p.ensure_dirs().expect("ensure_dirs");
        assert!(root.is_dir(), "data dir created");
        assert!(p.log_dir().is_dir(), "log dir created");
    }
}
