//! git::fixtures - owned by E-04 (the git fixture test harness).
//!
//! Programmatically fabricates **bare + working repo pairs** in tempdirs, each
//! driven into one of seven known states, so the E-03 git engine (and the
//! downstream E-07 policy engine and E-08 scheduler) can be tested
//! deterministically with no UI, no network, and no real personal repos.
//!
//! The payoff for downstream consumers is a one-call entry point:
//!
//! ```ignore
//! use reposync_core::git::fixtures::{build_fixture, FixtureState};
//! let fx = build_fixture(FixtureState::Behind);
//! let engine = reposync_core::git::SystemGitEngine::discover().unwrap();
//! let read = engine.inspect(fx.working_path()).unwrap();
//! assert_eq!(read.is_dirty, fx.expected.dirty);
//! // `fx` owns the tempdir; hold it for the test's lifetime. When `fx` drops,
//! // the tempdir (bare + working repos) is removed.
//! ```
//!
//! ## Determinism (AC2)
//!
//! Construction fixes the author/committer identity, the commit messages, and
//! controls timestamps via `GIT_*_DATE` environment variables, so the branch
//! topology, ref names, and ahead/behind relationships are byte-stable across
//! runs and runners. Exact commit SHAs are a nice-to-have (the spec's open
//! question) and are NOT asserted; the determinism check diffs structure and
//! relationships, not SHAs.
//!
//! ## Isolation + cleanup (AC4)
//!
//! Every fixture lives entirely inside an owned [`tempfile::TempDir`]. Dropping
//! the [`Fixture`] drops that handle, which removes the bare and working repos.
//! The repo paths borrow from the handle and become invalid once it drops, so
//! the consumer MUST hold the [`Fixture`] for the lifetime of the test.
//!
//! ## The seven states (AC1)
//!
//! | [`FixtureState`]   | branch | dirty | detached | ahead       | behind      |
//! |--------------------|--------|-------|----------|-------------|-------------|
//! | `Clean`            | yes    | no    | no       | `Some(0)`   | `Some(0)`   |
//! | `Dirty`            | yes    | yes   | no       | `Some(0)`   | `Some(0)`   |
//! | `Ahead`            | yes    | no    | no       | `Some(n>0)` | `Some(0)`   |
//! | `Behind`           | yes    | no    | no       | `Some(0)`   | `Some(n>0)` |
//! | `DetachedHead`     | none   | no    | yes      | `None`      | `None`      |
//! | `DeletedUpstream`  | yes    | no    | no       | `None`      | `None`      |
//! | `NoUpstream`       | yes    | no    | no       | `None`      | `None`      |
//!
//! For `DetachedHead`, `DeletedUpstream`, and `NoUpstream` the expected
//! ahead/behind is `None` (no comparison base), matching E-03's provisional
//! contract (E-03 AC11) - deliberately distinct from `Some(0)` ("level with
//! upstream").

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

/// A selector over the seven known fixture states (AC6). Callers pick a state by
/// name rather than by string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FixtureState {
    /// Working tree matches upstream, no local changes.
    Clean,
    /// Uncommitted working-tree modifications present.
    Dirty,
    /// Local commits not pushed to the bare upstream.
    Ahead,
    /// Bare upstream has commits the working clone lacks.
    Behind,
    /// HEAD checked out to a commit, not a branch.
    DetachedHead,
    /// The tracking branch's upstream ref removed from the bare repo.
    DeletedUpstream,
    /// A local branch with no configured upstream.
    NoUpstream,
}

impl FixtureState {
    /// All seven states, in a stable order. Used to parameterize the
    /// cross-check and determinism checks over the whole set.
    pub const ALL: [FixtureState; 7] = [
        FixtureState::Clean,
        FixtureState::Dirty,
        FixtureState::Ahead,
        FixtureState::Behind,
        FixtureState::DetachedHead,
        FixtureState::DeletedUpstream,
        FixtureState::NoUpstream,
    ];

