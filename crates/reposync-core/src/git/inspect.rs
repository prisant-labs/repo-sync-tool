//! git::inspect - owned by E-03 (cheap reads via git2).
//!
//! The ONE git2 read in the tracer. Pure, synchronous, never touches the
//! network and never mutates the repository.

use std::path::Path;

use git2::{Repository, StatusOptions};

use crate::error::AppError;
use crate::git::InspectResult;

/// Inspect a repository's local state using git2.
///
/// Returns [`AppError::NotARepo`] if `repo_path` is not a git repository.
pub fn inspect(repo_path: &Path) -> Result<InspectResult, AppError> {
    let repo =
        Repository::open(repo_path).map_err(|e| AppError::not_a_repo_from_git2(repo_path, &e))?;

    let is_detached = repo.head_detached().unwrap_or(false);

    // HEAD may be unborn (a fresh repo with no commits). Treat that as "no
    // commit / no branch" rather than an error.
    let head = repo.head().ok();

    let head_sha = head
        .as_ref()
        .and_then(|h| h.peel_to_commit().ok())
        .map(|c| c.id().to_string());

    let active_branch = head.as_ref().and_then(|h| {
        if h.is_branch() {
            h.shorthand().ok().map(|s| s.to_string())
        } else {
            None
        }
    });

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
    let branch = repo
        .find_branch(shorthand, git2::BranchType::Local)
        .ok()?;
    let upstream = branch.upstream().ok()?;
    upstream.name().ok().flatten().map(|s| s.to_string())
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
        let sig =
            git2::Signature::now("T", "t@example.com").expect("sig");
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
    fn non_repo_is_not_a_repo_error() {
        let tmp = TempDir::new().expect("tempdir");
        let err = inspect(tmp.path()).expect_err("should fail");
        assert!(matches!(err, AppError::NotARepo { .. }));
    }
}
