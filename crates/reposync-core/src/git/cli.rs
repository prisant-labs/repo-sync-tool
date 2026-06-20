//! git::cli - owned by E-03 (network + mutation: shell out to git).
//!
//! All network and mutating git goes through the system git CLI (never git2's
//! network transports), capturing the full raw command, stdout, stderr, exit
//! code, and duration for the activity log.

use std::path::Path;
use std::time::Instant;

use tokio::process::Command;

use crate::error::AppError;
use crate::git::{AheadBehind, FetchOutcome};

/// Raw capture of a single git CLI invocation.
pub(crate) struct Captured {
    pub raw_command: String,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
}

/// Run `git -C <repo_path> <args...>`, capturing output, exit code, and wall
/// time. A spawn failure (e.g. git missing) maps to [`AppError::GitNotFound`].
pub(crate) async fn run_git(
    git_exe: &Path,
    repo_path: &Path,
    args: &[&str],
) -> Result<Captured, AppError> {
    let mut cmd = Command::new(git_exe);
    cmd.arg("-C").arg(repo_path);
    for a in args {
        cmd.arg(a);
    }

    let pretty_args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let raw_command = format!(
        "{} -C {} {}",
        git_exe.display(),
        repo_path.display(),
        pretty_args.join(" ")
    );

    let started = Instant::now();
    let output = cmd
        .output()
        .await
        .map_err(|_| AppError::GitNotFound)?;
    let duration_ms = started.elapsed().as_millis() as i64;

    Ok(Captured {
        raw_command,
        raw_stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        raw_stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        exit_code: output.status.code(),
        duration_ms,
    })
}

/// Fetch all remotes and prune. On a non-zero exit the outcome is still
/// returned (with `success = false`); the caller decides how to surface it.
pub(crate) async fn fetch(git_exe: &Path, repo_path: &Path) -> Result<FetchOutcome, AppError> {
    let captured = run_git(git_exe, repo_path, &["fetch", "--all", "--prune"]).await?;
    let success = captured.exit_code == Some(0);
    Ok(FetchOutcome {
        raw_command: captured.raw_command,
        raw_stdout: captured.raw_stdout,
        raw_stderr: captured.raw_stderr,
        exit_code: captured.exit_code,
        duration_ms: captured.duration_ms,
        success,
    })
}

/// Compute ahead/behind counts between HEAD and `upstream` via
/// `git rev-list --left-right --count HEAD...<upstream>`. On any failure or
/// missing upstream returns `AheadBehind { ahead: None, behind: None }`.
pub(crate) async fn ahead_behind(
    git_exe: &Path,
    repo_path: &Path,
    upstream: &str,
) -> Result<AheadBehind, AppError> {
    let range = format!("HEAD...{upstream}");
    let captured = run_git(
        git_exe,
        repo_path,
        &["rev-list", "--left-right", "--count", &range],
    )
    .await?;

    if captured.exit_code != Some(0) {
        return Ok(AheadBehind {
            ahead: None,
            behind: None,
        });
    }

    match parse_left_right_count(&captured.raw_stdout) {
        Some((ahead, behind)) => Ok(AheadBehind {
            ahead: Some(ahead),
            behind: Some(behind),
        }),
        None => Ok(AheadBehind {
            ahead: None,
            behind: None,
        }),
    }
}

/// Parse the two whitespace-separated integers from `git rev-list
/// --left-right --count` output (e.g. `"0\t1\n"` -> `(0, 1)`). The left count
/// is "ahead" (commits on HEAD not in upstream); the right is "behind".
pub(crate) fn parse_left_right_count(s: &str) -> Option<(i64, i64)> {
    let mut parts = s.split_whitespace();
    let left = parts.next()?.parse::<i64>().ok()?;
    let right = parts.next()?.parse::<i64>().ok()?;
    Some((left, right))
}

#[cfg(test)]
mod tests {
    use super::parse_left_right_count;

    #[test]
    fn parses_tab_separated() {
        assert_eq!(parse_left_right_count("0\t1\n"), Some((0, 1)));
        assert_eq!(parse_left_right_count("3\t5"), Some((3, 5)));
        assert_eq!(parse_left_right_count("  2   7  "), Some((2, 7)));
    }

    #[test]
    fn rejects_garbage() {
        assert_eq!(parse_left_right_count(""), None);
        assert_eq!(parse_left_right_count("only-one"), None);
        assert_eq!(parse_left_right_count("a\tb"), None);
    }

    use crate::git::SystemGitEngine;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    /// Run a plain git command, returning whether it succeeded.
    fn git(dir: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn git_resolvable() -> bool {
        Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[tokio::test]
    async fn fetch_sees_remote_commit_as_behind() {
        if !git_resolvable() {
            eprintln!("skipping fetch_sees_remote_commit_as_behind: git not resolvable");
            return;
        }
        let engine = match SystemGitEngine::discover() {
            Ok(e) => e,
            Err(_) => {
                eprintln!("skipping: SystemGitEngine::discover failed");
                return;
            }
        };

        let root = TempDir::new().expect("tempdir");
        let upstream = root.path().join("upstream");
        let work = root.path().join("work");
        let clone = root.path().join("clone");
        std::fs::create_dir_all(&upstream).unwrap();

        // Bare remote.
        assert!(git(root.path(), &["init", "--bare", upstream.to_str().unwrap()]));

        // Seed it via a working clone with one commit, then push.
        assert!(git(root.path(), &["clone", upstream.to_str().unwrap(), work.to_str().unwrap()]));
        assert!(git(&work, &["config", "user.email", "t@example.com"]));
        assert!(git(&work, &["config", "user.name", "T"]));
        std::fs::write(work.join("a.txt"), "1\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "first"]));
        // Default branch may be master or main depending on git config.
        let pushed = git(&work, &["push", "origin", "HEAD"]);
        assert!(pushed, "push of first commit should succeed");

        // Clone we will inspect (it is now one commit behind after the remote
        // gets a second commit).
        assert!(git(root.path(), &["clone", upstream.to_str().unwrap(), clone.to_str().unwrap()]));

        // Second commit on the remote (via the work clone).
        std::fs::write(work.join("a.txt"), "2\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "second"]));
        assert!(git(&work, &["push", "origin", "HEAD"]));

        // Fetch in the clone: should succeed.
        let outcome = engine.fetch(&clone).await.expect("fetch ok");
        assert!(outcome.success, "fetch should succeed: {}", outcome.raw_stderr);
        assert_eq!(outcome.exit_code, Some(0));
        assert!(outcome.duration_ms >= 0);

        // The clone is behind by one commit relative to its upstream.
        let inspect = engine.inspect(&clone).expect("inspect clone");
        let upstream_ref = inspect
            .upstream_branch
            .expect("clone should have an upstream tracking branch");
        let ab = engine
            .ahead_behind(&clone, &upstream_ref)
            .await
            .expect("ahead_behind ok");
        assert_eq!(ab.behind, Some(1), "clone should be 1 behind after remote commit");
    }
}
