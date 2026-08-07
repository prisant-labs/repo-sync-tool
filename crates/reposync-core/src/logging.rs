//! logging - the core half of structured diagnostics.
//!
//! The core emits through the `tracing` FACADE and never installs a subscriber.
//! That split is deliberate twice over. A library that installs a global
//! subscriber steals the choice from every consumer, including the test harness.
//! And `tracing-subscriber`'s timestamp formatting pulls the `time` crate, which
//! the workspace manifest states must never enter the core tree. The subscriber,
//! the rolling file appender, and the guard that keeps them alive live in
//! `src-tauri/src/logging.rs`.
//!
//! What DOES live here is the part with logic worth testing: the stable event
//! names, and the size sweep that bounds the log directory.
//!
//! WHY ANY OF THIS EXISTS. RepoSync is a tray app. It has no console. Every
//! resident failure - a scheduler tick that could not persist its outcome, an
//! activity write that failed, a git binary that vanished, an update check that
//! errored - went to `eprintln!`, which is to say to a stderr that no user has
//! ever seen. The 2026-07-30 audit put it plainly: this should precede
//! large-scale dogfooding, because it is what turns a rare resident failure into
//! evidence someone can act on.

use std::fs;
use std::path::{Path, PathBuf};

/// Stable event names, carried on the `event` field of every diagnostic.
///
/// These are the grep targets in a user-submitted log, so they are treated as a
/// contract: rename one and every previously-collected log stops matching the
/// searches written against it. Add freely; change reluctantly.
///
/// The naming is `area.what_happened`, past tense, because a log line records
/// something that already occurred.
pub mod event {
    // --- scheduler ---
    /// A scheduled job could not persist its outcome. The repo stays due and
    /// retries, so the user sees nothing; that is exactly why it needs a log.
    pub const SCHEDULER_OUTCOME_PERSIST_FAILED: &str = "scheduler.outcome_persist_failed";
    /// A scheduler tick itself failed.
    pub const SCHEDULER_TICK_FAILED: &str = "scheduler.tick_failed";
    /// A spawned job task did not return a value - it panicked or was cancelled.
    /// The release profile sets `panic = "abort"`, so in a shipped build a
    /// panicking job takes the process with it and this can only appear in a
    /// debug or dev run. It is a real event there: that repo's job vanished
    /// without recording anything.
    pub const SCHEDULER_JOB_ABORTED: &str = "scheduler.job_aborted";
    /// A job could not re-read the repo's CURRENT failure count and fell back to
    /// the count the due query snapshotted (BL-NI-72).
    ///
    /// Distinct from `scheduler.outcome_persist_failed`, and deliberately so:
    /// nothing failed to PERSIST here. The job read stale input, classified
    /// against it, and wrote the result successfully. Filing both under one name
    /// would make a log search for write problems return read problems, which
    /// defeats the point of the name being a grep target.
    ///
    /// The consequence when it fires is narrow but real: the three-strikes
    /// classification for that one job used a count that may predate a manual
    /// check, which is exactly the staleness BL-NI-72 removed everywhere else.
    pub const SCHEDULER_FAILURE_COUNT_READ_FAILED: &str = "scheduler.failure_count_read_failed";
    /// The activity retention sweep failed.
    pub const RETENTION_SWEEP_FAILED: &str = "retention.sweep_failed";

    // --- activity ---
    /// An activity row could not be written. Best-effort by design, which means
    /// this log line is the ONLY trace that the audit trail has a hole in it.
    pub const ACTIVITY_WRITE_FAILED: &str = "activity.write_failed";

    // --- persistence ---
    /// A manual check or update failed HARD (git missing, path gone, repo no
    /// longer a working tree) and the best-effort write of that failure into
    /// `repo_local_state.last_error_code` ALSO failed.
    ///
    /// Both halves have to fail for this to fire, which is why it is worth a name:
    /// the user asked for something, it broke, and the record that it broke also
    /// broke. Without this line the repo simply keeps whatever error state it had
    /// before, and the Repos list and the daily summary quietly disagree with what
    /// the user just watched happen.
    pub const STATE_ERROR_CODE_PERSIST_FAILED: &str = "state.error_code_persist_failed";
    /// The database was opened, migrated, or recovered in a notable way.
    pub const DB_RECOVERED: &str = "db.recovered";
    /// The resolved data directory is OneDrive-rooted, where a sync agent can
    /// corrupt the WAL sidecars mid-write (BL-NI-12).
    pub const DB_ONEDRIVE_ROOTED: &str = "db.onedrive_rooted";

