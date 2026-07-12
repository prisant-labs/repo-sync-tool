//! git::cli - owned by E-03 (network + mutation: shell out to git).
//!
//! All network and mutating git goes through the system git CLI (never git2's
//! network transports), capturing the full raw command, stdout, stderr, exit
//! code, and duration for the activity log.

use std::path::Path;
use std::time::{Duration, Instant};

use tokio::process::Command;

use crate::error::AppError;
use crate::git::{AheadBehind, FetchClass, FetchOutcome, PullClass, PullOutcome};

/// Raw capture of a single git CLI invocation.
pub(crate) struct Captured {
    pub raw_command: String,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: i64,
}

/// Wall-clock cap on any single git subprocess. A watched repo can point at a
/// remote that accepts the TCP connection and then never responds (a dead or
/// hostile "slowloris" remote); with no bound, `output().await` blocks for the OS
/// TCP timeout (minutes). Because the scheduler awaits each job, and each tick in
/// turn, one hung git op would wedge the whole background loop. This is a liveness
/// bound, not a performance knob: RepoSync fetches are incremental, so 120s sits
/// far above any legitimate op yet well under a user-noticeable hang. On elapse the
/// child is killed and the op is surfaced as a network-class failure (see
/// [`run_git`]). Security review R2 / correctness CC-1 (2026-07).
const GIT_OP_TIMEOUT: Duration = Duration::from_secs(120);

