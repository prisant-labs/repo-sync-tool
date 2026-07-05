//! git::discover - owned by E-03 (git executable discovery + version probing).
//!
//! Two pure, host-independent pieces plus a thin discovery driver:
//!
//!   - [`parse_git_version`]: turn `git --version` stdout into a [`GitVersion`].
//!   - [`GitVersion::meets_floor`]: compare against the >= 2.30 floor (AC7).
//!   - [`resolve_from_candidates`]: pick the first existing candidate from an
//!     ordered list (AC6), keeping the ordering logic testable with INJECTED
//!     paths rather than the host's real `git`.
//!
//! The discovery ORDER (AC6) is: explicit `settings.git_executable_path`, then
//! `PATH`, then well-known Windows install locations. [`candidate_paths`] builds
//! that ordered list; [`resolve_from_candidates`] applies an injectable
//! existence predicate so the ordering is unit-testable off-host.

use std::path::PathBuf;

/// The minimum supported git version (AC7): `status --porcelain=v2`,
/// `rev-list --left-right --count`, and modern `for-each-ref` all behave from
/// 2.30 onward.
pub const MIN_GIT_VERSION: GitVersion = GitVersion {
    major: 2,
    minor: 30,
    patch: 0,
};

/// A parsed `major.minor.patch` git version. Only the three leading numeric
/// components are modeled; platform suffixes (`.windows.1`, `.1`) are ignored
/// for the floor comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl GitVersion {
    /// Whether this version is at or above the >= 2.30 floor. Ordering is the
    /// natural `(major, minor, patch)` tuple comparison.
    pub fn meets_floor(&self) -> bool {
        (self.major, self.minor, self.patch)
            >= (
                MIN_GIT_VERSION.major,
                MIN_GIT_VERSION.minor,
                MIN_GIT_VERSION.patch,
            )
    }
}

impl std::fmt::Display for GitVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Parse the stdout of `git --version` into a [`GitVersion`].
///
/// Accepts the canonical `git version 2.40.1.windows.1` form and tolerates
/// extra leading/trailing whitespace. Returns `None` if no `git version`
/// prefix is present or the numeric triple cannot be read. Missing patch
/// defaults to 0 (git occasionally prints `2.30` with no patch).
pub fn parse_git_version(stdout: &str) -> Option<GitVersion> {
    // Find the token after the literal "git version".
    let trimmed = stdout.trim();
    let rest = trimmed.strip_prefix("git version ")?;
    let token = rest.split_whitespace().next()?;

    // Split on '.' and take the leading numeric components. The platform
    // suffix (e.g. "windows") is non-numeric, so parsing stops there.
    let mut parts = token.split('.');
    let major = parts.next()?.parse::<u32>().ok()?;
    let minor = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);
    let patch = parts
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    Some(GitVersion {
        major,
        minor,
        patch,
    })
}

/// Resolve the first candidate for which `exists` returns true, preserving the
/// candidate ORDER (AC6). The `exists` predicate is injected so ordering can be
/// tested without depending on the host's real filesystem or `git`.
pub fn resolve_from_candidates<F>(candidates: &[PathBuf], mut exists: F) -> Option<PathBuf>
where
    F: FnMut(&PathBuf) -> bool,
{
    candidates.iter().find(|c| exists(c)).cloned()
}

