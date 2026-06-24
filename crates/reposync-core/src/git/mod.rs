//! git - owned by E-03 (the git engine boundary).
//!
//! Splits into cheap reads (`inspect`, via git2) and network/mutation
//! operations (`cli`, by shelling out to git). The `GitEngine` trait is the
//! seam both sides implement; in the tracer the concrete [`SystemGitEngine`]
//! provides the behavior and the trait is a marker that E-03 will flesh out.

pub mod cli;
pub mod discover;
pub mod inspect;

/// E-04's git fixture test harness. Compiled for the crate's own tests and
/// under the `test-support` feature so downstream test trees (E-07 policy,
/// E-08 scheduler) and integration tests can import the same fabricated states.
#[cfg(any(test, feature = "test-support"))]
pub mod fixtures;

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

/// Classification of a `git fetch` outcome (AC10 / BL-NI-05).
///
/// This is an INTERNAL `git/` type, not part of the frozen IPC contract: the
/// update-policy engine (E-07) maps `AuthFailure` to pause and `NetworkFailure`
/// to retry, so the parser must distinguish them. `Unknown` is the conservative
/// fallback when the captured exit code + stderr match no known signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FetchClass {
    /// Exit 0 with evidence the remote tracking refs advanced.
    Success,
    /// Exit 0 but the remote had nothing new ("already up to date").
    NoOp,
    /// A credential/authentication failure (e.g. 403, "Authentication failed",
    /// "could not read Username"). E-07 maps this to pause, not retry.
    AuthFailure,
    /// A transport/connectivity failure (DNS, connection refused/timeout,
    /// "Could not resolve host"). E-07 maps this to retry.
    NetworkFailure,
    /// A non-zero exit that matches no known signature. The conservative
    /// fallback so the policy never silently mis-handles an unfamiliar failure.
    Unknown,
}

impl FetchClass {
    /// Whether this class represents a successful fetch (success or no-op).
    pub fn is_success(self) -> bool {
        matches!(self, FetchClass::Success | FetchClass::NoOp)
    }
}

/// Raw outcome of a `git fetch`, captured for the activity log.
///
/// `class` is the AC10 classification; `success` is retained as a convenience
/// derived from `class.is_success()` so existing callers (and `repo.rs`) keep a
/// simple boolean without re-deriving it.
#[derive(Debug, Clone)]
pub struct FetchOutcome {
    pub raw_command: String,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
    pub success: bool,
    pub class: FetchClass,
}

/// Ahead/behind commit counts relative to an upstream. `None` when unknown
/// (e.g. no upstream configured or the command failed).
#[derive(Debug, Clone, Copy)]
pub struct AheadBehind {
    pub ahead: Option<i64>,
    pub behind: Option<i64>,
}

use crate::git::discover::{
    candidate_paths_from_env, parse_git_version, GitVersion, MIN_GIT_VERSION,
};

/// The first-class availability state of the git engine (AC8).
///
/// This is the data a banner needs: the shell renders the missing/too-old case
/// without inspecting an error. It is distinct from a generic [`AppError`]:
/// "git not found" is a normal, recoverable state on Windows, not a failure of
/// an operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitAvailability {
    /// A git at or above the >= 2.30 floor was discovered.
    Available { version: GitVersion },
    /// A git was discovered but is below the >= 2.30 floor. Usable but flagged
    /// with a clear, non-blocking warning (AC7): operations are still attempted.
    BelowFloor { version: GitVersion },
    /// No usable git was found (not on PATH, not at any well-known location, or
    /// the explicit override did not resolve). The "git unavailable" state.
    Unavailable,
}

impl GitAvailability {
    /// Whether a git executable was resolved at all (floor aside). `false` only
    /// in the [`GitAvailability::Unavailable`] state.
    pub fn is_resolved(&self) -> bool {
        !matches!(self, GitAvailability::Unavailable)
    }

    /// Whether git is unavailable (the "git not found" first-class state).
    pub fn is_unavailable(&self) -> bool {
        matches!(self, GitAvailability::Unavailable)
    }

    /// Whether the resolved git is below the supported floor.
    pub fn is_below_floor(&self) -> bool {
        matches!(self, GitAvailability::BelowFloor { .. })
    }
}

