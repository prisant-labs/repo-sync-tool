//! git::inspect - owned by E-03 (cheap reads via git2).
//!
//! The ONE git2 read in the tracer. Pure, synchronous, never touches the
//! network and never mutates the repository.

use std::path::Path;

use git2::{Repository, StatusOptions};

use crate::error::AppError;
use crate::git::{AheadBehind, HeadState, InspectResult};

/// Inspect a repository's local state using git2.
///
/// Returns [`AppError::NotARepo`] if `repo_path` is not a git repository.
pub fn inspect(repo_path: &Path) -> Result<InspectResult, AppError> {
    let repo =
        Repository::open(repo_path).map_err(|e| AppError::not_a_repo_from_git2(repo_path, &e))?;

    // Kept as a Result: an ERROR here is not "not detached", it is "we could
    // not tell", and the two must not collapse. `is_detached` keeps the old
    // lenient reading because it is a pre-existing boolean column with readers
    // that expect one; `head_state` below is allowed to say it does not know.
    let detached = repo.head_detached();
    let is_detached = detached.as_ref().copied().unwrap_or(false);

    // HEAD may be unborn (a fresh repo with no commits), which `git2` reports
    // as an ERROR with a specific code. Every OTHER error is a repository that
    // could not be read - a corrupt ref, an unreadable `.git/HEAD`, a ref
    // pointing at nothing - and those are a different fact entirely. `.ok()`
    // alone throws that distinction away, so the code is kept.
    let head_err = repo.head().err().map(|e| e.code());
    let head = repo.head().ok();

    // Peel HEAD to its commit once, for both the SHA and the committer time (E-17
    // local recency). An unborn HEAD or an unreadable commit yields None for both.
    let head_commit = head.as_ref().and_then(|h| h.peel_to_commit().ok());
    let head_sha = head_commit.as_ref().map(|c| c.id().to_string());
    // The committer time (stable, matches "when the checkout's HEAD was committed"),
    // not the author time. git2 reports it in seconds since the epoch.
    let last_commit_at = head_commit.as_ref().map(|c| c.time().seconds());

    let active_branch = head.as_ref().and_then(|h| {
        if h.is_branch() {
            h.shorthand().ok().map(|s| s.to_string())
        } else {
            None
        }
    });

    // RR6: what HEAD actually is, decided here where all three cases are
    // visible at once. Detached first, because a detached HEAD is also not a
    // branch and would otherwise fall through to `Unborn`. `head.is_none()`
    // is the unborn case: `repo.head()` errors on a repository with no
    // commits, which is the one condition that leaves nothing to peel.
    //
    // A detached HEAD whose commit cannot be READ still reports `Detached` -
    // the question this answers is what HEAD points AT, not whether the
    // object store could be read, and conflating the two is what made
    // deriving this from a null `head_sha` lossy.
    let head_state = if detached.is_err() {
        // We could not even establish whether HEAD is detached, so we cannot
        // classify it. Saying `Branch` or `Unborn` here would be a guess.
        None
    } else if is_detached {
        Some(HeadState::Detached)
    } else if head.is_some() {
        Some(HeadState::Branch)
    } else if head_err == Some(git2::ErrorCode::UnbornBranch) {
        // The ONE error that genuinely means "a repository with no commits".
        Some(HeadState::Unborn)
    } else {
        // HEAD could not be read, and not because it is unborn. This is the
        // case the whole column was added to keep honest: reporting `Unborn`
        // would tell the user a damaged repository has no commits, which is
        // confident, reassuring and wrong. NULL renders as "never run", which
        // is at least not a claim about the repository (Codex review of PRs
        // #93-#96, finding 1).
        None
    };

    // Dirty = any tracked or untracked (non-ignored) change present.
    let mut status_opts = StatusOptions::new();
    status_opts.include_untracked(true);
    status_opts.include_ignored(false);
    let is_dirty = match repo.statuses(Some(&mut status_opts)) {
        Ok(statuses) => !statuses.is_empty(),
        // Safety: if the status read fails we do NOT know the tree is clean.
        // Reporting clean here could let the policy choose would-fast-forward
        // over a possibly-dirty tree. Treat unknown dirty-state conservatively
        // as dirty so the policy errs toward skip-with-reason; a single status
        // hiccup degrades to "skipped", never "clean".
        Err(_) => true,
    };

    // Best-effort upstream: the tracking branch of HEAD's local branch.
    let upstream_branch = upstream_for_head(&repo);

    Ok(InspectResult {
        head_sha,
        active_branch,
        is_dirty,
        is_detached,
        upstream_branch,
        last_commit_at,
        head_state,
    })
}