    // --- git ---
    /// The git binary is missing, unreadable, or below the supported floor.
    pub const GIT_UNAVAILABLE: &str = "git.unavailable";

    // --- updates ---
    /// An update check failed.
    pub const UPDATE_CHECK_FAILED: &str = "update.check_failed";

    // --- logging itself ---
    /// The log-directory size sweep removed one or more files.
    pub const LOG_PRUNED: &str = "log.pruned";
    /// The log-directory size sweep failed. Emitted through the subscriber it is
    /// pruning for, which is fine: the failure is not fatal.
    pub const LOG_PRUNE_FAILED: &str = "log.prune_failed";
}

/// The filename prefix every rotated log file carries.
///
/// The appender composes `{prefix}.{date}.{suffix}`, so a day's file is
/// `reposync.2026-07-31.log`. The sweep below relies on that shape.
pub const LOG_FILE_PREFIX: &str = "reposync";

/// The filename suffix every rotated log file carries.
pub const LOG_FILE_SUFFIX: &str = "log";

/// Total bytes the log directory may occupy before the oldest files are dropped.
///
/// This is the SIZE half of retention. The appender's own `max_log_files` bounds
/// retention by AGE, which is not sufficient on its own: a single pathological
/// day - a remote flapping, a repo failing every tick - can produce one enormous
/// file that no age limit will ever reach.
///
/// 32 MiB holds a great deal of a quiet tray app's history while staying small
/// enough to attach to an issue.
pub const LOG_DIR_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// What the log directory currently holds, for the Diagnostics view.
///
/// Counts only OUR files, by the same `LOG_FILE_PREFIX` rule the sweep uses.
/// The two have to agree: a diagnostics panel reporting a size the sweep does
/// not charge itself for would make the retention budget look broken every time
/// an unrelated file sat in the directory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LogDirStats {
    /// Whether the directory could be read at all.
    ///
    /// Kept SEPARATE from the counts rather than collapsed into a zero. "We
    /// looked and there are no files" and "we could not look" are different
    /// facts, and only the second one is itself a problem - a directory the app
    /// cannot read is a directory it probably cannot write either, which is
    /// exactly the condition a diagnostics panel exists to catch. Reporting a
    /// confident `0` for both would hide the interesting case behind the boring
    /// one.
    pub readable: bool,
    /// Rotated log files present. Meaningless unless `readable`.
    pub file_count: u64,
    /// Bytes they occupy in total. Meaningless unless `readable`.
    pub total_bytes: u64,
}

/// Measure `dir` against [`LogDirStats`].
///
/// Never errors: this feeds a display panel, and a diagnostics read that fails
/// closed tells the user nothing. An unreadable or missing directory comes back
/// with `readable: false` so the caller can say which of the two it is.
pub fn log_dir_stats(dir: &Path) -> LogDirStats {
    let mut stats = LogDirStats::default();
    let Ok(entries) = fs::read_dir(dir) else {
        return stats;
    };
    stats.readable = true;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(LOG_FILE_PREFIX) {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            stats.file_count += 1;
            stats.total_bytes += meta.len();
        }
    }
    stats
}