/// The git engine seam (AC1, AC5).
///
/// Splits cheap reads (git2-backed in the system impl) from network/mutation
/// operations (CLI-backed). The split is the whole point: an all-CLI read impl
/// is a drop-in replacement behind this trait if `libgit2-sys` ever fights a
/// target toolchain, with no change above the trait.
///
/// The async network methods use native `async fn` in traits (stable since Rust
/// 1.75); the `async_fn_in_trait` lint about an unconstrained `Send` bound is
/// allowed deliberately. Callers in this crate (and the scheduler, E-08) drive
/// the system impl directly, whose futures are `Send`; the trait exists for the
/// read-path swap, not for dynamic dispatch.
#[allow(async_fn_in_trait)]
pub trait GitEngine {
    /// The current availability state (AC8).
    fn availability(&self) -> &GitAvailability;

    /// Cheap, synchronous, read-only inspection via the read backend.
    fn inspect(&self, repo_path: &Path) -> Result<InspectResult, AppError>;

    /// Ahead/behind of HEAD vs. its configured upstream via the read backend.
    /// `None`/`None` for the no-upstream and deleted-upstream states (AC11).
    fn ahead_behind_read(&self, repo_path: &Path) -> Result<AheadBehind, AppError>;

    /// Fetch all remotes (network) via the CLI backend.
    async fn fetch(&self, repo_path: &Path) -> Result<FetchOutcome, AppError>;

    /// Ahead/behind vs. `upstream` via the CLI backend (`rev-list`).
    async fn ahead_behind(&self, repo_path: &Path, upstream: &str)
        -> Result<AheadBehind, AppError>;

    /// Resolve a ref to a SHA via the CLI backend (`rev-parse`).
    async fn rev_parse(&self, repo_path: &Path, refname: &str) -> Result<Option<String>, AppError>;

    /// Working-tree dirtiness via the CLI backend (`status --porcelain=v2`).
    async fn status(&self, repo_path: &Path) -> Result<cli::PorcelainStatus, AppError>;

    /// Enumerate refs + upstreams via the CLI backend (`for-each-ref`).
    async fn for_each_ref(&self, repo_path: &Path) -> Result<Vec<cli::RefRow>, AppError>;
}

/// A git engine backed by git2 (reads) and the system git CLI (network).
///
/// Construction NEVER fails (AC9): [`SystemGitEngine::new`] always returns a
/// usable engine. When no git resolves, the engine lands in the
/// [`GitAvailability::Unavailable`] state and network ops return
/// [`AppError::GitNotFound`], but reads (git2) and app launch are unaffected.
#[derive(Debug, Clone)]
pub struct SystemGitEngine {
    /// The resolved git executable, or `None` in the unavailable state.
    git_exe: Option<PathBuf>,
    /// The explicit override candidate (from `settings.git_executable_path`),
    /// remembered so [`SystemGitEngine::reprobe`] re-honors it (AC8).
    explicit: Option<String>,
    availability: GitAvailability,
}

impl GitEngine for SystemGitEngine {
    fn availability(&self) -> &GitAvailability {
        &self.availability
    }

    fn inspect(&self, repo_path: &Path) -> Result<InspectResult, AppError> {
        inspect::inspect(repo_path)
    }

    fn ahead_behind_read(&self, repo_path: &Path) -> Result<AheadBehind, AppError> {
        inspect::ahead_behind(repo_path)
    }

    async fn fetch(&self, repo_path: &Path) -> Result<FetchOutcome, AppError> {
        let exe = self.require_exe()?;
        cli::fetch(exe, repo_path).await
    }

    async fn ahead_behind(
        &self,
        repo_path: &Path,
        upstream: &str,
    ) -> Result<AheadBehind, AppError> {
        let exe = self.require_exe()?;
        cli::ahead_behind(exe, repo_path, upstream).await
    }

    async fn rev_parse(&self, repo_path: &Path, refname: &str) -> Result<Option<String>, AppError> {
        let exe = self.require_exe()?;
        cli::rev_parse(exe, repo_path, refname).await
    }

    async fn status(&self, repo_path: &Path) -> Result<cli::PorcelainStatus, AppError> {
        let exe = self.require_exe()?;
        cli::status(exe, repo_path).await
    }

    async fn for_each_ref(&self, repo_path: &Path) -> Result<Vec<cli::RefRow>, AppError> {
        let exe = self.require_exe()?;
        cli::for_each_ref(exe, repo_path).await
    }
}