/// Best-effort resolution of HEAD's upstream (tracking) branch name. Returns
/// `None` on any error or when no upstream is configured.
fn upstream_for_head(repo: &Repository) -> Option<String> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let shorthand = head.shorthand().ok()?;
    let branch = repo.find_branch(shorthand, git2::BranchType::Local).ok()?;
    let upstream = branch.upstream().ok()?;
    upstream.name().ok().flatten().map(|s| s.to_string())
}

/// Ahead/behind commit counts of HEAD relative to its configured upstream, via
/// git2 (AC4). This is the cheap-read counterpart to the CLI's
/// `rev-list --left-right --count`.
///
/// Returns `AheadBehind { ahead: None, behind: None }` (AC11) for:
///   - **no-upstream**: HEAD's branch has no tracking branch configured;
///   - **deleted-upstream**: the configured upstream ref no longer resolves to
///     a commit (the remote-tracking ref was pruned);
///   - detached HEAD, an unborn HEAD, or any other read failure.
///
/// "No comparison base" (`None`) is deliberately distinct from "equal to
/// upstream" (`Some((0, 0))`): a repo whose upstream is gone must not be
/// reported as up to date.
pub fn ahead_behind(repo_path: &Path) -> Result<AheadBehind, AppError> {
    let repo =
        Repository::open(repo_path).map_err(|e| AppError::not_a_repo_from_git2(repo_path, &e))?;
    Ok(ahead_behind_in(&repo))
}