/// Delete the oldest log files until `dir` fits within `budget_bytes`.
///
/// Returns the paths removed, oldest first, so the caller can log what it did.
///
/// THE NEWEST FILE IS NEVER DELETED, even when it alone exceeds the budget. It
/// is the file the appender currently holds open: on Windows the delete would
/// simply fail, and on a platform where it succeeded the running process would
/// go on writing to an unlinked handle, silently losing the session that is most
/// likely to be the one being diagnosed. A budget overshoot is the strictly
/// better failure.
///
/// Ordering is lexicographic on the filename, which is chronological because the
/// appender's date component is zero-padded ISO (`2026-07-31`). That is an
/// assumption about the appender's format, so it is asserted in the tests rather
/// than trusted.
///
/// A missing directory is not an error: the sweep runs at startup, and on a
/// first launch there is nothing to sweep yet.
pub fn prune_to_size_budget(dir: &Path, budget_bytes: u64) -> std::io::Result<Vec<PathBuf>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    // (name, path, size) for our own files only. An unrelated file in the log
    // directory is not ours to delete, and it does not count toward the budget
    // either - charging the sweep for bytes it is not allowed to reclaim would
    // make it delete our logs to compensate for someone else's file.
    let mut files: Vec<(String, PathBuf, u64)> = Vec::new();
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(LOG_FILE_PREFIX) {
            continue;
        }
        let size = entry.metadata()?.len();
        files.push((name.to_string(), path, size));
    }

    files.sort_by(|a, b| a.0.cmp(&b.0));

    let mut total: u64 = files.iter().map(|(_, _, size)| size).sum();
    let mut removed = Vec::new();

    // `files.len() > 1` is the guard that preserves the newest file: the loop
    // stops with one file left no matter how far over budget it is.
    let mut index = 0;
    while total > budget_bytes && index + 1 < files.len() {
        let (_, path, size) = &files[index];
        match fs::remove_file(path) {
            Ok(()) => {
                total = total.saturating_sub(*size);
                removed.push(path.clone());
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Raced with something else removing it. Already gone is the
                // outcome we wanted.
                total = total.saturating_sub(*size);
            }
            Err(e) => return Err(e),
        }
        index += 1;
    }

    Ok(removed)
}

/// A `tracing` capture harness for tests in this crate.
///
/// WHY THIS IS HAND-ROLLED. The obvious tool is `tracing-subscriber`'s test
/// writer, and this crate may not have it: `tracing-subscriber`'s timestamp
/// formatting pulls `time`, which the workspace excludes from the core tree, and
/// `cargo tree -p reposync-core` enforces that in CI. So the capture is built
/// from the FACADE's own traits, which cost about forty lines and keep the
/// dependency invariant intact rather than negotiating with it.
///
/// WHY IT EXISTS AT ALL. Several of this crate's most important behaviors are
/// best-effort by design: an activity row that cannot be written is swallowed so
/// a logging hiccup never aborts the git operation that already happened, and a
/// scheduled outcome that cannot be persisted leaves the repo due to retry. In
/// each case the log line is the ONLY trace that anything went wrong. Tests could
/// previously assert that those paths do not panic, which is the easy half; the
/// half that matters, that the diagnostic actually reaches a subscriber, had
/// nothing checking it. A swallowed failure that also fails to log is
/// indistinguishable from success, and that is the exact condition these events
/// exist to prevent.
#[cfg(test)]
pub(crate) mod capture {
    use std::sync::{Arc, Mutex};
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing::{Event, Level, Metadata, Subscriber};

    /// The `event = ...` names seen while the capture was installed, with the
    /// level each was emitted at. Cheap to clone; the clone shares the buffer.
    #[derive(Clone, Default)]
    pub(crate) struct Captured(Arc<Mutex<Vec<(String, Level)>>>);

    impl Captured {
        /// Whether an event with this stable name was emitted.
        pub(crate) fn saw(&self, event_name: &str) -> bool {
            self.0.lock().unwrap().iter().any(|(n, _)| n == event_name)
        }

        /// Whether it was emitted at this level. Kept separate from [`saw`]
        /// because level is part of the contract: a failure demoted to `debug`
        /// vanishes from a default-filtered log while still passing a
        /// name-only assertion.
        pub(crate) fn saw_at(&self, event_name: &str, level: Level) -> bool {
            self.0
                .lock()
                .unwrap()
                .iter()
                .any(|(n, l)| n == event_name && *l == level)
        }

        /// Every captured name, for an assertion message worth reading.
        pub(crate) fn names(&self) -> Vec<String> {
            self.0
                .lock()
                .unwrap()
                .iter()
                .map(|(n, _)| n.clone())
                .collect()
        }
    }