impl SystemGitEngine {
    /// Construct an engine, discovering git from the optional explicit override
    /// then PATH then well-known locations, and probing its version.
    ///
    /// Always succeeds (AC9): a missing git yields an engine in the
    /// [`GitAvailability::Unavailable`] state, never an `Err`. `explicit` is the
    /// `settings.git_executable_path` value (or `None`).
    pub fn new(explicit: Option<String>) -> SystemGitEngine {
        let (git_exe, availability) = resolve_and_probe(explicit.as_deref());
        SystemGitEngine {
            git_exe,
            explicit,
            availability,
        }
    }

    /// Back-compat constructor: discover with no explicit override. Returns
    /// `Err(GitNotFound)` ONLY when git is unavailable, for the call sites that
    /// still want the fail-fast shape (tests, the tracer `repo.rs` flows).
    ///
    /// Prefer [`SystemGitEngine::new`] for app construction, which never gates
    /// launch on git (AC9).
    pub fn discover() -> Result<SystemGitEngine, AppError> {
        let engine = SystemGitEngine::new(None);
        if engine.availability.is_unavailable() {
            Err(AppError::GitNotFound)
        } else {
            Ok(engine)
        }
    }

    /// Re-run discovery + version probing, honoring the same explicit override
    /// the engine was built with (AC8). On a later successful discovery this
    /// flips the state off [`GitAvailability::Unavailable`]; the caller (E-08
    /// scheduler) keys auto-resume off the returned state. Returns the new
    /// availability.
    pub fn reprobe(&mut self) -> GitAvailability {
        let (git_exe, availability) = resolve_and_probe(self.explicit.as_deref());
        self.git_exe = git_exe;
        self.availability = availability.clone();
        availability
    }

    /// The current availability state (AC8).
    pub fn availability(&self) -> &GitAvailability {
        &self.availability
    }

    /// The resolved git executable path, or `None` in the unavailable state.
    pub fn git_exe(&self) -> Option<&Path> {
        self.git_exe.as_deref()
    }

    /// Cheap, synchronous, read-only inspection via git2.
    pub fn inspect(&self, repo_path: &Path) -> Result<InspectResult, AppError> {
        inspect::inspect(repo_path)
    }

    /// Fetch all remotes (network) via the git CLI. Errors with
    /// [`AppError::GitNotFound`] in the unavailable state.
    pub async fn fetch(&self, repo_path: &Path) -> Result<FetchOutcome, AppError> {
        let exe = self.require_exe()?;
        cli::fetch(exe, repo_path).await
    }

    /// Compute ahead/behind vs. `upstream` via the git CLI. Errors with
    /// [`AppError::GitNotFound`] in the unavailable state.
    pub async fn ahead_behind(
        &self,
        repo_path: &Path,
        upstream: &str,
    ) -> Result<AheadBehind, AppError> {
        let exe = self.require_exe()?;
        cli::ahead_behind(exe, repo_path, upstream).await
    }

    /// The resolved exe, or [`AppError::GitNotFound`] when unavailable.
    fn require_exe(&self) -> Result<&Path, AppError> {
        self.git_exe.as_deref().ok_or(AppError::GitNotFound)
    }
}

/// Resolve a git executable (explicit -> PATH -> well-known) and probe its
/// version, returning the resolved path (if any) and the availability state.
/// Never panics; every failure path collapses to
/// `(None, GitAvailability::Unavailable)`.
fn resolve_and_probe(explicit: Option<&str>) -> (Option<PathBuf>, GitAvailability) {
    let candidates = candidate_paths_from_env(explicit);
    let Some(exe) = discover::resolve_from_candidates(&candidates, |c| candidate_is_runnable(c))
    else {
        return (None, GitAvailability::Unavailable);
    };

    // Probe the version. A spawn that produces unparseable output still counts
    // as resolved; we just cannot assert the floor, so treat it as Available
    // (conservative: do not block on a version we could not read).
    let availability = match probe_version(&exe) {
        Some(version) if version.meets_floor() => GitAvailability::Available { version },
        Some(version) => GitAvailability::BelowFloor { version },
        None => GitAvailability::Available {
            version: MIN_GIT_VERSION,
        },
    };
    (Some(exe), availability)
}

