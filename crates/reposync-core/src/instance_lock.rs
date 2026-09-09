//! The data-directory lock: the backstop that stops two RepoSync processes from
//! sharing one database.
//!
//! Owning item: BL-NI-73 (nothing stops two RepoSync instances sharing one
//! database), in `docs/backlog.md`. Design note:
//! `_local/2026-09-09_07-10_db-lock-backstop-design.md`.
//!
//! ## Why this exists when a single-instance guard already does
//!
//! `tauri-plugin-single-instance` (see `src-tauri/src/single_instance.rs`) is the
//! first line and stays the broader one. It is keyed on the APPLICATION, so it also
//! covers the harm this module cannot see: two instances with different data
//! directories running concurrent `git` in the same working trees.
//!
//! It has a measured gap. A second launch that lands before the first instance has
//! created its message window finds no window to hand off to and proceeds as a full
//! instance. Measured on a release build 2026-09-08 with `_local/tools/race-probe.ps1`:
//! at true simultaneity both instances survived 5 times out of 10, and at 25 ms of
//! separation and beyond, never. A person double-clicking cannot hit that window. A
//! script, a login storm, or a slow disk can - and the measurement was taken on a fast
//! machine, so the untested case is the one that widens it.
//!
//! This module closes the database half of that gap, and only that half. Section 4 of
//! the design note has the full split; the short version is in [`acquire`].
//!
//! ## Why an operating-system lock rather than a row in the database
//!
//! The tempting alternative is a row holding a process id and a timestamp. It was
//! rejected because a row survives a crash: kill RepoSync from task manager, lose
//! power, or panic under the release profile's `panic = "abort"`, and the row stays
//! behind and locks the app out of its own database permanently. The usual repair is a
//! heartbeat plus a staleness timeout, which forces the app to answer "is that other
//! instance dead, or just slow?" from evidence that cannot settle it - a process id can
//! be reused, and a missed heartbeat can equally mean a paused process or a stalled
//! disk.
//!
//! The operating system already knows the answer. It holds the lock, and it releases it
//! when the process dies, for any reason, with nothing left to guess.
//!
//! `PRAGMA locking_mode=EXCLUSIVE` was rejected for a different reason: it would change
//! write-ahead-log and recovery behaviour that `db::init_pool_with_recovery` depends on,
//! and buying this guarantee by altering the recovery path is a bad trade when a
//! separate file buys the same guarantee and touches nothing.

use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

/// A held claim on a data directory, released when this value is dropped.
///
/// **The caller must keep it alive for as long as the process intends to use the
/// database.** Dropping it releases the lock and lets a second instance in. It is
/// deliberately not `Clone` and holds its `File` privately so the only way to release
/// early is to drop it on purpose.
#[derive(Debug)]
pub struct InstanceLock {
    /// The open, locked handle. Never read or written - the lock is the whole point,
    /// and the operating system drops it with the handle.
    _file: File,
    path: PathBuf,
}