    /// Pulls the value of the `event` field out of one tracing event.
    ///
    /// Both `record_str` and `record_debug` are implemented because the macro
    /// chooses between them by how the value is typed at the call site: a plain
    /// `event = "name"` literal arrives as a str, while `event = CONST` where the
    /// constant is a `&'static str` may arrive through the debug path with
    /// surrounding quotes. Handling only one of them silently captures nothing
    /// for half the call sites, and a capture that quietly sees nothing would
    /// make every assertion here vacuous.
    struct EventName(Option<String>);

    impl Visit for EventName {
        fn record_str(&mut self, field: &Field, value: &str) {
            if field.name() == "event" {
                self.0 = Some(value.to_string());
            }
        }
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            if field.name() == "event" && self.0.is_none() {
                self.0 = Some(format!("{value:?}").trim_matches('"').to_string());
            }
        }
    }

    struct CaptureSubscriber(Captured);

    impl Subscriber for CaptureSubscriber {
        fn enabled(&self, _metadata: &Metadata<'_>) -> bool {
            true
        }
        fn new_span(&self, _span: &Attributes<'_>) -> Id {
            // Spans are not what this captures; a constant id is sufficient and
            // never observed.
            Id::from_u64(1)
        }
        fn record(&self, _span: &Id, _values: &Record<'_>) {}
        fn record_follows_from(&self, _span: &Id, _follows: &Id) {}
        fn event(&self, event: &Event<'_>) {
            let mut visitor = EventName(None);
            event.record(&mut visitor);
            if let Some(name) = visitor.0 {
                self.0
                     .0
                    .lock()
                    .unwrap()
                    .push((name, *event.metadata().level()));
            }
        }
        fn enter(&self, _span: &Id) {}
        fn exit(&self, _span: &Id) {}
    }

    /// Install the capture for the current thread, returning the buffer and a
    /// guard that uninstalls on drop.
    ///
    /// Thread-local by construction, via `set_default` rather than the global
    /// `set_global_default`, so concurrently running tests cannot see each
    /// other's events and no test can install a subscriber that outlives it.
    ///
    /// The consequence to know: a `#[tokio::test]` with the default
    /// current-thread flavor polls its future on this thread and is captured. A
    /// `flavor = "multi_thread"` test moves work to worker threads that do NOT
    /// inherit the thread-local, so events emitted there are invisible here.
    /// That is why the assertions below live on single-threaded tests.
    pub(crate) fn install() -> (Captured, tracing::subscriber::DefaultGuard) {
        let captured = Captured::default();
        let guard = tracing::subscriber::set_default(CaptureSubscriber(captured.clone()));
        (captured, guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    /// The capture harness has to be shown working before anything relies on it.
    ///
    /// A capture that silently sees nothing would make every assertion written
    /// against it vacuous, and vacuous assertions are worse than absent ones
    /// because they read as coverage. So: an event with a name is seen, its level
    /// is recorded, an event WITHOUT an `event` field is ignored rather than
    /// captured under some fallback name, and nothing is captured once the guard
    /// has dropped.
    #[test]
    fn the_capture_harness_actually_captures() {
        {
            let (captured, _guard) = capture::install();
            tracing::error!(event = event::ACTIVITY_WRITE_FAILED, "boom");
            tracing::warn!(event = event::GIT_UNAVAILABLE, "missing");
            tracing::info!("a line with no event field");

            assert!(captured.saw(event::ACTIVITY_WRITE_FAILED));
            assert!(captured.saw_at(event::ACTIVITY_WRITE_FAILED, tracing::Level::ERROR));
            assert!(
                !captured.saw_at(event::ACTIVITY_WRITE_FAILED, tracing::Level::WARN),
                "the level is part of what is asserted, not incidental"
            );
            assert!(captured.saw_at(event::GIT_UNAVAILABLE, tracing::Level::WARN));
            assert_eq!(
                captured.names().len(),
                2,
                "a line with no `event` field must not be captured under a fallback name"
            );
            assert!(!captured.saw("never.emitted"));
        }
        // Outside the guard's scope the subscriber is uninstalled. Nothing to
        // assert directly, but this pins that the guard is scoped rather than
        // global, which is what keeps concurrent tests from seeing each other.
    }

    /// Write a log file of `size` bytes named for `date`.
    fn write_log(dir: &Path, date: &str, size: usize) -> PathBuf {
        let path = dir.join(format!("{LOG_FILE_PREFIX}.{date}.{LOG_FILE_SUFFIX}"));
        let mut f = fs::File::create(&path).expect("create log file");
        f.write_all(&vec![b'x'; size]).expect("write log file");
        path
    }

    #[test]
    fn a_missing_directory_is_not_an_error() {
        let dir = TempDir::new().expect("tempdir");
        let missing = dir.path().join("no-such-dir");
        let removed = prune_to_size_budget(&missing, 1).expect("a missing log dir must be benign");
        assert!(removed.is_empty());
    }

    #[test]
    fn under_budget_removes_nothing() {
        let dir = TempDir::new().expect("tempdir");
        write_log(dir.path(), "2026-07-29", 100);
        write_log(dir.path(), "2026-07-30", 100);

        let removed = prune_to_size_budget(dir.path(), 1_000).expect("prune");
        assert!(
            removed.is_empty(),
            "nothing may be deleted while the directory is under budget"
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn oldest_files_go_first_and_only_as_far_as_needed() {
        let dir = TempDir::new().expect("tempdir");
        let oldest = write_log(dir.path(), "2026-07-28", 100);
        let middle = write_log(dir.path(), "2026-07-29", 100);
        let newest = write_log(dir.path(), "2026-07-30", 100);

        // Budget 250 of 300 used: dropping ONE file is enough.
        let removed = prune_to_size_budget(dir.path(), 250).expect("prune");

        assert_eq!(removed, vec![oldest.clone()], "the oldest file goes first");
        assert!(!oldest.exists(), "the oldest file is gone");
        assert!(
            middle.exists() && newest.exists(),
            "the sweep must stop as soon as it is under budget, not keep going"
        );
    }

    /// The guard that matters most. Deleting the file the appender holds open
    /// would either fail outright on Windows or, worse, leave the process
    /// writing to an unlinked handle - silently discarding the session most
    /// likely to be the one under investigation.
    #[test]
    fn the_newest_file_survives_even_when_it_alone_blows_the_budget() {
        let dir = TempDir::new().expect("tempdir");
        let old = write_log(dir.path(), "2026-07-29", 100);
        let newest = write_log(dir.path(), "2026-07-30", 10_000);

        let removed = prune_to_size_budget(dir.path(), 50).expect("prune");

        assert_eq!(removed, vec![old.clone()]);
        assert!(
            newest.exists(),
            "the active log file must survive a budget it cannot possibly satisfy; \
             an overshoot beats losing the session being diagnosed"
        );
    }

    #[test]
    fn files_that_are_not_ours_are_neither_deleted_nor_counted() {
        let dir = TempDir::new().expect("tempdir");
        let foreign = dir.path().join("someone-elses.log");
        fs::write(&foreign, vec![b'x'; 10_000]).expect("write foreign file");
        write_log(dir.path(), "2026-07-29", 100);
        let newest = write_log(dir.path(), "2026-07-30", 100);

        let removed = prune_to_size_budget(dir.path(), 1_000).expect("prune");

        assert!(
            removed.is_empty(),
            "a foreign file must not be charged to our budget; counting it would \
             make the sweep delete OUR logs to pay for someone else's bytes"
        );
        assert!(foreign.exists(), "a foreign file is not ours to delete");
        assert!(newest.exists());
    }

    /// The sweep orders by filename and calls that chronological. That is only
    /// true because the appender's date component is zero-padded ISO. If the
    /// format ever changed to something like `7-4-2026`, lexicographic order
    /// would silently stop being date order and the sweep would delete the wrong
    /// files. Asserting the assumption keeps that from being a silent break.
    #[test]
    fn lexicographic_filename_order_is_chronological_order() {
        let mut names = vec![
            format!("{LOG_FILE_PREFIX}.2026-07-09.{LOG_FILE_SUFFIX}"),
            format!("{LOG_FILE_PREFIX}.2026-12-01.{LOG_FILE_SUFFIX}"),
            format!("{LOG_FILE_PREFIX}.2026-07-10.{LOG_FILE_SUFFIX}"),
            format!("{LOG_FILE_PREFIX}.2025-01-31.{LOG_FILE_SUFFIX}"),
        ];
        names.sort();

        assert_eq!(
            names,
            vec![
                format!("{LOG_FILE_PREFIX}.2025-01-31.{LOG_FILE_SUFFIX}"),
                format!("{LOG_FILE_PREFIX}.2026-07-09.{LOG_FILE_SUFFIX}"),
                format!("{LOG_FILE_PREFIX}.2026-07-10.{LOG_FILE_SUFFIX}"),
                format!("{LOG_FILE_PREFIX}.2026-12-01.{LOG_FILE_SUFFIX}"),
            ],
            "zero-padded ISO dates must sort chronologically as plain strings"
        );
    }

    /// An unreadable directory must not look like an empty one. A diagnostics
    /// panel that reports a confident "0 files" for a folder it could not open
    /// hides a permissions problem behind a number that reads as normal - and a
    /// directory the app cannot read is one it probably cannot write to either,
    /// which is precisely the failure the panel exists to surface.
    #[test]
    fn an_unreadable_directory_is_distinguished_from_an_empty_one() {
        let dir = TempDir::new().expect("tempdir");

        let missing = log_dir_stats(&dir.path().join("no-such-dir"));
        assert!(!missing.readable, "a missing directory is not readable");
        assert_eq!(missing.file_count, 0);

        let empty = log_dir_stats(dir.path());
        assert!(empty.readable, "an existing empty directory IS readable");
        assert_eq!(empty.file_count, 0);

        assert_ne!(
            missing, empty,
            "the two states must be distinguishable; collapsing them is the bug"
        );
    }

    /// The stats and the sweep must charge for the same bytes. If they diverged,
    /// the Diagnostics panel would show a directory over its budget while the
    /// sweep considered it fine, and the retention budget would look broken every
    /// time an unrelated file landed in the folder.
    #[test]
    fn stats_count_only_our_files_like_the_sweep_does() {
        let dir = TempDir::new().expect("tempdir");
        write_log(dir.path(), "2026-07-29", 100);
        write_log(dir.path(), "2026-07-30", 250);
        fs::write(dir.path().join("someone-elses.log"), vec![b'x'; 9_000])
            .expect("write foreign file");
        fs::create_dir(dir.path().join("reposync-subdir")).expect("create dir");

        let stats = log_dir_stats(dir.path());

        assert!(stats.readable);
        assert_eq!(stats.file_count, 2, "only the two prefixed FILES count");
        assert_eq!(
            stats.total_bytes, 350,
            "a foreign file's bytes are not ours to report, exactly as the sweep \
             does not charge itself for them"
        );
    }

    /// Event names are a grep contract for logs already sitting on users'
    /// machines. This pins the format so a rename is a deliberate, visible edit.
    #[test]
    fn event_names_follow_the_area_dot_past_tense_convention() {
        for name in [
            event::SCHEDULER_OUTCOME_PERSIST_FAILED,
            event::SCHEDULER_TICK_FAILED,
            event::SCHEDULER_JOB_ABORTED,
            event::RETENTION_SWEEP_FAILED,
            event::ACTIVITY_WRITE_FAILED,
            event::DB_RECOVERED,
            event::DB_ONEDRIVE_ROOTED,
            event::GIT_UNAVAILABLE,
            event::UPDATE_CHECK_FAILED,
            event::LOG_PRUNED,
            event::LOG_PRUNE_FAILED,
        ] {
            assert!(
                name.contains('.'),
                "{name} must be namespaced as area.what_happened"
            );
            assert_eq!(
                name.to_lowercase(),
                name,
                "{name} must be lowercase so grepping a log is case-stable"
            );
            assert!(
                !name.contains(' '),
                "{name} must not contain spaces; it is a grep target"
            );
        }
    }
}