    /// A short, stable name for the state (for test diagnostics).
    pub fn name(self) -> &'static str {
        match self {
            FixtureState::Clean => "clean",
            FixtureState::Dirty => "dirty",
            FixtureState::Ahead => "ahead",
            FixtureState::Behind => "behind",
            FixtureState::DetachedHead => "detached-head",
            FixtureState::DeletedUpstream => "deleted-upstream",
            FixtureState::NoUpstream => "no-upstream",
        }
    }
}

/// The intended, declared facts of a fabricated state (AC6). Recipes assert the
/// engines report exactly these; downstream consumers read them to know what to
/// expect without re-reading the builder internals.
///
/// `ahead`/`behind` are `Option`-shaped to match E-03's contract (E-03 AC11):
/// `None` means "no comparison base" (no upstream / deleted upstream / detached),
/// which is deliberately distinct from `Some(0)` ("level with upstream").
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpectedFacts {
    /// The active branch shorthand (e.g. `main`), or `None` when detached.
    pub branch: Option<String>,
    /// The full 40-hex HEAD commit SHA.
    pub head_sha: String,
    /// Whether the working tree has uncommitted changes.
    pub dirty: bool,
    /// Whether HEAD is detached (not on a branch).
    pub detached: bool,
    /// Commits on HEAD not in upstream, or `None` when there is no comparison
    /// base.
    pub ahead: Option<u32>,
    /// Commits in upstream not on HEAD, or `None` when there is no comparison
    /// base.
    pub behind: Option<u32>,
}

/// A ready bare + working repo pair in a tempdir, plus its declared facts (AC6).
///
/// Holds the owned [`TempDir`]; dropping the `Fixture` removes the repos. The
/// `bare_path`/`working_path` borrow from that handle, so the consumer must
/// keep the `Fixture` alive for the lifetime of the test.
#[derive(Debug)]
pub struct Fixture {
    /// The owned tempdir root. Both repos live under it; its `Drop` cleans up.
    tempdir: TempDir,
    bare_path: PathBuf,
    working_path: PathBuf,
    /// The state's declared facts.
    pub expected: ExpectedFacts,
}

impl Fixture {
    /// Path to the bare (upstream) repository.
    pub fn bare_path(&self) -> &Path {
        &self.bare_path
    }

    /// Path to the working clone - the repo the engine inspects.
    pub fn working_path(&self) -> &Path {
        &self.working_path
    }

    /// The tempdir root that owns both repos (for isolation assertions).
    pub fn root_path(&self) -> &Path {
        self.tempdir.path()
    }
}

/// Build a fixture in the requested state (AC6 entry point). Returns a
/// [`Fixture`] owning its tempdir; hold it for the test's lifetime.
///
/// Panics if any underlying git operation fails (this is test-support code: a
/// failed fabrication is a test bug, surfaced immediately and loudly).
pub fn build_fixture(state: FixtureState) -> Fixture {
    match state {
        FixtureState::Clean => recipe_clean(),
        FixtureState::Dirty => recipe_dirty(),
        FixtureState::Ahead => recipe_ahead(),
        FixtureState::Behind => recipe_behind(),
        FixtureState::DetachedHead => recipe_detached_head(),
        FixtureState::DeletedUpstream => recipe_deleted_upstream(),
        FixtureState::NoUpstream => recipe_no_upstream(),
    }
}

/// Branch the recipes build on. Fixed (rather than relying on git's
/// `init.defaultBranch`) so ref names are identical across runners regardless
/// of host git config.
const FIXTURE_BRANCH: &str = "main";

// --- state recipes (AC1) -----------------------------------------------------

/// **clean** - working clone matches its upstream, no local changes.
fn recipe_clean() -> Fixture {
    let mut b = RepoBuilder::new();
    b.commit_file("a.txt", "1\n");
    b.push();
    let head_sha = b.head_sha();
    b.into_fixture(ExpectedFacts {
        branch: Some(FIXTURE_BRANCH.to_string()),
        head_sha,
        dirty: false,
        detached: false,
        ahead: Some(0),
        behind: Some(0),
    })
}