/// Inner ahead/behind read against an open repo. Every "no comparison base"
/// path collapses to `None`/`None`.
fn ahead_behind_in(repo: &Repository) -> AheadBehind {
    let none = AheadBehind {
        ahead: None,
        behind: None,
    };

    // HEAD must be a born branch (not detached, not unborn) to have an upstream.
    let Ok(head) = repo.head() else {
        return none;
    };
    if !head.is_branch() {
        return none;
    }
    let Ok(shorthand) = head.shorthand() else {
        return none;
    };
    let Ok(branch) = repo.find_branch(shorthand, git2::BranchType::Local) else {
        return none;
    };

    // No-upstream: no tracking branch configured.
    let Ok(upstream) = branch.upstream() else {
        return none;
    };

    let local_oid = match head.target() {
        Some(oid) => oid,
        None => return none,
    };
    // Deleted-upstream: the tracking ref exists in config but no longer resolves
    // to a commit (it was pruned). `get().target()` is None / the oid fails to
    // peel, so we report None rather than a misleading (0, 0).
    let upstream_oid = match upstream.get().target() {
        Some(oid) => oid,
        None => return none,
    };

    match repo.graph_ahead_behind(local_oid, upstream_oid) {
        Ok((ahead, behind)) => AheadBehind {
            ahead: Some(ahead as i64),
            behind: Some(behind as i64),
        },
        Err(_) => none,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn init_repo_with_commit(dir: &Path) {
        let repo = git2::Repository::init(dir).expect("init");
        std::fs::write(dir.join("file.txt"), "v1\n").expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("file.txt")).expect("add");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        let sig = git2::Signature::now("T", "t@example.com").expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .expect("commit");
    }

    #[test]
    fn inspects_clean_then_dirty() {
        let tmp = TempDir::new().expect("tempdir");
        init_repo_with_commit(tmp.path());

        let clean = inspect(tmp.path()).expect("inspect clean");
        let sha = clean.head_sha.expect("head_sha present");
        assert_eq!(sha.len(), 40, "head_sha must be 40 hex chars");
        assert!(sha.chars().all(|c| c.is_ascii_hexdigit()));
        // git2 default initial branch is "master".
        assert!(
            clean.active_branch.is_some(),
            "active_branch should be set on a branch HEAD"
        );
        assert!(!clean.is_dirty, "fresh commit should be clean");
        assert!(!clean.is_detached);

        // Dirty it with a new untracked file.
        std::fs::write(tmp.path().join("new.txt"), "untracked\n").expect("write");
        let dirty = inspect(tmp.path()).expect("inspect dirty");
        assert!(dirty.is_dirty, "untracked file should make it dirty");
    }

    #[test]
    fn inspect_reads_head_commit_committer_time() {
        // E-17 AC2: inspect reads the HEAD commit's committer time (unix seconds) so
        // the check path can persist it into last_local_commit_at. Commit with a KNOWN
        // committer time and assert inspect reports exactly that.
        let tmp = TempDir::new().expect("tempdir");
        let repo = git2::Repository::init(tmp.path()).expect("init");
        std::fs::write(tmp.path().join("f.txt"), "v1\n").expect("write");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new("f.txt")).expect("add");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("tree");
        let tree = repo.find_tree(tree_id).expect("find tree");
        // A fixed committer time (2023-11-14T22:13:20Z), offset 0.
        let when = git2::Time::new(1_700_000_000, 0);
        let sig = git2::Signature::new("T", "t@example.com", &when).expect("sig");
        repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .expect("commit");

        let inspected = inspect(tmp.path()).expect("inspect");
        assert_eq!(
            inspected.last_commit_at,
            Some(1_700_000_000),
            "inspect reports the HEAD commit's committer time"
        );
    }

    #[test]
    fn non_repo_is_not_a_repo_error() {
        let tmp = TempDir::new().expect("tempdir");
        let err = inspect(tmp.path()).expect_err("should fail");
        assert!(matches!(err, AppError::NotARepo { .. }));
    }

    // --- git2 ahead/behind (AC4 / AC11) --------------------------------------

    /// Run a plain `git` CLI command in `dir`; panics on failure (test helper
    /// used to fabricate upstream relationships git2 cannot create alone).
    fn run_git(dir: &Path, args: &[&str]) -> bool {
        std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn git_resolvable() -> bool {
        std::process::Command::new("git")
            .arg("--version")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    #[test]
    fn ahead_behind_none_when_no_upstream() {
        // A fresh repo with one commit and NO upstream configured -> None/None
        // (AC11: "no comparison base" is not "(0, 0)").
        let tmp = TempDir::new().expect("tempdir");
        init_repo_with_commit(tmp.path());

        let ab = ahead_behind(tmp.path()).expect("ahead_behind ok");
        assert_eq!(ab.ahead, None, "no upstream must report ahead = None");
        assert_eq!(ab.behind, None, "no upstream must report behind = None");
    }

    #[test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    fn ahead_behind_reports_behind_against_real_upstream() {
        if !git_resolvable() {
            eprintln!("skipping ahead_behind_reports_behind_against_real_upstream: git missing");
            return;
        }
        let root = TempDir::new().expect("tempdir");
        let upstream = root.path().join("upstream.git");
        let work = root.path().join("work");
        let clone = root.path().join("clone");

        assert!(run_git(
            root.path(),
            &["init", "--bare", upstream.to_str().unwrap()]
        ));
        assert!(run_git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), work.to_str().unwrap()]
        ));
        assert!(run_git(&work, &["config", "user.email", "t@example.com"]));
        assert!(run_git(&work, &["config", "user.name", "T"]));
        std::fs::write(work.join("a.txt"), "1\n").unwrap();
        assert!(run_git(&work, &["add", "a.txt"]));
        assert!(run_git(&work, &["commit", "-m", "first"]));
        assert!(run_git(&work, &["push", "origin", "HEAD"]));

        // Clone now; it tracks origin and is level.
        assert!(run_git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), clone.to_str().unwrap()]
        ));

        // Level: ahead 0, behind 0 (a KNOWN comparison, Some(0), not None).
        let level = ahead_behind(&clone).expect("ahead_behind level");
        assert_eq!(level.ahead, Some(0));
        assert_eq!(level.behind, Some(0));

        // Advance the remote by one commit, then update the clone's tracking ref.
        std::fs::write(work.join("a.txt"), "2\n").unwrap();
        assert!(run_git(&work, &["add", "a.txt"]));
        assert!(run_git(&work, &["commit", "-m", "second"]));
        assert!(run_git(&work, &["push", "origin", "HEAD"]));
        assert!(run_git(&clone, &["fetch", "origin"]));

        let behind = ahead_behind(&clone).expect("ahead_behind behind");
        assert_eq!(behind.ahead, Some(0), "clone has no local commits ahead");
        assert_eq!(
            behind.behind,
            Some(1),
            "clone is one commit behind upstream"
        );
    }

    #[test]
    #[ignore = "slow git-fixture tier: run with --ignored (see ci-plan.md)"]
    fn ahead_behind_none_when_upstream_deleted() {
        if !git_resolvable() {
            eprintln!("skipping ahead_behind_none_when_upstream_deleted: git missing");
            return;
        }
        let root = TempDir::new().expect("tempdir");
        let upstream = root.path().join("upstream.git");
        let work = root.path().join("work");
        let clone = root.path().join("clone");

        assert!(run_git(
            root.path(),
            &["init", "--bare", upstream.to_str().unwrap()]
        ));
        assert!(run_git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), work.to_str().unwrap()]
        ));
        assert!(run_git(&work, &["config", "user.email", "t@example.com"]));
        assert!(run_git(&work, &["config", "user.name", "T"]));
        std::fs::write(work.join("a.txt"), "1\n").unwrap();
        assert!(run_git(&work, &["add", "a.txt"]));
        assert!(run_git(&work, &["commit", "-m", "first"]));
        assert!(run_git(&work, &["push", "origin", "HEAD"]));
        assert!(run_git(
            root.path(),
            &["clone", upstream.to_str().unwrap(), clone.to_str().unwrap()]
        ));

        // Delete the remote-tracking ref the clone's upstream points at,
        // simulating a pruned/deleted upstream while config still names it.
        // Find the tracking ref (origin/main or origin/master) and remove it.
        let _ = run_git(&clone, &["update-ref", "-d", "refs/remotes/origin/main"]);
        let _ = run_git(&clone, &["update-ref", "-d", "refs/remotes/origin/master"]);

        let ab = ahead_behind(&clone).expect("ahead_behind ok");
        assert_eq!(ab.ahead, None, "deleted upstream must report ahead = None");
        assert_eq!(
            ab.behind, None,
            "deleted upstream must report behind = None"
        );
    }

    #[test]
    fn ahead_behind_none_when_detached() {
        // Detached HEAD has no branch and therefore no upstream -> None/None.
        let tmp = TempDir::new().expect("tempdir");
        let repo = git2::Repository::init(tmp.path()).expect("init");
        std::fs::write(tmp.path().join("f.txt"), "v\n").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("T", "t@example.com").unwrap();
        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "c", &tree, &[])
            .unwrap();
        // Detach onto the commit.
        repo.set_head_detached(commit_oid).unwrap();

        let ab = ahead_behind(tmp.path()).expect("ahead_behind ok");
        assert_eq!(ab.ahead, None);
        assert_eq!(ab.behind, None);
    }

    // --- RR6: head_state tells the three "no branch" cases apart --------------
    //
    // These three assertions are the whole reason the field exists. The Repos
    // table rendered one bare dash for all of them, and the two below that are
    // NOT detached were indistinguishable from each other in the database:
    // `active_branch`, `head_sha` and `last_local_commit_at` are all NULL for
    // both an unborn HEAD and a repo nothing has inspected.

    #[test]
    fn head_state_is_branch_for_an_ordinary_checkout() {
        let tmp = TempDir::new().expect("tempdir");
        init_repo_with_commit(tmp.path());

        let got = inspect(tmp.path()).expect("inspect");
        assert_eq!(got.head_state, Some(HeadState::Branch));
        assert!(got.active_branch.is_some(), "a branch checkout names it");
    }

    #[test]
    fn head_state_is_unborn_for_a_repo_with_no_commits() {
        let tmp = TempDir::new().expect("tempdir");
        git2::Repository::init(tmp.path()).expect("init");

        let got = inspect(tmp.path()).expect("inspect an unborn repo");
        assert_eq!(got.head_state, Some(HeadState::Unborn));
        // The three fields that used to be the only evidence, all absent -
        // which is exactly why they could not answer the question.
        assert_eq!(got.active_branch, None);
        assert_eq!(got.head_sha, None);
        assert_eq!(got.last_commit_at, None);
        assert!(!got.is_detached, "unborn is not detached");
    }

    #[test]
    fn head_state_is_unknown_when_head_cannot_be_read_at_all() {
        // The case this field was added for, and the one the first
        // implementation got wrong (Codex review of PRs #93-#96, finding 1).
        //
        // `repo.head()` errors on an unborn branch AND on a reference it cannot
        // read. The original code did `repo.head().ok()` and called every
        // resulting `None` "unborn", so a repository whose ref is corrupt
        // reported "no commits" - a confident, reassuring, wrong answer about
        // the user's data, which is the exact failure the column's own
        // migration note says it exists to avoid.
        //
        // The fixture: a real repo with a real commit, then its branch ref
        // overwritten with garbage. `.git/HEAD` still points at the branch, so
        // `Repository::open` succeeds and only the RESOLUTION fails.
        let tmp = TempDir::new().expect("tempdir");
        init_repo_with_commit(tmp.path());

        let head_file =
            std::fs::read_to_string(tmp.path().join(".git").join("HEAD")).expect("read .git/HEAD");
        let ref_name = head_file
            .trim()
            .strip_prefix("ref: ")
            .expect("a branch checkout writes `ref: refs/heads/...`")
            .to_string();
        // Kill any packed form too, or git2 resolves the ref from there and the
        // fixture quietly tests nothing.
        let _ = std::fs::remove_file(tmp.path().join(".git").join("packed-refs"));
        std::fs::write(
            tmp.path().join(".git").join(&ref_name),
            "this is not an object id
",
        )
        .expect("corrupt the branch ref");

        let repo = git2::Repository::open(tmp.path()).expect("open still works");
        let err = match repo.head() {
            Ok(_) => panic!("a corrupt ref must not resolve"),
            Err(e) => e,
        };
        assert_ne!(
            err.code(),
            git2::ErrorCode::UnbornBranch,
            "the fixture must fail for a reason OTHER than unborn, or it is              not testing the distinction this fix is about",
        );

        let got = inspect(tmp.path()).expect("inspect still returns");
        assert_eq!(
            got.head_state, None,
            "an unreadable HEAD is NOT an unborn repository; it is unknown",
        );
        assert_ne!(got.head_state, Some(HeadState::Unborn));
    }

    #[test]
    fn head_state_is_detached_and_beats_the_unborn_branch_of_the_test() {
        let tmp = TempDir::new().expect("tempdir");
        init_repo_with_commit(tmp.path());
        let repo = git2::Repository::open(tmp.path()).expect("open");
        let oid = repo.head().unwrap().target().unwrap();
        repo.set_head_detached(oid).unwrap();

        let got = inspect(tmp.path()).expect("inspect");
        // Detached is checked FIRST in `inspect`: a detached HEAD is also not a
        // branch, so an ordering slip would report it as Unborn - a repo with
        // commits described as having none.
        assert_eq!(got.head_state, Some(HeadState::Detached));
        assert_eq!(got.active_branch, None);
        assert!(got.head_sha.is_some(), "a detached HEAD still has a commit");
    }
}