/// Build the ordered candidate list for the current host (AC6):
///
///   1. explicit `settings.git_executable_path` (when `Some`),
///   2. the bare name `git` (resolved against `PATH` at spawn time),
///   3. well-known Windows install locations (when on Windows).
///
/// The well-known list is a single table so the macOS port adds its entries in
/// one place. Environment lookups are passed in (not read here) so the function
/// stays pure and testable; [`candidate_paths_from_env`] is the host-reading
/// wrapper.
pub fn candidate_paths(
    explicit: Option<&str>,
    program_files: Option<&str>,
    local_app_data: Option<&str>,
    user_profile: Option<&str>,
    scoop: Option<&str>,
) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();

    // 1. Explicit override always wins.
    if let Some(p) = explicit {
        let p = p.trim();
        if !p.is_empty() {
            out.push(PathBuf::from(p));
        }
    }

    // 2. Bare "git" on PATH. Command::new("git") resolves this via PATH at
    //    spawn time; we represent it as the literal name so the resolver's
    //    "on PATH" probe can short-circuit to it.
    out.push(PathBuf::from("git"));

    // 3. Well-known Windows install locations. Populated only with the inputs
    //    the caller read from the environment; on non-Windows hosts these are
    //    typically None and contribute nothing.
    if let Some(pf) = program_files {
        let pf = PathBuf::from(pf);
        out.push(pf.join("Git").join("cmd").join("git.exe"));
        out.push(pf.join("Git").join("bin").join("git.exe"));
    }
    if let Some(lad) = local_app_data {
        // winget/Programs install layouts under LOCALAPPDATA.
        let lad = PathBuf::from(lad);
        out.push(
            lad.join("Microsoft")
                .join("WinGet")
                .join("Links")
                .join("git.exe"),
        );
        out.push(lad.join("Programs").join("Git").join("cmd").join("git.exe"));
    }
    // Scoop shims. The default Scoop root is %USERPROFILE%\scoop, and Scoop
    // exposes app entry points via a `shims` directory, so the git shim lives at
    // %USERPROFILE%\scoop\shims\git.exe. A relocated install sets %SCOOP% to the
    // chosen root, so when present its shims dir is also a candidate. (This is
    // the Scoop shim the LOCALAPPDATA comment used to claim but never added.)
    if let Some(up) = user_profile {
        out.push(
            PathBuf::from(up)
                .join("scoop")
                .join("shims")
                .join("git.exe"),
        );
    }
    if let Some(scoop_root) = scoop {
        out.push(PathBuf::from(scoop_root).join("shims").join("git.exe"));
    }

    out
}