/// **dirty** - clean upstream relationship, plus an uncommitted working-tree
/// change (a new untracked file).
fn recipe_dirty() -> Fixture {
    let mut b = RepoBuilder::new();
    b.commit_file("a.txt", "1\n");
    b.push();
    let head_sha = b.head_sha();
    // Make the tree dirty without changing HEAD: an untracked file.
    b.write_working_file("dirty.txt", "uncommitted\n");
    b.into_fixture(ExpectedFacts {
        branch: Some(FIXTURE_BRANCH.to_string()),
        head_sha,
        dirty: true,
        detached: false,
        ahead: Some(0),
        behind: Some(0),
    })
}

/// **ahead** - local commits that have not been pushed to the bare upstream.
fn recipe_ahead() -> Fixture {
    let mut b = RepoBuilder::new();
    b.commit_file("a.txt", "1\n");
    b.push();
    // Two more local commits, NOT pushed: HEAD is 2 ahead of @{u}.
    b.commit_file("a.txt", "2\n");
    b.commit_file("a.txt", "3\n");
    let head_sha = b.head_sha();
    b.into_fixture(ExpectedFacts {
        branch: Some(FIXTURE_BRANCH.to_string()),
        head_sha,
        dirty: false,
        detached: false,
        ahead: Some(2),
        behind: Some(0),
    })
}

/// **behind** - the bare upstream has a commit the working clone lacks.
fn recipe_behind() -> Fixture {
    let mut b = RepoBuilder::new();
    b.commit_file("a.txt", "1\n");
    b.push();
    // Advance the upstream by one commit using a SEPARATE pusher clone, then
    // fetch into the working clone so its tracking ref is ahead of HEAD.
    b.advance_upstream("a.txt", "2\n");
    b.fetch();
    let head_sha = b.head_sha();
    b.into_fixture(ExpectedFacts {
        branch: Some(FIXTURE_BRANCH.to_string()),
        head_sha,
        dirty: false,
        detached: false,
        ahead: Some(0),
        behind: Some(1),
    })
}

/// **detached HEAD** - HEAD points at a commit, not a branch.
fn recipe_detached_head() -> Fixture {
    let mut b = RepoBuilder::new();
    b.commit_file("a.txt", "1\n");
    b.push();
    b.commit_file("a.txt", "2\n");
    let head_sha = b.head_sha();
    // Detach onto the current commit: HEAD is now a bare SHA, no branch.
    b.detach_head();
    b.into_fixture(ExpectedFacts {
        branch: None,
        head_sha,
        dirty: false,
        detached: true,
        // No branch -> no upstream -> no comparison base (E-03 AC11).
        ahead: None,
        behind: None,
    })
}

/// **deleted-upstream** - the working clone still names its upstream in config,
/// but the remote-tracking ref no longer resolves (it was pruned).
fn recipe_deleted_upstream() -> Fixture {
    let mut b = RepoBuilder::new();
    b.commit_file("a.txt", "1\n");
    b.push();
    let head_sha = b.head_sha();
    // Remove the remote-tracking ref the upstream config points at, leaving the
    // branch.<name>.merge / .remote config intact but the comparison base gone.
    b.delete_tracking_ref();
    b.into_fixture(ExpectedFacts {
        branch: Some(FIXTURE_BRANCH.to_string()),
        head_sha,
        dirty: false,
        detached: false,
        // Config names an upstream, but it no longer resolves (E-03 AC11).
        ahead: None,
        behind: None,
    })
}

/// **no-upstream** - a local branch with no configured upstream at all.
fn recipe_no_upstream() -> Fixture {
    // A standalone repo (no clone, no remote): the branch has no upstream.
    let mut b = RepoBuilder::new_standalone();
    b.commit_file("a.txt", "1\n");
    let head_sha = b.head_sha();
    b.into_fixture(ExpectedFacts {
        branch: Some(FIXTURE_BRANCH.to_string()),
        head_sha,
        dirty: false,
        detached: false,
        // No tracking branch configured (E-03 AC11).
        ahead: None,
        behind: None,
    })
}

/// Fixed author/committer identity for deterministic commits.
const FIXTURE_NAME: &str = "RepoSync Fixture";
const FIXTURE_EMAIL: &str = "fixture@reposync.test";

