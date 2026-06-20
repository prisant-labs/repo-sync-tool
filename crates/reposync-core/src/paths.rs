//! paths - owned by E-02 (platform path resolution: the path seam).
//!
//! Week-1 tracer slice: resolve the per-user data directory and the SQLite
//! database path, creating the directory on first use. OneDrive detection and
//! corrupt-backup handling are deferred to the full E-02 effort.

use std::path::PathBuf;

/// Per-user data directory for RepoSync, created on first call.
///
/// Windows: `%LOCALAPPDATA%\RepoSync`.
/// macOS: `~/Library/Application Support/RepoSync`.
pub fn data_dir() -> PathBuf {
    let dir = resolve_data_dir();
    // Best-effort create; callers that need the directory will surface a clear
    // error downstream if creation truly failed (E-02 hardens this).
    let _ = std::fs::create_dir_all(&dir);
    dir
}

/// Path to the SQLite database file inside [`data_dir`].
pub fn db_path() -> PathBuf {
    data_dir().join("reposync.db")
}

#[cfg(target_os = "windows")]
fn resolve_data_dir() -> PathBuf {
    // LOCALAPPDATA is set on all supported Windows versions. Fall back to the
    // current directory only if the environment is unexpectedly empty so we
    // never panic during the tracer.
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("RepoSync")
}

#[cfg(target_os = "macos")]
fn resolve_data_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join("Library")
        .join("Application Support")
        .join("RepoSync")
}

#[cfg(not(any(target_os = "windows", target_os = "macos")))]
fn resolve_data_dir() -> PathBuf {
    // Linux / other: respect XDG_DATA_HOME, else ~/.local/share. Not a V1
    // target platform, but keeps the crate buildable and testable everywhere.
    if let Some(xdg) = std::env::var_os("XDG_DATA_HOME") {
        return PathBuf::from(xdg).join("RepoSync");
    }
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    home.join(".local").join("share").join("RepoSync")
}