/// Host-reading wrapper over [`candidate_paths`]: pulls `ProgramFiles`,
/// `LOCALAPPDATA`, `USERPROFILE`, and `SCOOP` from the environment and assembles
/// the ordered list.
pub fn candidate_paths_from_env(explicit: Option<&str>) -> Vec<PathBuf> {
    let pf = std::env::var("ProgramFiles").ok();
    let lad = std::env::var("LOCALAPPDATA").ok();
    let up = std::env::var("USERPROFILE").ok();
    let scoop = std::env::var("SCOOP").ok();
    candidate_paths(
        explicit,
        pf.as_deref(),
        lad.as_deref(),
        up.as_deref(),
        scoop.as_deref(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_windows_version() {
        let v = parse_git_version("git version 2.40.1.windows.1\n").expect("parse");
        assert_eq!(
            v,
            GitVersion {
                major: 2,
                minor: 40,
                patch: 1
            }
        );
    }

    #[test]
    fn parses_plain_unix_version() {
        let v = parse_git_version("git version 2.34.1\n").expect("parse");
        assert_eq!(
            v,
            GitVersion {
                major: 2,
                minor: 34,
                patch: 1
            }
        );
    }

    #[test]
    fn parses_version_without_patch() {
        let v = parse_git_version("git version 2.30").expect("parse");
        assert_eq!(
            v,
            GitVersion {
                major: 2,
                minor: 30,
                patch: 0
            }
        );
    }

    #[test]
    fn tolerates_surrounding_whitespace() {
        let v = parse_git_version("  git version 2.45.2  \n").expect("parse");
        assert_eq!(v.major, 2);
        assert_eq!(v.minor, 45);
        assert_eq!(v.patch, 2);
    }

    #[test]
    fn rejects_non_git_output() {
        assert!(parse_git_version("").is_none());
        assert!(parse_git_version("not a version").is_none());
        assert!(parse_git_version("git version notanumber").is_none());
    }

    #[test]
    fn floor_comparison() {
        // At the floor exactly.
        assert!(parse_git_version("git version 2.30.0")
            .unwrap()
            .meets_floor());
        // Above the floor.
        assert!(parse_git_version("git version 2.40.1.windows.1")
            .unwrap()
            .meets_floor());
        assert!(parse_git_version("git version 3.0.0")
            .unwrap()
            .meets_floor());
        // Below the floor by minor.
        assert!(!parse_git_version("git version 2.20.1")
            .unwrap()
            .meets_floor());
        // Below the floor by major.
        assert!(!parse_git_version("git version 1.9.5")
            .unwrap()
            .meets_floor());
        // 2.29.x is below; 2.30 with no patch is at the floor.
        assert!(!parse_git_version("git version 2.29.9")
            .unwrap()
            .meets_floor());
    }

    #[test]
    fn resolver_honors_order_explicit_first() {
        // Explicit override exists -> chosen even though "git" also "exists".
        let candidates = candidate_paths(
            Some("C:/custom/git.exe"),
            Some("C:/Program Files"),
            None,
            None,
            None,
        );
        // Pretend everything exists; first (explicit) must win.
        let chosen = resolve_from_candidates(&candidates, |_| true);
        assert_eq!(chosen, Some(PathBuf::from("C:/custom/git.exe")));
    }

    #[test]
    fn resolver_falls_through_to_path_then_wellknown() {
        let candidates = candidate_paths(None, Some("C:/Program Files"), None, None, None);
        // "git" (PATH) does not exist; the well-known cmd path does.
        let wellknown = PathBuf::from("C:/Program Files")
            .join("Git")
            .join("cmd")
            .join("git.exe");
        let wk = wellknown.clone();
        let chosen = resolve_from_candidates(&candidates, move |c| *c == wk);
        assert_eq!(chosen, Some(wellknown));
    }

    #[test]
    fn resolver_prefers_path_over_wellknown() {
        let candidates = candidate_paths(None, Some("C:/Program Files"), None, None, None);
        // Both "git" (PATH) and the well-known path "exist"; PATH wins (it is
        // earlier in the order).
        let chosen = resolve_from_candidates(&candidates, |_| true);
        assert_eq!(chosen, Some(PathBuf::from("git")));
    }

    #[test]
    fn resolver_none_when_nothing_exists() {
        let candidates = candidate_paths(None, Some("C:/Program Files"), None, None, None);
        let chosen = resolve_from_candidates(&candidates, |_| false);
        assert_eq!(chosen, None);
    }

    #[test]
    fn explicit_override_is_first_candidate() {
        let candidates = candidate_paths(Some("D:/git/git.exe"), None, None, None, None);
        assert_eq!(candidates[0], PathBuf::from("D:/git/git.exe"));
        // "git" (PATH) is always present as a fallback.
        assert!(candidates.iter().any(|c| c == &PathBuf::from("git")));
    }

    #[test]
    fn blank_explicit_is_ignored() {
        let candidates = candidate_paths(Some("   "), None, None, None, None);
        // First candidate is the PATH "git", not a blank path.
        assert_eq!(candidates[0], PathBuf::from("git"));
    }

    #[test]
    fn scoop_user_profile_shim_is_a_candidate() {
        // L2: the Scoop shim under %USERPROFILE%\scoop\shims\git.exe must be in
        // the ordered list. Before the fix the comment claimed a Scoop shim but
        // no Scoop path was ever added.
        let candidates = candidate_paths(None, None, None, Some("C:/Users/jp"), None);
        let scoop_shim = PathBuf::from("C:/Users/jp")
            .join("scoop")
            .join("shims")
            .join("git.exe");
        assert!(
            candidates.contains(&scoop_shim),
            "the %USERPROFILE%\\scoop\\shims\\git.exe candidate must be present: {candidates:?}"
        );
    }

    #[test]
    fn scoop_env_shim_is_a_candidate_when_set() {
        // L2: when %SCOOP% is set (a relocated Scoop root), its shims dir is also
        // a candidate.
        let candidates =
            candidate_paths(None, None, None, Some("C:/Users/jp"), Some("D:/scoop-root"));
        let scoop_env_shim = PathBuf::from("D:/scoop-root").join("shims").join("git.exe");
        assert!(
            candidates.contains(&scoop_env_shim),
            "the %SCOOP%\\shims\\git.exe candidate must be present when SCOOP is set: {candidates:?}"
        );
    }

    #[test]
    fn scoop_candidates_preserve_existing_order() {
        // L2: adding Scoop must not reorder the existing candidates: explicit is
        // still first, then PATH "git", then the ProgramFiles entries.
        let candidates = candidate_paths(
            Some("C:/custom/git.exe"),
            Some("C:/Program Files"),
            Some("C:/Users/jp/AppData/Local"),
            Some("C:/Users/jp"),
            Some("D:/scoop-root"),
        );
        assert_eq!(candidates[0], PathBuf::from("C:/custom/git.exe"));
        assert_eq!(candidates[1], PathBuf::from("git"));
        assert_eq!(
            candidates[2],
            PathBuf::from("C:/Program Files")
                .join("Git")
                .join("cmd")
                .join("git.exe")
        );
        assert_eq!(
            candidates[3],
            PathBuf::from("C:/Program Files")
                .join("Git")
                .join("bin")
                .join("git.exe")
        );
    }
}