/// A fixed timestamp base (2021-01-01T00:00:00 +0000) for `GIT_*_DATE`. Each
/// commit advances by a fixed delta so ordering is stable without wall-clock
/// dependence.
const FIXTURE_EPOCH: i64 = 1_609_459_200;

/// A small builder over a bare + working pair used by the recipes (the
/// primitive of step 1). Wraps the system `git` CLI with a fixed identity and
/// controlled timestamps so every fabricated structure is deterministic.
///
/// Layout under the owned tempdir:
///   - `bare.git`     - the bare upstream repo (created unless standalone).
///   - `working`      - the working clone the engine inspects.
///   - `pusher`       - a second clone used to advance the upstream out-of-band
///     (created lazily by [`RepoBuilder::advance_upstream`]).
struct RepoBuilder {
    tempdir: TempDir,
    /// The bare upstream path, or `None` for a standalone (no-remote) repo.
    bare: Option<PathBuf>,
    working: PathBuf,
    /// Monotonic commit counter, driving the controlled timestamp so commit
    /// order is stable without wall-clock dependence.
    commit_index: i64,
}

impl RepoBuilder {
    /// Create a bare upstream + a working clone of it, with a fixed initial
    /// branch and identity. The working clone tracks `origin/main` once the
    /// first commit is pushed.
    fn new() -> RepoBuilder {
        let tempdir = TempDir::new().expect("create fixture tempdir");
        let root = tempdir.path().to_path_buf();
        let bare = root.join("bare.git");
        let working = root.join("working");

        // Bare upstream with a fixed initial branch so ref names match across
        // runners regardless of the host's init.defaultBranch.
        run_git_in(
            &root,
            &[
                "init",
                "--bare",
                &format!("--initial-branch={FIXTURE_BRANCH}"),
                bare.to_str().unwrap(),
            ],
        );
        // Clone the (empty) bare repo. `--branch` cannot be used here because the
        // bare repo has no commits yet, so the branch ref does not exist; instead
        // the bare's `--initial-branch` makes the clone's unborn HEAD point at
        // `main`. Cloning an empty repo warns ("empty repository") but succeeds.
        run_git_in(
            &root,
            &["clone", bare.to_str().unwrap(), working.to_str().unwrap()],
        );
        configure_repo(&working);
        // Pin the working clone's unborn HEAD to the fixed branch so the first
        // commit lands on `main` regardless of the host clone default.
        run_git_in(
            &working,
            &[
                "symbolic-ref",
                "HEAD",
                &format!("refs/heads/{FIXTURE_BRANCH}"),
            ],
        );

        RepoBuilder {
            tempdir,
            bare: Some(bare),
            working,
            commit_index: 0,
        }
    }

    /// Create a STANDALONE working repo (no bare upstream, no remote) - the
    /// basis for the no-upstream state.
    fn new_standalone() -> RepoBuilder {
        let tempdir = TempDir::new().expect("create fixture tempdir");
        let root = tempdir.path().to_path_buf();
        let working = root.join("working");

        run_git_in(
            &root,
            &[
                "init",
                &format!("--initial-branch={FIXTURE_BRANCH}"),
                working.to_str().unwrap(),
            ],
        );
        configure_repo(&working);

        RepoBuilder {
            tempdir,
            bare: None,
            working,
            commit_index: 0,
        }
    }

    /// Write `contents` to `name` in the working tree and commit it with the
    /// fixed identity and the next controlled timestamp.
    fn commit_file(&mut self, name: &str, contents: &str) {
        self.write_working_file(name, contents);
        run_git_in(&self.working, &["add", name]);
        let stamp = self.next_timestamp();
        run_git_dated(
            &self.working,
            &[
                "commit",
                "-m",
                &format!("fixture commit {}", self.commit_index),
            ],
            &stamp,
        );
    }

    /// Write a file into the working tree WITHOUT committing it (used to make a
    /// tree dirty, or as the content of a subsequent commit).
    fn write_working_file(&self, name: &str, contents: &str) {
        std::fs::write(self.working.join(name), contents).expect("write working file");
    }