/// Build the hardened git command RepoSync runs on the user's behalf.
///
/// RepoSync shells out to git AUTONOMOUSLY (on the scheduler tick) inside repo
/// directories the user assembled from arbitrary origins, so the target repo's
/// `.git/config` is attacker-controlled. Several git config keys EXECUTE a command
/// during a plain fetch/pull with no user action - `core.fsmonitor`, a
/// `core.hooksPath` hook, and the `ext::<cmd>` remote-helper protocol. We
/// neutralize those repo-local execution vectors here: a `-c key=value` on the
/// command line has the highest config precedence, so it overrides whatever the
/// repo set (worst case the override is a benign no-op, never the attacker's
/// command). We also suppress interactive credential prompting so a background op
/// fails fast instead of popping a GUI dialog or hanging on input that can never
/// arrive in a windowless tray context.
///
/// We deliberately do NOT null the user's global/system git config: that config is
/// the user's own and is trusted, and nulling it would break legitimate proxy,
/// `insteadOf`, and credential-helper setups. Two repo-local vectors that only fire
/// for an auth-demanding or ssh remote (`credential.helper=!cmd`, `core.sshCommand`)
/// are left as a documented residual, because overriding them without breaking
/// legitimate private-repo credentials needs a separate decision (see
/// `docs/backlog.md`). Security review R1 (2026-07).
fn build_git_command(git_exe: &Path, repo_path: &Path, args: &[&str]) -> Command {
    let mut cmd = Command::new(git_exe);
    // Neutralize repo-local, config-driven code execution (highest precedence wins).
    cmd.arg("-c").arg("core.fsmonitor=");
    cmd.arg("-c").arg("core.hooksPath=/dev/null");
    cmd.arg("-c").arg("protocol.ext.allow=never");
    cmd.arg("-C").arg(repo_path);
    for a in args {
        cmd.arg(a);
    }
    // Fail fast rather than prompt or hang for credentials in a windowless context.
    // With no cached credential-helper result, git emits "terminal prompts
    // disabled", which classify_fetch reads as an auth failure (paused-on-auth).
    cmd.env("GIT_TERMINAL_PROMPT", "0");
    cmd.env("GIT_ASKPASS", "");
    cmd.env("SSH_ASKPASS", "");
    cmd.env("GCM_INTERACTIVE", "never");
    // If the GIT_OP_TIMEOUT future is dropped, kill the child rather than leak it.
    cmd.kill_on_drop(true);
    // On Windows, spawning the console-mode git.exe without CREATE_NO_WINDOW
    // attaches a fresh console to each child, which flashes on screen. Every
    // network/CLI git op funnels through here and the scheduler fans these out
    // per repo each minute, so this flag is what stops the tray app from popping
    // a burst of black git.exe windows. Same idiom as src-tauri/src/opener.rs;
    // tokio::process::Command exposes creation_flags as an inherent method on
    // Windows, so no CommandExt import is needed here.
    #[cfg(windows)]
    {
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Run `git -C <repo_path> <args...>` (hardened; see [`build_git_command`]),
/// capturing output, exit code, and wall time, bounded by [`GIT_OP_TIMEOUT`]. A
/// spawn failure (e.g. git missing) maps to [`AppError::GitNotFound`]; a timeout is
/// surfaced as a synthetic non-zero capture whose stderr classifies as a network
/// failure, so the existing failure state machine retries and then auto-pauses
/// rather than poisoning the caller with a hard error.
pub(crate) async fn run_git(
    git_exe: &Path,
    repo_path: &Path,
    args: &[&str],
) -> Result<Captured, AppError> {
    let mut cmd = build_git_command(git_exe, repo_path, args);

    let pretty_args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    let raw_command = format!(
        "{} -C {} {}",
        git_exe.display(),
        repo_path.display(),
        pretty_args.join(" ")
    );

    let started = Instant::now();
    let output = match tokio::time::timeout(GIT_OP_TIMEOUT, cmd.output()).await {
        Ok(result) => result.map_err(|_| AppError::GitNotFound)?,
        Err(_elapsed) => {
            // Exceeded GIT_OP_TIMEOUT; kill_on_drop terminated the child when the
            // output() future was dropped. Surface a synthetic capture: exit_code
            // None plus an "operation timed out" stderr, which classify_fetch and
            // classify_pull map to a NetworkFailure (retry, then 3-strikes pause) -
            // never a hard error that would surface as a spurious GitNotFound.
            return Ok(Captured {
                raw_command,
                raw_stdout: String::new(),
                raw_stderr: format!(
                    "reposync: git operation timed out after {}s and was terminated",
                    GIT_OP_TIMEOUT.as_secs()
                ),
                exit_code: None,
                duration_ms: started.elapsed().as_millis() as i64,
            });
        }
    };
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
    let class = classify_fetch(
        captured.exit_code,
        &captured.raw_stdout,
        &captured.raw_stderr,
    );
    Ok(FetchOutcome {
        raw_command: captured.raw_command,
        raw_stdout: captured.raw_stdout,
        raw_stderr: captured.raw_stderr,
        exit_code: captured.exit_code,
        duration_ms: captured.duration_ms,
        success: class.is_success(),
        class,
    })
}

/// Classify a `git fetch` invocation into a [`FetchClass`] from its exit code
/// plus captured stdout/stderr (AC10). This is a PURE function over the capture:
/// no I/O, deterministic, unit-testable from string fixtures.
///
/// Classification strategy (conservative): on a zero exit, distinguish a real
/// update (refs advanced, visible as `->` arrows or a `* [new ...]` line) from a
/// no-op ("already up to date" / empty output). On a non-zero exit, scan stderr
/// for an auth signature first (so a 403 over HTTP is not mistaken for a network
/// drop), then a network signature; anything else is `Unknown`.
pub(crate) fn classify_fetch(exit_code: Option<i32>, stdout: &str, stderr: &str) -> FetchClass {
    if exit_code == Some(0) {
        // git prints fetch progress to STDERR, not stdout, so inspect both.
        let combined = format!("{stdout}\n{stderr}");
        if fetch_shows_update(&combined) {
            FetchClass::Success
        } else {
            FetchClass::NoOp
        }
    } else if stderr_is_auth_failure(stderr) {
        FetchClass::AuthFailure
    } else if stderr_is_network_failure(stderr) {
        FetchClass::NetworkFailure
    } else {
        FetchClass::Unknown
    }
}

/// Whether fetch output shows the remote tracking refs actually advanced.
/// A successful fetch that pulled commits prints lines containing `->`
/// (ref update arrows) or `* [new branch]` / `* [new tag]` markers.
fn fetch_shows_update(output: &str) -> bool {
    output.lines().any(|line| {
        let l = line.trim();
        l.contains("->") || l.contains("[new branch]") || l.contains("[new tag]")
    })
}

/// Whether a non-zero fetch's stderr matches a known AUTHENTICATION signature.
/// Checked before the network signature so an HTTP 403 is classified as auth,
/// not network.
fn stderr_is_auth_failure(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("authentication failed")
        || s.contains("could not read username")
        || s.contains("could not read password")
        || s.contains("permission denied")
        || s.contains("access denied")
        || s.contains("403 forbidden")
        || s.contains("invalid username or password")
        || s.contains("terminal prompts disabled")
        || (s.contains("fatal: unable to access") && s.contains("403"))
}

/// Whether a non-zero fetch's stderr matches a known NETWORK/transport
/// signature (DNS, connection, timeout, TLS handshake).
fn stderr_is_network_failure(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("could not resolve host")
        || s.contains("couldn't resolve host")
        || s.contains("could not resolve proxy")
        || s.contains("connection timed out")
        || s.contains("connection refused")
        || s.contains("connection reset")
        || s.contains("failed to connect")
        || s.contains("network is unreachable")
        || s.contains("operation timed out")
        || s.contains("ssl_error")
        || s.contains("unable to access")
            && (s.contains("timed out") || s.contains("resolve") || s.contains("connect"))
}

/// Fast-forward `pull --ff-only`. On a non-zero exit the outcome is still
/// returned (with `success = false`); the caller decides how to surface it.
///
/// `--ff-only` makes git refuse to create a merge commit: if the branch cannot
/// fast-forward, the pull fails with a non-zero exit rather than mutating, so the
/// fast-forward-only safety rule is enforced by git itself, not just by the
/// policy engine that gated this call.
pub(crate) async fn pull_ff_only(
    git_exe: &Path,
    repo_path: &Path,
) -> Result<PullOutcome, AppError> {
    let captured = run_git(git_exe, repo_path, &["pull", "--ff-only"]).await?;
    let class = classify_pull(
        captured.exit_code,
        &captured.raw_stdout,
        &captured.raw_stderr,
    );
    Ok(PullOutcome {
        raw_command: captured.raw_command,
        raw_stdout: captured.raw_stdout,
        raw_stderr: captured.raw_stderr,
        exit_code: captured.exit_code,
        duration_ms: captured.duration_ms,
        success: class.is_success(),
        class,
    })
}

/// Classify a `git pull --ff-only` invocation into a [`PullClass`] from its exit
/// code plus captured stdout/stderr. A PURE function over the capture: no I/O,
/// deterministic, unit-testable from string fixtures.
///
/// Strategy (reuses the fetch-class auth/network signatures, then adds the
/// pull-specific fast-forward failure): on a zero exit, distinguish a real
/// fast-forward (the tree advanced, shown by "Fast-forward" or a `->` updating
/// line) from a no-op ("Already up to date"). On a non-zero exit, scan stderr
/// for an auth signature first, then a network signature, then the
/// "not possible to fast-forward" / "Need to specify how to reconcile divergent
/// branches" / "diverging" signatures that `--ff-only` emits when the branch has
/// diverged; anything else is `Unknown`.
pub(crate) fn classify_pull(exit_code: Option<i32>, stdout: &str, stderr: &str) -> PullClass {
    if exit_code == Some(0) {
        // git prints pull progress across both streams; inspect both.
        let combined = format!("{stdout}\n{stderr}");
        if pull_shows_up_to_date(&combined) {
            PullClass::NoOp
        } else {
            PullClass::Success
        }
    } else if stderr_is_auth_failure(stderr) {
        PullClass::AuthFailure
    } else if stderr_is_network_failure(stderr) {
        PullClass::NetworkFailure
    } else if stderr_is_ff_not_possible(stderr) {
        PullClass::FfNotPossible
    } else {
        PullClass::Unknown
    }
}

/// Whether a zero-exit pull's output says the branch was already up to date
/// (nothing to fast-forward).
fn pull_shows_up_to_date(output: &str) -> bool {
    let s = output.to_ascii_lowercase();
    s.contains("already up to date") || s.contains("already up-to-date")
}

/// Whether a non-zero `pull --ff-only` stderr matches the FAST-FORWARD-IMPOSSIBLE
/// signature git emits when the branch has diverged (so `--ff-only` refuses).
fn stderr_is_ff_not_possible(stderr: &str) -> bool {
    let s = stderr.to_ascii_lowercase();
    s.contains("not possible to fast-forward")
        || s.contains("can only fast-forward")
        || s.contains("need to specify how to reconcile divergent branches")
        || s.contains("diverging branches can't be fast-forwarded")
        || (s.contains("fast-forward") && s.contains("aborting"))
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

/// Resolve a ref (or `HEAD`) to its object id via `git rev-parse <ref>`,
/// through the capture point so the call is auditable. Returns `None` when the
/// command fails or the output does not parse as a SHA. The CLI counterpart to
/// the git2 HEAD-SHA read; the engine offers both so the all-CLI fallback is a
/// localized swap.
pub(crate) async fn rev_parse(
    git_exe: &Path,
    repo_path: &Path,
    refname: &str,
) -> Result<Option<String>, AppError> {
    let captured = run_git(git_exe, repo_path, &["rev-parse", refname]).await?;
    if captured.exit_code != Some(0) {
        return Ok(None);
    }
    Ok(parse_rev_parse(&captured.raw_stdout))
}

/// Read working-tree dirtiness via `git status --porcelain=v2`, through the
/// capture point. The CLI counterpart to the git2 dirty read.
///
/// On a non-zero exit this returns [`AppError::CommandFailed`] (mirroring
/// [`rev_parse`]): a failed `git status` with empty stdout must NOT be read as a
/// clean tree, which would let the policy pick a mutation over an unknown state.
pub(crate) async fn status(git_exe: &Path, repo_path: &Path) -> Result<PorcelainStatus, AppError> {
    let captured = run_git(git_exe, repo_path, &["status", "--porcelain=v2"]).await?;
    if captured.exit_code != Some(0) {
        return Err(command_failed(&captured));
    }
    Ok(parse_porcelain_v2(&captured.raw_stdout))
}

/// Enumerate refs and their upstreams via `git for-each-ref`, through the
/// capture point. The format is fixed to `%(refname) %(objectname) %(upstream)`
/// so [`parse_for_each_ref`] can read it.
pub(crate) async fn for_each_ref(
    git_exe: &Path,
    repo_path: &Path,
) -> Result<Vec<RefRow>, AppError> {
    let captured = run_git(
        git_exe,
        repo_path,
        &[
            "for-each-ref",
            "--format=%(refname) %(objectname) %(upstream)",
        ],
    )
    .await?;
    if captured.exit_code != Some(0) {
        return Err(command_failed(&captured));
    }
    Ok(parse_for_each_ref(&captured.raw_stdout))
}

/// Build an [`AppError::CommandFailed`] from a non-zero git capture. Centralizes
/// the exit-code + stderr mapping for the read ops that must surface a failed
/// command rather than mis-parsing empty output as a benign result (M1/M2). The
/// exit code is forced to a concrete `i32` (a signal-killed process with no code
/// becomes `-1`) so the error always carries a non-zero discriminator.
fn command_failed(captured: &Captured) -> AppError {
    AppError::CommandFailed {
        exit_code: captured.exit_code.unwrap_or(-1),
        stderr: captured.raw_stderr.clone(),
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

/// Parse `git rev-parse <ref>` stdout into a single resolved object id.
///
/// `rev-parse` prints one full hex SHA per line; this returns the FIRST line as
/// the resolved ref. Returns `None` if there is no non-empty line or the line
/// is not a 4-40 char hex string (e.g. an error leaked to stdout). A pure
/// function over captured stdout.
pub(crate) fn parse_rev_parse(stdout: &str) -> Option<String> {
    let line = stdout.lines().map(str::trim).find(|l| !l.is_empty())?;
    let is_hex = (4..=40).contains(&line.len()) && line.chars().all(|c| c.is_ascii_hexdigit());
    if is_hex {
        Some(line.to_string())
    } else {
        None
    }
}

/// The dirty/clean verdict parsed from `git status --porcelain=v2`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PorcelainStatus {
    /// Any tracked change (ordinary `1`/`2` or unmerged `u` entry) is present.
    pub has_tracked_changes: bool,
    /// At least one untracked (`?`) entry is present.
    pub has_untracked: bool,
}

impl PorcelainStatus {
    /// Whether the working tree is dirty: any tracked change OR any untracked
    /// file. Matches the git2 dirty definition used in `inspect.rs`.
    pub fn is_dirty(&self) -> bool {
        self.has_tracked_changes || self.has_untracked
    }
}

/// Parse `git status --porcelain=v2` stdout into a [`PorcelainStatus`].
///
/// Porcelain v2 line kinds (the leading token of each line):
///   - `1` ordinary changed entry (tracked change)
///   - `2` renamed/copied entry (tracked change)
///   - `u` unmerged entry (tracked change)
///   - `?` untracked entry
///   - `!` ignored entry (not dirty; we never request these)
///   - `#` header line (branch/oid metadata; ignored)
///
/// A pure function over captured stdout; no I/O.
pub(crate) fn parse_porcelain_v2(stdout: &str) -> PorcelainStatus {
    let mut has_tracked_changes = false;
    let mut has_untracked = false;
    for line in stdout.lines() {
        match line.chars().next() {
            Some('1') | Some('2') | Some('u') => has_tracked_changes = true,
            Some('?') => has_untracked = true,
            // '#' headers, '!' ignored, and blank lines are not dirtiness.
            _ => {}
        }
    }
    PorcelainStatus {
        has_tracked_changes,
        has_untracked,
    }
}

/// One ref row parsed from `git for-each-ref`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefRow {
    /// The fully-qualified ref name, e.g. `refs/heads/main`.
    pub refname: String,
    /// The object id the ref points at (full hex SHA).
    pub object_id: String,
    /// The configured upstream of this ref, when present (the `%(upstream)`
    /// field), e.g. `refs/remotes/origin/main`. Empty fields parse to `None`.
    pub upstream: Option<String>,
}

/// Parse `git for-each-ref --format='%(refname) %(objectname) %(upstream)'`
/// stdout into [`RefRow`]s.
///
/// Each non-empty line is `refname objectname [upstream]`, space-separated.
/// A missing upstream (git emits an empty `%(upstream)` field) yields `None`.
/// Lines with fewer than two whitespace tokens are skipped. A pure function
/// over captured stdout; the caller controls the `--format` it pairs with this.
pub(crate) fn parse_for_each_ref(stdout: &str) -> Vec<RefRow> {
    let mut rows = Vec::new();
    for line in stdout.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(refname), Some(object_id)) = (parts.next(), parts.next()) else {
            continue;
        };
        let upstream = parts.next().map(|s| s.to_string());
        rows.push(RefRow {
            refname: refname.to_string(),
            object_id: object_id.to_string(),
            upstream,
        });
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::{classify_fetch, parse_left_right_count};
    use crate::error::AppError;
    use crate::git::FetchClass;

    // --- git command hardening (security R1/R2) -------------------------------
    //
    // run_git funnels every autonomous git op. These assert the hardening is
    // actually applied to the constructed command: repo-local config-driven
    // execution vectors are overridden, and interactive credential prompting is
    // disabled. The argv/env are introspected via std::process::Command's getters
    // (the command is never spawned), so the checks are deterministic and offline.

    #[test]
    fn run_git_command_neutralizes_repo_local_execution() {
        use super::build_git_command;
        use std::path::Path;

        let cmd = build_git_command(
            Path::new("git"),
            Path::new("some/repo"),
            &["fetch", "--all"],
        );
        let args: Vec<String> = cmd
            .as_std()
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();

        // A `-c key=value` override (highest config precedence) for each vector.
        let has_override = |val: &str| args.windows(2).any(|w| w[0] == "-c" && w[1] == val);
        assert!(
            has_override("core.fsmonitor="),
            "fsmonitor not disabled: {args:?}"
        );
        assert!(
            has_override("core.hooksPath=/dev/null"),
            "hooks not disabled: {args:?}"
        );
        assert!(
            has_override("protocol.ext.allow=never"),
            "ext:: protocol not blocked: {args:?}"
        );
        // The actual op and target repo survive the hardening.
        assert!(args.iter().any(|a| a == "-C"), "missing -C: {args:?}");
        assert!(args.iter().any(|a| a == "fetch"), "missing op: {args:?}");
        assert!(
            args.iter().any(|a| a == "--all"),
            "missing op arg: {args:?}"
        );
    }

    #[test]
    fn run_git_command_disables_interactive_credential_prompts() {
        use super::build_git_command;
        use std::path::Path;

        let cmd = build_git_command(Path::new("git"), Path::new("some/repo"), &["fetch"]);
        let envs: Vec<(String, Option<String>)> = cmd
            .as_std()
            .get_envs()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().into_owned(),
                    v.map(|v| v.to_string_lossy().into_owned()),
                )
            })
            .collect();
        let env_is = |key: &str, want: &str| {
            envs.iter()
                .any(|(k, v)| k == key && v.as_deref() == Some(want))
        };
        assert!(
            env_is("GIT_TERMINAL_PROMPT", "0"),
            "terminal prompt not disabled: {envs:?}"
        );
        assert!(
            env_is("GCM_INTERACTIVE", "never"),
            "GCM UI not disabled: {envs:?}"
        );
        assert!(
            envs.iter().any(|(k, _)| k == "GIT_ASKPASS"),
            "GIT_ASKPASS not overridden: {envs:?}"
        );
    }

    // --- fetch classification (AC10 / BL-NI-05) -------------------------------
    //
    // Fixtures are captured-shape `git fetch` output. git writes fetch progress
    // to STDERR, so the "update" and "no-op" cases put their text there.

    #[test]
    fn classifies_no_op_already_up_to_date() {
        // A fetch that found nothing new prints no ref-update lines (often no
        // output at all on stderr).
        assert_eq!(classify_fetch(Some(0), "", ""), FetchClass::NoOp);
        assert_eq!(
            classify_fetch(Some(0), "", "Fetching origin\n"),
            FetchClass::NoOp
        );
    }

    #[test]
    fn classifies_success_when_refs_advance() {
        let stderr = "Fetching origin\n\
             remote: Enumerating objects: 5, done.\n\
             From github.com:o/repo\n   \
             abc1234..def5678  main       -> origin/main\n";
        assert_eq!(classify_fetch(Some(0), "", stderr), FetchClass::Success);
    }

    #[test]
    fn classifies_success_on_new_branch() {
        let stderr = "From github.com:o/repo\n \
             * [new branch]      feature    -> origin/feature\n";
        assert_eq!(classify_fetch(Some(0), "", stderr), FetchClass::Success);
    }

    #[test]
    fn classifies_auth_failure_https() {
        let stderr = "fatal: Authentication failed for 'https://github.com/o/repo.git/'\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::AuthFailure
        );
    }

    #[test]
    fn classifies_auth_failure_no_terminal_prompt() {
        let stderr = "fatal: could not read Username for 'https://github.com': \
             terminal prompts disabled\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::AuthFailure
        );
    }

    #[test]
    fn classifies_auth_failure_ssh_permission_denied() {
        let stderr = "git@github.com: Permission denied (publickey).\n\
             fatal: Could not read from remote repository.\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::AuthFailure
        );
    }

    #[test]
    fn classifies_network_failure_dns() {
        let stderr = "fatal: unable to access 'https://github.com/o/repo.git/': \
             Could not resolve host: github.com\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::NetworkFailure
        );
    }

    #[test]
    fn classifies_network_failure_connection_timeout() {
        let stderr = "ssh: connect to host github.com port 22: Connection timed out\n\
             fatal: Could not read from remote repository.\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::NetworkFailure
        );
    }

    #[test]
    fn classifies_network_failure_connection_refused() {
        let stderr = "fatal: unable to access 'https://example.com/r.git/': \
             Failed to connect to example.com port 443: Connection refused\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::NetworkFailure
        );
    }

    #[test]
    fn unknown_failure_is_conservative_fallback() {
        // A non-zero exit whose stderr matches no known signature must NOT be
        // silently mapped to auth or network.
        let stderr = "fatal: the remote end hung up unexpectedly\n";
        assert_eq!(classify_fetch(Some(128), "", stderr), FetchClass::Unknown);
        // A non-zero exit with empty stderr is also Unknown.
        assert_eq!(classify_fetch(Some(1), "", ""), FetchClass::Unknown);
        // No exit code at all (signal-killed) with no signature -> Unknown.
        assert_eq!(classify_fetch(None, "", ""), FetchClass::Unknown);
    }

    #[test]
    fn auth_checked_before_network_on_http_403() {
        // A 403 over HTTP can mention "unable to access"; auth must win.
        let stderr = "fatal: unable to access 'https://github.com/o/repo.git/': \
             The requested URL returned error: 403 Forbidden\n";
        assert_eq!(
            classify_fetch(Some(128), "", stderr),
            FetchClass::AuthFailure
        );
    }

    #[test]
    fn fetch_class_is_success_truth_table() {
        assert!(FetchClass::Success.is_success());
        assert!(FetchClass::NoOp.is_success());
        assert!(!FetchClass::AuthFailure.is_success());
        assert!(!FetchClass::NetworkFailure.is_success());
        assert!(!FetchClass::Unknown.is_success());
    }

    // --- pull --ff-only classification (E-07) ---------------------------------

    use super::classify_pull;
    use crate::git::PullClass;

    #[test]
    fn classifies_pull_no_op_already_up_to_date() {
        // git's modern phrasing is "Already up to date."; older is "up-to-date".
        assert_eq!(
            classify_pull(Some(0), "Already up to date.\n", ""),
            PullClass::NoOp
        );
        assert_eq!(
            classify_pull(Some(0), "Already up-to-date.\n", ""),
            PullClass::NoOp
        );
    }

    #[test]
    fn classifies_pull_success_on_fast_forward() {
        let stdout = "Updating abc1234..def5678\n\
             Fast-forward\n a.txt | 2 +-\n 1 file changed, 1 insertion(+), 1 deletion(-)\n";
        assert_eq!(classify_pull(Some(0), stdout, ""), PullClass::Success);
    }

    #[test]
    fn classifies_pull_ff_not_possible_when_diverged() {
        // `git pull --ff-only` on a diverged branch refuses with this message.
        let stderr = "fatal: Not possible to fast-forward, aborting.\n";
        assert_eq!(
            classify_pull(Some(128), "", stderr),
            PullClass::FfNotPossible
        );
    }

    #[test]
    fn classifies_pull_ff_not_possible_divergent_reconcile() {
        // Newer git phrases the divergent-branch refusal differently under
        // pull.ff=only / --ff-only.
        let stderr = "fatal: Need to specify how to reconcile divergent branches.\n";
        assert_eq!(
            classify_pull(Some(128), "", stderr),
            PullClass::FfNotPossible
        );
    }

    #[test]
    fn classifies_pull_auth_failure() {
        let stderr = "fatal: Authentication failed for 'https://github.com/o/repo.git/'\n";
        assert_eq!(classify_pull(Some(128), "", stderr), PullClass::AuthFailure);
    }

    #[test]
    fn classifies_pull_network_failure() {
        let stderr = "fatal: unable to access 'https://github.com/o/repo.git/': \
             Could not resolve host: github.com\n";
        assert_eq!(
            classify_pull(Some(128), "", stderr),
            PullClass::NetworkFailure
        );
    }

    #[test]
    fn pull_auth_checked_before_ff_not_possible() {
        // A stderr that mentions BOTH auth and fast-forward must classify as
        // auth: auth is the actionable failure, and ff-not-possible is the
        // benign-but-blocked case checked last.
        let stderr = "fatal: Authentication failed; not possible to fast-forward\n";
        assert_eq!(classify_pull(Some(128), "", stderr), PullClass::AuthFailure);
    }

    #[test]
    fn pull_unknown_is_conservative_fallback() {
        let stderr = "fatal: the remote end hung up unexpectedly\n";
        assert_eq!(classify_pull(Some(128), "", stderr), PullClass::Unknown);
        assert_eq!(classify_pull(Some(1), "", ""), PullClass::Unknown);
        assert_eq!(classify_pull(None, "", ""), PullClass::Unknown);
    }

    #[test]
    fn pull_class_is_success_truth_table() {
        assert!(PullClass::Success.is_success());
        assert!(PullClass::NoOp.is_success());
        assert!(!PullClass::FfNotPossible.is_success());
        assert!(!PullClass::AuthFailure.is_success());
        assert!(!PullClass::NetworkFailure.is_success());
        assert!(!PullClass::Unknown.is_success());
    }

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

    // --- rev-parse parser (AC3) ----------------------------------------------

    use super::{parse_for_each_ref, parse_porcelain_v2, parse_rev_parse, RefRow};

    #[test]
    fn rev_parse_resolves_full_sha() {
        let out = "1f2e3d4c5b6a7980172635445362718091a2b3c4\n";
        assert_eq!(
            parse_rev_parse(out).as_deref(),
            Some("1f2e3d4c5b6a7980172635445362718091a2b3c4")
        );
    }

    #[test]
    fn rev_parse_takes_first_line() {
        // `git rev-parse HEAD @{u}` prints two SHAs; the first is HEAD.
        let out = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n\
             bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb\n";
        assert_eq!(
            parse_rev_parse(out).as_deref(),
            Some("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
        );
    }

    #[test]
    fn rev_parse_rejects_non_hex() {
        assert_eq!(parse_rev_parse(""), None);
        assert_eq!(parse_rev_parse("\n\n"), None);
        // An error message leaked to stdout must not parse as a SHA.
        assert_eq!(parse_rev_parse("fatal: bad revision 'nope'"), None);
    }

    // --- status --porcelain=v2 parser (AC3) ----------------------------------

    #[test]
    fn porcelain_clean_tree() {
        // A clean tree prints only header lines (or nothing).
        let out = "# branch.oid 1111111111111111111111111111111111111111\n\
             # branch.head main\n\
             # branch.upstream origin/main\n\
             # branch.ab +0 -0\n";
        let status = parse_porcelain_v2(out);
        assert!(!status.is_dirty(), "header-only output is clean");
        assert!(!status.has_tracked_changes);
        assert!(!status.has_untracked);
    }

    #[test]
    fn porcelain_tracked_modification_is_dirty() {
        // An ordinary changed entry (leading '1').
        let out = "# branch.head main\n\
             1 .M N... 100644 100644 100644 1111111 1111111 file.txt\n";
        let status = parse_porcelain_v2(out);
        assert!(status.is_dirty());
        assert!(status.has_tracked_changes);
        assert!(!status.has_untracked);
    }

    #[test]
    fn porcelain_untracked_is_dirty() {
        let out = "# branch.head main\n? new.txt\n";
        let status = parse_porcelain_v2(out);
        assert!(status.is_dirty());
        assert!(!status.has_tracked_changes);
        assert!(status.has_untracked);
    }

    #[test]
    fn porcelain_rename_and_unmerged_are_tracked_changes() {
        let renamed = "2 R. N... 100644 100644 100644 111 111 R100 new.txt\told.txt\n";
        assert!(parse_porcelain_v2(renamed).has_tracked_changes);
        let unmerged = "u UU N... 100644 100644 100644 100644 111 222 333 conflict.txt\n";
        assert!(parse_porcelain_v2(unmerged).has_tracked_changes);
    }

    #[test]
    fn porcelain_ignored_entries_are_not_dirty() {
        // We never request '!' lines, but if present they must not count.
        let out = "# branch.head main\n! ignored.log\n";
        let status = parse_porcelain_v2(out);
        assert!(!status.is_dirty());
    }

    // --- for-each-ref parser (AC3) -------------------------------------------

    #[test]
    fn for_each_ref_parses_branch_with_upstream() {
        let out = "refs/heads/main 1111111111111111111111111111111111111111 \
             refs/remotes/origin/main\n";
        let rows = parse_for_each_ref(out);
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0],
            RefRow {
                refname: "refs/heads/main".into(),
                object_id: "1111111111111111111111111111111111111111".into(),
                upstream: Some("refs/remotes/origin/main".into()),
            }
        );
    }

    #[test]
    fn for_each_ref_handles_missing_upstream() {
        // A local branch with no upstream emits an empty %(upstream) field, so
        // the line has only two tokens.
        let out = "refs/heads/feature 2222222222222222222222222222222222222222\n";
        let rows = parse_for_each_ref(out);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].refname, "refs/heads/feature");
        assert_eq!(rows[0].upstream, None);
    }

    #[test]
    fn for_each_ref_parses_multiple_and_skips_blank() {
        let out = "refs/heads/main aaaa refs/remotes/origin/main\n\
             \n\
             refs/heads/dev bbbb\n";
        let rows = parse_for_each_ref(out);
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows[0].upstream.as_deref(),
            Some("refs/remotes/origin/main")
        );
        assert_eq!(rows[1].refname, "refs/heads/dev");
        assert_eq!(rows[1].upstream, None);
    }

    #[test]
    fn for_each_ref_empty_input_is_empty() {
        assert!(parse_for_each_ref("").is_empty());
        assert!(parse_for_each_ref("\n\n").is_empty());
    }

    use crate::git::{GitEngine, SystemGitEngine};
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
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
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
        assert!(git(
            root.path(),
            &["init", "--bare", upstream.to_str().unwrap()]
        ));

        // Seed it via a working clone with one commit, then push.
        assert!(git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), work.to_str().unwrap()]
        ));
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
        assert!(git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), clone.to_str().unwrap()]
        ));

        // Second commit on the remote (via the work clone).
        std::fs::write(work.join("a.txt"), "2\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "second"]));
        assert!(git(&work, &["push", "origin", "HEAD"]));

        // Fetch in the clone: should succeed and CLASSIFY as a real update
        // (the clone pulled the second commit's tracking-ref advance).
        let outcome = engine.fetch(&clone).await.expect("fetch ok");
        assert!(
            outcome.success,
            "fetch should succeed: {}",
            outcome.raw_stderr
        );
        assert_eq!(outcome.exit_code, Some(0));
        assert_eq!(
            outcome.class,
            FetchClass::Success,
            "fetch that advanced origin/HEAD classifies as Success: {}",
            outcome.raw_stderr
        );
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
        assert_eq!(
            ab.behind,
            Some(1),
            "clone should be 1 behind after remote commit"
        );
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn pull_ff_only_fast_forwards_a_behind_clone() {
        // E-07 end-to-end: a clone one commit behind its upstream fast-forwards
        // cleanly. We fabricate the behind state inline (the fixtures live behind
        // the test-support feature, not available to this in-crate test), fetch so
        // the tracking ref is ahead, then pull --ff-only and assert it advanced.
        if !git_resolvable() {
            eprintln!("skipping pull_ff_only_fast_forwards_a_behind_clone: git not resolvable");
            return;
        }
        let engine = match SystemGitEngine::discover() {
            Ok(e) => e,
            Err(_) => return,
        };

        let root = TempDir::new().expect("tempdir");
        let upstream = root.path().join("upstream");
        let work = root.path().join("work");
        let clone = root.path().join("clone");
        std::fs::create_dir_all(&upstream).unwrap();

        assert!(git(
            root.path(),
            &["init", "--bare", upstream.to_str().unwrap()]
        ));
        assert!(git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), work.to_str().unwrap()]
        ));
        assert!(git(&work, &["config", "user.email", "t@example.com"]));
        assert!(git(&work, &["config", "user.name", "T"]));
        std::fs::write(work.join("a.txt"), "1\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "first"]));
        assert!(git(&work, &["push", "origin", "HEAD"]));

        // The clone we will fast-forward; level at clone time.
        assert!(git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), clone.to_str().unwrap()]
        ));

        // Advance the upstream by one commit via the work clone.
        std::fs::write(work.join("a.txt"), "2\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "second"]));
        assert!(git(&work, &["push", "origin", "HEAD"]));

        // Bring the clone's tracking ref up to date (the policy engine gates the
        // pull on a known-behind state, so a check/fetch precedes it).
        let fetch = engine.fetch(&clone).await.expect("fetch ok");
        assert!(fetch.success, "fetch should succeed: {}", fetch.raw_stderr);

        // Pull --ff-only: must fast-forward and classify as a real success.
        let pull = engine.pull_ff_only(&clone).await.expect("pull ok");
        assert!(
            pull.success,
            "pull --ff-only should fast-forward: {}",
            pull.raw_stderr
        );
        assert_eq!(pull.exit_code, Some(0));
        assert_eq!(
            pull.class,
            PullClass::Success,
            "a fast-forward that advanced the tree classifies as Success: {} / {}",
            pull.raw_stdout,
            pull.raw_stderr
        );
        assert!(pull.raw_command.contains("pull --ff-only"));
        assert!(pull.duration_ms >= 0);

        // The working tree is now at the second commit's content. (Trim to
        // tolerate the CRLF git may check out on Windows.)
        let content = std::fs::read_to_string(clone.join("a.txt")).unwrap();
        assert_eq!(content.trim(), "2", "the fast-forward must update the tree");

        // And the clone is now level with its upstream (behind 0).
        let inspect = engine.inspect(&clone).expect("inspect clone");
        let upstream_ref = inspect
            .upstream_branch
            .expect("clone should have an upstream tracking branch");
        let ab = engine
            .ahead_behind(&clone, &upstream_ref)
            .await
            .expect("ahead_behind ok");
        assert_eq!(ab.behind, Some(0), "the clone is level after the pull");
    }

    #[tokio::test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    async fn pull_ff_only_refuses_to_fast_forward_a_diverged_clone() {
        // E-07 safety: --ff-only must REFUSE (non-zero, FfNotPossible) when the
        // branch has diverged, never creating a merge commit. The clone gets a
        // local commit AND the upstream advances, so a fast-forward is impossible.
        if !git_resolvable() {
            eprintln!(
                "skipping pull_ff_only_refuses_to_fast_forward_a_diverged_clone: git missing"
            );
            return;
        }
        let engine = match SystemGitEngine::discover() {
            Ok(e) => e,
            Err(_) => return,
        };

        let root = TempDir::new().expect("tempdir");
        let upstream = root.path().join("upstream");
        let work = root.path().join("work");
        let clone = root.path().join("clone");
        std::fs::create_dir_all(&upstream).unwrap();

        assert!(git(
            root.path(),
            &["init", "--bare", upstream.to_str().unwrap()]
        ));
        assert!(git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), work.to_str().unwrap()]
        ));
        assert!(git(&work, &["config", "user.email", "t@example.com"]));
        assert!(git(&work, &["config", "user.name", "T"]));
        std::fs::write(work.join("a.txt"), "1\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "first"]));
        assert!(git(&work, &["push", "origin", "HEAD"]));

        assert!(git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), clone.to_str().unwrap()]
        ));
        assert!(git(&clone, &["config", "user.email", "t@example.com"]));
        assert!(git(&clone, &["config", "user.name", "T"]));

        // Diverge: a local commit on the clone, AND an upstream commit.
        std::fs::write(clone.join("b.txt"), "local\n").unwrap();
        assert!(git(&clone, &["add", "b.txt"]));
        assert!(git(&clone, &["commit", "-m", "local divergent"]));

        std::fs::write(work.join("a.txt"), "2\n").unwrap();
        assert!(git(&work, &["add", "a.txt"]));
        assert!(git(&work, &["commit", "-m", "second"]));
        assert!(git(&work, &["push", "origin", "HEAD"]));

        let fetch = engine.fetch(&clone).await.expect("fetch ok");
        assert!(fetch.success, "fetch should succeed: {}", fetch.raw_stderr);

        // Pull --ff-only must fail (diverged) rather than merge.
        let pull = engine.pull_ff_only(&clone).await.expect("pull call ok");
        assert!(
            !pull.success,
            "a diverged --ff-only pull must NOT succeed (it would need a merge)"
        );
        assert_eq!(
            pull.class,
            PullClass::FfNotPossible,
            "a diverged --ff-only refusal classifies as FfNotPossible: {} / {}",
            pull.raw_stdout,
            pull.raw_stderr
        );
    }

    #[tokio::test]
    async fn status_errs_on_nonzero_exit() {
        // M1: `git status` in a NON-repo exits non-zero (128) with empty stdout.
        // Before the fix, status() parsed stdout regardless of exit code, so this
        // read as a clean tree. It must instead return an Err.
        if !git_resolvable() {
            eprintln!("skipping status_errs_on_nonzero_exit: git not resolvable");
            return;
        }
        let engine = match SystemGitEngine::discover() {
            Ok(e) => e,
            Err(_) => return,
        };
        let tmp = TempDir::new().expect("tempdir");
        // A plain empty directory is not a git repo.
        let result = engine.status(tmp.path()).await;
        assert!(
            result.is_err(),
            "status in a non-repo must error, not report a clean tree: {result:?}"
        );
        // It is the git command-failed variant, carrying the non-zero exit.
        match result {
            Err(AppError::CommandFailed { exit_code, .. }) => {
                assert_ne!(exit_code, 0, "a failed status must carry a non-zero exit");
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn for_each_ref_errs_on_nonzero_exit() {
        // M2: `git for-each-ref` in a NON-repo exits non-zero with empty stdout.
        // Before the fix, for_each_ref() returned an empty Vec silently. It must
        // instead return an Err.
        if !git_resolvable() {
            eprintln!("skipping for_each_ref_errs_on_nonzero_exit: git not resolvable");
            return;
        }
        let engine = match SystemGitEngine::discover() {
            Ok(e) => e,
            Err(_) => return,
        };
        let tmp = TempDir::new().expect("tempdir");
        let result = engine.for_each_ref(tmp.path()).await;
        assert!(
            result.is_err(),
            "for-each-ref in a non-repo must error, not return an empty list: {result:?}"
        );
        match result {
            Err(AppError::CommandFailed { exit_code, .. }) => {
                assert_ne!(
                    exit_code, 0,
                    "a failed for-each-ref must carry a non-zero exit"
                );
            }
            other => panic!("expected CommandFailed, got {other:?}"),
        }
    }

    /// Smoke-test the CLI read ops (`rev-parse`, `status --porcelain=v2`,
    /// `for-each-ref`) against a throwaway repo: each runs through the capture
    /// point and its parser, exercising the AC3 surface end-to-end.
    #[tokio::test]
    async fn cli_reads_against_real_repo() {
        if !git_resolvable() {
            eprintln!("skipping cli_reads_against_real_repo: git not resolvable");
            return;
        }
        let engine = match SystemGitEngine::discover() {
            Ok(e) => e,
            Err(_) => {
                eprintln!("skipping cli_reads_against_real_repo: discover failed");
                return;
            }
        };

        let tmp = TempDir::new().expect("tempdir");
        let dir = tmp.path();
        assert!(git(dir, &["init"]));
        assert!(git(dir, &["config", "user.email", "t@example.com"]));
        assert!(git(dir, &["config", "user.name", "T"]));
        std::fs::write(dir.join("a.txt"), "1\n").unwrap();
        assert!(git(dir, &["add", "a.txt"]));
        assert!(git(dir, &["commit", "-m", "first"]));

        // rev-parse HEAD resolves to a 40-char SHA.
        let sha = engine
            .rev_parse(dir, "HEAD")
            .await
            .expect("rev_parse ok")
            .expect("HEAD resolves");
        assert_eq!(sha.len(), 40);
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));

        // Clean tree: status reports not-dirty.
        let clean = engine.status(dir).await.expect("status ok");
        assert!(!clean.is_dirty(), "freshly committed tree is clean");

        // Dirty it with an untracked file.
        std::fs::write(dir.join("b.txt"), "new\n").unwrap();
        let dirty = engine.status(dir).await.expect("status ok");
        assert!(dirty.is_dirty(), "untracked file makes it dirty");
        assert!(dirty.has_untracked);

        // for-each-ref lists at least the current branch ref with its SHA.
        let rows = engine.for_each_ref(dir).await.expect("for_each_ref ok");
        assert!(
            rows.iter().any(|r| r.refname.starts_with("refs/heads/")),
            "for-each-ref should list the local branch: {rows:?}"
        );
    }
}