impl InstanceLock {
    /// The lock file this claim is holding, for diagnostics.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

/// Why a data directory could not be claimed.
#[derive(Debug)]
pub enum AcquireError {
    /// Another process holds the lock. This is the case the module exists for, and
    /// the caller should stand down rather than treat it as a failure.
    AlreadyRunning { path: PathBuf },
    /// The lock file itself could not be created or locked - a permission problem, a
    /// read-only volume, a filesystem that does not support locking.
    ///
    /// **This is not a reason to refuse to start.** See [`acquire`].
    Unavailable { path: PathBuf, cause: String },
}

/// Claim the data directory for this process.
///
/// Creates `reposync.lock` beside the database and takes an exclusive
/// operating-system lock on it. Returns an [`InstanceLock`] that must be kept alive
/// for the life of the process.
///
/// Call this BEFORE opening the database pool. Not for correctness of the lock itself,
/// but because the caller's response to [`AcquireError::AlreadyRunning`] is to exit,
/// and doing that before the pool opens means the losing instance never touches the
/// database at all.
///
/// ## What this does not cover
///
/// The lock is keyed on the DATA DIRECTORY, which is narrower than the
/// single-instance plugin's application-wide key. It cannot see two instances pointed
/// at different data directories, and those can still run concurrent `git` in the same
/// working trees if the same repositories are registered in both.
///
/// That is not a regression and not an argument for widening this: the plugin remains
/// the first line and the broader guard, and in the race this backstop exists for, both
/// processes are the same user on the same machine and therefore share a data
/// directory. This is a second line for the database harm, not a replacement for the
/// first line.
///
/// ## Why a filesystem failure is not fatal
///
/// [`AcquireError::Unavailable`] means the lock could not be established at all. The
/// correct response is to log it and start anyway, leaving the plugin as the only
/// guard - which is exactly the protection the app shipped with before this module
/// existed. Refusing to launch would turn a condition the app currently tolerates into
/// one that bricks the install, on a machine nobody can reproduce. The same reasoning
/// was applied to the window-startup failures in the binary smoke gate work.
pub fn acquire(paths: &crate::paths::AppPaths) -> Result<InstanceLock, AcquireError> {
    let path = paths.lock_path();

    // The data dir must exist before a file can be created in it. `init_pool_with_recovery`
    // does this too, but it has not run yet - this deliberately comes first.
    if let Err(e) = std::fs::create_dir_all(paths.data_dir()) {
        return Err(AcquireError::Unavailable {
            path,
            cause: format!("could not create the data directory: {e}"),
        });
    }

    // `create` truncates, which is fine and intended: the file's CONTENTS carry no
    // meaning. Writing a process id into it would invite exactly the stale-owner
    // guessing this design rejects, so the file stays empty and the lock is the only
    // signal.
    let file = match File::create(&path) {
        Ok(f) => f,
        Err(e) => {
            return Err(AcquireError::Unavailable {
                path,
                cause: format!("could not open the lock file: {e}"),
            })
        }
    };

    match file.try_lock() {
        Ok(()) => Ok(InstanceLock { _file: file, path }),
        Err(TryLockError::WouldBlock) => Err(AcquireError::AlreadyRunning { path }),
        Err(TryLockError::Error(e)) => Err(AcquireError::Unavailable {
            path,
            cause: format!("could not lock the lock file: {e}"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::AppPaths;
    use tempfile::TempDir;

    fn paths_in(dir: &TempDir) -> AppPaths {
        AppPaths::new(dir.path().join("RepoSync"))
    }

    #[test]
    fn the_first_caller_gets_the_lock() {
        let dir = TempDir::new().expect("tempdir");
        let paths = paths_in(&dir);

        let lock = acquire(&paths).expect("the first acquire must succeed");
        assert_eq!(lock.path(), paths.lock_path());
        assert!(lock.path().exists(), "the lock file must exist on disk");
    }

    #[test]
    fn a_second_caller_is_told_the_app_is_already_running() {
        let dir = TempDir::new().expect("tempdir");
        let paths = paths_in(&dir);

        let _first = acquire(&paths).expect("the first acquire must succeed");

        // This is the whole point of the module. It runs in ONE process because the
        // standard library's file lock is per-handle on Windows and per-open-file
        // description on unix, so a second handle contends with the first exactly as a
        // second process would. That is what makes the mechanism testable here at all;
        // the cross-PROCESS race is proved separately by `_local/tools/race-probe.ps1`.
        match acquire(&paths) {
            Err(AcquireError::AlreadyRunning { path }) => {
                assert_eq!(path, paths.lock_path());
            }
            Err(other) => panic!("expected AlreadyRunning, got {other:?}"),
            Ok(_) => panic!("two instances both claimed the same data directory"),
        }
    }

    #[test]
    fn dropping_the_lock_lets_the_next_instance_in() {
        let dir = TempDir::new().expect("tempdir");
        let paths = paths_in(&dir);

        let first = acquire(&paths).expect("the first acquire must succeed");
        drop(first);

        // A restart after a clean exit must not be locked out by its own leftover file.
        // The file is deliberately never deleted - deleting it would open a window
        // where a third process creates a fresh one and locks that instead, and the
        // operating system already releases the lock on drop.
        acquire(&paths).expect("after the holder drops, the directory is claimable again");
        assert!(
            paths.lock_path().exists(),
            "the lock file is left in place on purpose; only the LOCK is released"
        );
    }

    #[test]
    fn two_data_directories_do_not_contend() {
        let dir_a = TempDir::new().expect("tempdir a");
        let dir_b = TempDir::new().expect("tempdir b");

        // The lock is keyed on the data directory, so a developer instance pointed at
        // its own directory is not blocked by an installed one. The single-instance
        // plugin is what stops that pairing, deliberately and at a different layer.
        let _a = acquire(&paths_in(&dir_a)).expect("directory a");
        let _b = acquire(&paths_in(&dir_b)).expect("directory b");
    }

    #[test]
    fn a_data_dir_that_cannot_be_created_is_reported_as_unavailable_not_as_running() {
        let dir = TempDir::new().expect("tempdir");
        // A FILE where the data directory should be: create_dir_all cannot succeed.
        let blocked = dir.path().join("RepoSync");
        std::fs::write(&blocked, b"not a directory").expect("write the blocker");

        let paths = AppPaths::new(blocked);
        match acquire(&paths) {
            Err(AcquireError::Unavailable { .. }) => {}
            // The distinction is load-bearing: the caller EXITS on AlreadyRunning and
            // CONTINUES on Unavailable, so misclassifying a broken filesystem as a
            // running instance would stop the app from launching at all.
            other => panic!("expected Unavailable, got {other:?}"),
        }
    }
}