    /// Push the working clone's branch to the bare upstream and set it as the
    /// tracking branch (`-u`).
    fn push(&self) {
        run_git_in(&self.working, &["push", "-u", "origin", FIXTURE_BRANCH]);
    }

    /// Fetch from the upstream into the working clone (advances tracking refs).
    fn fetch(&self) {
        run_git_in(&self.working, &["fetch", "origin"]);
    }

    /// Advance the bare upstream by one commit using a SEPARATE pusher clone, so
    /// the working clone falls behind once it fetches. Requires a bare upstream.
    fn advance_upstream(&mut self, name: &str, contents: &str) {
        let bare = self
            .bare
            .as_ref()
            .expect("advance_upstream needs a bare upstream");
        let pusher = self.tempdir.path().join("pusher");
        run_git_in(
            self.tempdir.path(),
            &[
                "clone",
                &format!("--branch={FIXTURE_BRANCH}"),
                bare.to_str().unwrap(),
                pusher.to_str().unwrap(),
            ],
        );
        configure_repo(&pusher);
        std::fs::write(pusher.join(name), contents).expect("write pusher file");
        run_git_in(&pusher, &["add", name]);
        let stamp = self.next_timestamp();
        run_git_dated(
            &pusher,
            &[
                "commit",
                "-m",
                &format!("upstream commit {}", self.commit_index),
            ],
            &stamp,
        );
        run_git_in(&pusher, &["push", "origin", FIXTURE_BRANCH]);
    }

    /// Detach HEAD onto the current commit (no branch).
    fn detach_head(&self) {
        // `checkout --detach` points HEAD at the current commit's SHA.
        run_git_in(&self.working, &["checkout", "--detach", "HEAD"]);
    }

    /// Delete the remote-tracking ref the working clone's upstream points at,
    /// simulating a pruned/deleted upstream while branch config still names it.
    fn delete_tracking_ref(&self) {
        run_git_in(
            &self.working,
            &[
                "update-ref",
                "-d",
                &format!("refs/remotes/origin/{FIXTURE_BRANCH}"),
            ],
        );
    }

    /// The current HEAD SHA of the working clone (full 40-hex).
    fn head_sha(&self) -> String {
        let out = git_stdout(&self.working, &["rev-parse", "HEAD"]);
        out.trim().to_string()
    }

    /// Advance the controlled-timestamp counter and return the `GIT_*_DATE`
    /// value for the next commit. Each commit is one minute after the last.
    fn next_timestamp(&mut self) -> String {
        self.commit_index += 1;
        let epoch = FIXTURE_EPOCH + self.commit_index * 60;
        format!("{epoch} +0000")
    }

    /// Consume the builder into a [`Fixture`], moving the owned tempdir so
    /// cleanup is tied to the returned handle's `Drop`.
    fn into_fixture(self, expected: ExpectedFacts) -> Fixture {
        let bare_path = self.bare.clone().unwrap_or_else(|| self.working.clone());
        Fixture {
            tempdir: self.tempdir,
            bare_path,
            working_path: self.working,
            expected,
        }
    }
}

// --- git CLI helpers ---------------------------------------------------------

/// Configure the fixed identity (and disable signing / GPG / commit hooks that
/// could vary by host) on a repo so commits are deterministic.
fn configure_repo(repo: &Path) {
    run_git_in(repo, &["config", "user.name", FIXTURE_NAME]);
    run_git_in(repo, &["config", "user.email", FIXTURE_EMAIL]);
    run_git_in(repo, &["config", "commit.gpgsign", "false"]);
    run_git_in(repo, &["config", "tag.gpgsign", "false"]);
    // Normalize line endings so working-tree content is byte-stable on Windows.
    run_git_in(repo, &["config", "core.autocrlf", "false"]);
}

/// Run `git -C <repo> <args>`, panicking with captured stderr on a non-zero
/// exit. Test-support code: a failed fabrication is a test bug, surfaced loudly.
fn run_git_in(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} in {} failed (exit {:?}): {}",
        repo.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Run a `git commit` (or any committing op) with controlled author + committer