/// Whether a candidate path is a runnable git: either the bare name `git`
/// (resolved against PATH by actually spawning `git --version`) or an existing
/// file on disk. The bare-name probe is what lets PATH resolution work without
/// a separate `where`/`which`.
fn candidate_is_runnable(candidate: &Path) -> bool {
    if candidate == Path::new("git") {
        return spawn_version_ok(candidate);
    }
    candidate.is_file()
}

/// Whether `<exe> --version` spawns and exits zero (synchronous probe).
fn spawn_version_ok(exe: &Path) -> bool {
    std::process::Command::new(exe)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Run `<exe> --version` and parse the version. `None` if the spawn fails or
/// the output does not parse.
fn probe_version(exe: &Path) -> Option<GitVersion> {
    let output = std::process::Command::new(exe)
        .arg("--version")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_git_version(&stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn availability_predicates() {
        let avail = GitAvailability::Available {
            version: GitVersion {
                major: 2,
                minor: 40,
                patch: 0,
            },
        };
        assert!(avail.is_resolved());
        assert!(!avail.is_unavailable());
        assert!(!avail.is_below_floor());

        let below = GitAvailability::BelowFloor {
            version: GitVersion {
                major: 2,
                minor: 20,
                patch: 0,
            },
        };
        assert!(below.is_resolved());
        assert!(!below.is_unavailable());
        assert!(below.is_below_floor());

        let gone = GitAvailability::Unavailable;
        assert!(!gone.is_resolved());
        assert!(gone.is_unavailable());
        assert!(!gone.is_below_floor());
    }

    #[test]
    fn new_never_errs_and_returns_usable_engine() {
        // AC9: construction returns an engine (the type system already forbids
        // an Err here - new() is infallible by signature). A bogus explicit
        // override must not change that; the engine is still usable for reads.
        let engine = SystemGitEngine::new(Some("Z:/definitely/missing/git.exe".to_string()));

        // Reads work regardless of git availability (they go through git2).
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(matches!(
            engine.inspect(tmp.path()),
            Err(AppError::NotARepo { .. })
        ));
        // The availability is one of the three known states.
        assert!(matches!(
            engine.availability(),
            GitAvailability::Available { .. }
                | GitAvailability::BelowFloor { .. }
                | GitAvailability::Unavailable
        ));
    }

    #[tokio::test]
    async fn unavailable_engine_reads_ok_but_network_errs() {
        // Construct a guaranteed-unavailable engine by hand (no exe).
        let engine = SystemGitEngine {
            git_exe: None,
            explicit: None,
            availability: GitAvailability::Unavailable,
        };
        assert!(engine.availability().is_unavailable());

        // A network op returns GitNotFound rather than panicking.
        let tmp = tempfile::TempDir::new().unwrap();
        let err = engine.fetch(tmp.path()).await.expect_err("should error");
        assert!(matches!(err, AppError::GitNotFound));

        // Reads do not depend on the CLI: inspecting a non-repo still gives the
        // normal NotARepo error, proving the read path is independent of git
        // availability.
        let read = engine.inspect(tmp.path());
        assert!(matches!(read, Err(AppError::NotARepo { .. })));
    }

    #[test]
    fn reprobe_flips_off_unavailable_when_git_appears() {
        // Start unavailable (no exe, explicit override that cannot resolve).
        let mut engine = SystemGitEngine {
            git_exe: None,
            explicit: None,
            availability: GitAvailability::Unavailable,
        };
        assert!(engine.availability().is_unavailable());

        // Re-probe with the real environment. If the host has git (CI does, via
        // PATH), the state flips to resolved; if not, it stays unavailable. We
        // assert the re-probe RAN and returned a consistent state, and that when
        // git is present it is no longer unavailable.
        let state = engine.reprobe();
        assert_eq!(&state, engine.availability());
        if super::spawn_version_ok(Path::new("git")) {
            assert!(
                engine.availability().is_resolved(),
                "git is on PATH, so re-probe must resolve it"
            );
        }
    }

    #[test]
    fn floor_state_is_below_floor_for_old_git() {
        // resolve_and_probe maps a below-floor version to BelowFloor. We cannot
        // install an old git here, so assert the mapping via the helper inputs:
        // a parsed old version does not meet the floor.
        let old = parse_git_version("git version 2.20.1").unwrap();
        assert!(!old.meets_floor());
        let new = parse_git_version("git version 2.40.1").unwrap();
        assert!(new.meets_floor());
    }
}
