//! git - owned by E-03 (the git engine boundary).
//!
//! Splits into cheap reads (`inspect`, via git2) and network/mutation
//! operations (`cli`, by shelling out to git). The `GitEngine` trait is the
//! seam both sides implement; in the tracer the concrete [`SystemGitEngine`]
//! provides the behavior and the trait is a marker that E-03 will flesh out.

pub mod cli;
pub mod inspect;

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Result of a cheap, read-only repository inspection (git2).
#[derive(Debug, Clone)]
pub struct InspectResult {
    pub head_sha: Option<String>,
    pub active_branch: Option<String>,
    pub is_dirty: bool,
    pub is_detached: bool,
    pub upstream_branch: Option<String>,
}

/// Raw outcome of a `git fetch`, captured for the activity log.
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub raw_command: String,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    pub success: bool,
}

/// Ahead/behind commit counts relative to an upstream. `None` when unknown
/// (e.g. no upstream configured or the command failed).
#[derive(Debug, Clone, Copy)]
pub struct AheadBehind {
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
}

/// Marker trait for the git engine seam. E-03 fleshes this out into the full
/// async trait; the tracer ships only the concrete [`SystemGitEngine`].
pub trait GitEngine {}

/// A git engine backed by git2 (reads) and the system git CLI (network).
#[derive(Debug, Clone)]
pub struct SystemGitEngine {
    git_exe: PathBuf,
}

impl GitEngine for SystemGitEngine {}

impl SystemGitEngine {
    /// Discover the git executable: prefer one on PATH, then the default
    /// Windows install location. Returns [`AppError::GitNotFound`] if neither
    /// resolves.
    pub fn discover() -> Result<SystemGitEngine, AppError> {
        if let Some(exe) = discover_git_exe() {
            Ok(SystemGitEngine { git_exe: exe })
        } else {
            Err(AppError::GitNotFound)
        }
    }

    /// The resolved git executable path.
    pub fn git_exe(&self) -> &Path {
        &self.git_exe
    }

    /// Cheap, synchronous, read-only inspection via git2.
    pub fn inspect(&self, repo_path: &Path) -> Result<InspectResult, AppError> {
        inspect::inspect(repo_path)
    }

    /// Fetch all remotes (network) via the git CLI.
    pub async fn fetch(&self, repo_path: &Path) -> Result<FetchOutcome, AppError> {
        cli::fetch(&self.git_exe, repo_path).await
    }

    /// Compute ahead/behind vs. `upstream` via the git CLI.
    pub async fn ahead_behind(
        &self,
        repo_path: &Path,
        upstream: &str,
    ) -> Result<AheadBehind, AppError> {
        cli::ahead_behind(&self.git_exe, repo_path, upstream).await
    }
}

/// Locate the git executable. Checks PATH first (via `where`/`which` semantics
/// by probing common names), then the default Windows install location.
fn discover_git_exe() -> Option<PathBuf> {
    // 1. On PATH: try to run `git --version`. If it spawns, "git" is resolvable
    //    and Command::new("git") will find it via PATH at call time.
    if git_on_path() {
        return Some(PathBuf::from("git"));
    }

    // 2. Default Windows install location: %ProgramFiles%\Git\cmd\git.exe.
    #[cfg(target_os = "windows")]
    {
        if let Some(pf) = std::env::var_os("ProgramFiles") {
            let candidate = PathBuf::from(pf).join("Git").join("cmd").join("git.exe");
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    None
}

/// Whether `git` is invokable from PATH (synchronous probe).
fn git_on_path() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