/// dates via the `GIT_AUTHOR_DATE` / `GIT_COMMITTER_DATE` environment so commit
/// timestamps are deterministic.
fn run_git_dated(repo: &Path, args: &[&str], date: &str) {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} in {} failed (exit {:?}): {}",
        repo.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
}

/// Run `git -C <repo> <args>` and return captured stdout, panicking on failure.
fn git_stdout(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn git {args:?}: {e}"));
    assert!(
        output.status.success(),
        "git {args:?} in {} failed (exit {:?}): {}",
        repo.display(),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::inspect;

    /// Assert a 40-hex SHA.
    fn is_full_sha(s: &str) -> bool {
        s.len() == 40 && s.chars().all(|c| c.is_ascii_hexdigit())
    }

    #[test]
    fn clean_fixture_has_clean_branch_head() {
        let fx = build_fixture(FixtureState::Clean);
        assert!(!fx.expected.dirty);
        assert!(!fx.expected.detached);
        assert_eq!(fx.expected.branch.as_deref(), Some(FIXTURE_BRANCH));
        assert!(is_full_sha(&fx.expected.head_sha));
        assert_eq!(fx.expected.ahead, Some(0));
        assert_eq!(fx.expected.behind, Some(0));
    }

    /// Each recipe's DECLARED facts must match what E-03's git2 reads
    /// (`inspect` + `ahead_behind`) report against the fabricated repo (AC1).
    /// This is the recipe self-validation: the harness proves it built what it
    /// claims, using the real engine - not a hand-rolled re-read.
    fn assert_git2_agrees_with_declared(state: FixtureState) {
        let fx = build_fixture(state);
        let working = fx.working_path();

        let read = inspect::inspect(working).expect("git2 inspect ok");
        let ab = inspect::ahead_behind(working).expect("git2 ahead_behind ok");

        assert_eq!(
            read.is_detached,
            fx.expected.detached,
            "[{}] git2 detached disagrees with declared",
            state.name()
        );
        assert_eq!(
            read.active_branch,
            fx.expected.branch,
            "[{}] git2 branch disagrees with declared",
            state.name()
        );
        assert_eq!(
            read.is_dirty,
            fx.expected.dirty,
            "[{}] git2 dirty disagrees with declared",
            state.name()
        );
        assert_eq!(
            read.head_sha.as_deref(),
            Some(fx.expected.head_sha.as_str()),
            "[{}] git2 HEAD SHA disagrees with declared",
            state.name()
        );
        // ahead/behind: declared u32 maps to E-03's i64; None stays None.
        let declared_ahead = fx.expected.ahead.map(|n| n as i64);
        let declared_behind = fx.expected.behind.map(|n| n as i64);
        assert_eq!(
            ab.ahead,
            declared_ahead,
            "[{}] git2 ahead disagrees with declared",
            state.name()
        );
        assert_eq!(
            ab.behind,
            declared_behind,
            "[{}] git2 behind disagrees with declared",
            state.name()
        );
    }

    #[test]
    fn recipe_clean_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::Clean);
    }

    #[test]
    fn recipe_dirty_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::Dirty);
    }

    #[test]
    fn recipe_ahead_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::Ahead);
    }

    #[test]
    fn recipe_behind_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::Behind);
    }

    #[test]
    fn recipe_detached_head_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::DetachedHead);
    }

    #[test]
    fn recipe_deleted_upstream_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::DeletedUpstream);
    }

    #[test]
    fn recipe_no_upstream_matches_engine() {
        assert_git2_agrees_with_declared(FixtureState::NoUpstream);
    }

    // --- AC3: the git2-vs-CLI cross-check -------------------------------------
    //
    // Runs in the crate's own `#[cfg(test)]` tree (no `test-support` feature
    // needed) so it executes under a plain `cargo test --workspace`. The
    // `tests/git_fixture_cross_check.rs` integration test re-runs the same shape
    // through the PUBLIC feature-gated surface to prove the E-07/E-08 consumer
    // path; both must stay green.

    use crate::git::{cli, GitEngine, SystemGitEngine};

    /// CLI active-branch read via `git symbolic-ref --short -q HEAD`: the
    /// shorthand when on a branch, `None` when detached. Independent of git2 so
    /// the cross-check compares two real readings, not one value twice.
    fn cli_active_branch(working: &Path) -> Option<String> {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(working)
            .args(["symbolic-ref", "--short", "-q", "HEAD"])
            .output()
            .expect("git symbolic-ref spawn");
        if !out.status.success() {
            return None;
        }
        let name = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if name.is_empty() {
            None
        } else {
            Some(name)
        }
    }

    /// CLI detached probe: `git symbolic-ref -q HEAD` exits non-zero when HEAD is
    /// detached (it points at a SHA, not a branch ref).
    fn cli_is_detached(working: &Path) -> bool {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(working)
            .args(["symbolic-ref", "-q", "HEAD"])
            .output()
            .expect("git symbolic-ref spawn");
        !out.status.success()
    }

    /// The cross-check body for a single state: runs E-03's git2 reads AND the
    /// four CLI parsers against the same fabricated repo and asserts agreement on
    /// HEAD SHA, branch, dirty, detached, and ahead/behind. `fetch` is excluded
    /// (network/mutation, not a state read).
    async fn cross_check_state(engine: &SystemGitEngine, state: FixtureState) {
        let fx = build_fixture(state);
        let working = fx.working_path();
        let label = state.name();

        // git2 reads (inspect.rs).
        let g2 = engine.inspect(working).expect("git2 inspect ok");
        let g2_ab = engine
            .ahead_behind_read(working)
            .expect("git2 ahead_behind_read ok");

        // CLI parsers (cli.rs), via the public engine.
        let cli_head = engine
            .rev_parse(working, "HEAD")
            .await
            .expect("cli rev-parse ok");
        let cli_status = engine.status(working).await.expect("cli status ok");
        let cli_refs = engine
            .for_each_ref(working)
            .await
            .expect("cli for-each-ref ok");
        // rev-list ahead/behind only when a comparison base exists; otherwise the
        // contract value is None/None (E-03 AC11).
        let cli_ab = match g2.upstream_branch.as_deref() {
            Some(upstream) => engine
                .ahead_behind(working, upstream)
                .await
                .expect("cli ahead_behind ok"),
            None => crate::git::AheadBehind {
                ahead: None,
                behind: None,
            },
        };

        // 1. HEAD SHA: git2 vs CLI, and both vs the declared fact.
        assert_eq!(
            g2.head_sha.as_deref(),
            cli_head.as_deref(),
            "[{label}] HEAD SHA git2 vs CLI"
        );
        assert_eq!(
            g2.head_sha.as_deref(),
            Some(fx.expected.head_sha.as_str()),
            "[{label}] HEAD SHA git2 vs declared"
        );

        // 2. dirty.
        assert_eq!(
            g2.is_dirty,
            cli_status.is_dirty(),
            "[{label}] dirty git2 vs CLI"
        );
        assert_eq!(
            g2.is_dirty, fx.expected.dirty,
            "[{label}] dirty vs declared"
        );

        // 3. detached.
        assert_eq!(
            g2.is_detached,
            cli_is_detached(working),
            "[{label}] detached git2 vs CLI"
        );
        assert_eq!(
            g2.is_detached, fx.expected.detached,
            "[{label}] detached vs declared"
        );

        // 4. branch.
        assert_eq!(
            g2.active_branch,
            cli_active_branch(working),
            "[{label}] branch git2 vs CLI"
        );
        assert_eq!(
            g2.active_branch, fx.expected.branch,
            "[{label}] branch vs declared"
        );
        // for-each-ref exercised as a state read: HEAD's branch ref, when on a
        // branch, must appear in the parsed rows pointing at the HEAD SHA.
        if let Some(branch) = g2.active_branch.as_deref() {
            let fq = format!("refs/heads/{branch}");
            let row: &cli::RefRow = cli_refs
                .iter()
                .find(|r| r.refname == fq)
                .unwrap_or_else(|| panic!("[{label}] for-each-ref missing {fq}: {cli_refs:?}"));
            assert_eq!(
                Some(row.object_id.as_str()),
                g2.head_sha.as_deref(),
                "[{label}] for-each-ref branch SHA vs HEAD SHA"
            );
        }

        // 5. ahead/behind: git2 vs CLI, and both vs the declared (E-03 AC11)
        // contract - the ratification for no-upstream / deleted-upstream.
        assert_eq!(g2_ab.ahead, cli_ab.ahead, "[{label}] ahead git2 vs CLI");
        assert_eq!(g2_ab.behind, cli_ab.behind, "[{label}] behind git2 vs CLI");
        let declared_ahead = fx.expected.ahead.map(|n| n as i64);
        let declared_behind = fx.expected.behind.map(|n| n as i64);
        assert_eq!(
            g2_ab.ahead, declared_ahead,
            "[{label}] ahead vs declared E-03 contract"
        );
        assert_eq!(
            g2_ab.behind, declared_behind,
            "[{label}] behind vs declared E-03 contract"
        );
    }

    /// AC3: the parameterized git2-vs-CLI cross-check over ALL seven states.
    #[tokio::test]
    async fn git2_and_cli_agree_across_all_states() {
        let engine = SystemGitEngine::discover().expect("git engine should discover on this host");
        for state in FixtureState::ALL {
            cross_check_state(&engine, state).await;
        }
    }

    /// AC4: a fixture lives entirely inside its owned tempdir, and dropping it
    /// removes that directory. No path escapes the tempdir; nothing leaks.
    #[test]
    fn fixture_is_isolated_and_cleans_up() {
        let root_path;
        {
            let fx = build_fixture(FixtureState::Clean);
            root_path = fx.root_path().to_path_buf();
            assert!(root_path.exists(), "tempdir should exist while held");
            assert!(
                fx.bare_path().starts_with(&root_path),
                "bare repo must live inside the tempdir"
            );
            assert!(
                fx.working_path().starts_with(&root_path),
                "working repo must live inside the tempdir"
            );
        }
        // Dropped: the tempdir (and both repos) are gone.
        assert!(
            !root_path.exists(),
            "tempdir must be removed when the Fixture drops"
        );
    }

    /// AC2: building a state twice yields identical structure - same branch,
    /// same detached flag, same dirty flag, same ahead/behind relationship.
    /// Exact SHAs are a nice-to-have (spec open question) and not asserted here;
    /// we diff STRUCTURE and RELATIONSHIPS, not SHAs.
    #[test]
    fn recipes_are_structurally_deterministic() {
        for state in FixtureState::ALL {
            let a = build_fixture(state);
            let b = build_fixture(state);
            assert_eq!(
                a.expected.branch,
                b.expected.branch,
                "[{}] branch not deterministic",
                state.name()
            );
            assert_eq!(
                a.expected.detached,
                b.expected.detached,
                "[{}] detached not deterministic",
                state.name()
            );
            assert_eq!(
                a.expected.dirty,
                b.expected.dirty,
                "[{}] dirty not deterministic",
                state.name()
            );
            assert_eq!(
                a.expected.ahead,
                b.expected.ahead,
                "[{}] ahead not deterministic",
                state.name()
            );
            assert_eq!(
                a.expected.behind,
                b.expected.behind,
                "[{}] behind not deterministic",
                state.name()
            );
        }
    }

    /// With fixed identity, messages, AND controlled timestamps, the commit SHAs
    /// are in fact reproducible run-to-run. This is the spec's "nice-to-have"
    /// (fully-pinned SHAs); we assert it as a bonus so a future change that
    /// breaks reproducibility is caught, but the contract above only requires
    /// structural determinism.
    #[test]
    fn recipe_clean_sha_is_reproducible() {
        let a = build_fixture(FixtureState::Clean);
        let b = build_fixture(FixtureState::Clean);
        assert_eq!(
            a.expected.head_sha, b.expected.head_sha,
            "fixed identity + timestamps should yield a stable HEAD SHA"
        );
    }
}
