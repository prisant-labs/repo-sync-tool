//! logging - the shell half of structured diagnostics.
//!
//! `reposync-core` emits through the `tracing` facade; this module decides where
//! those events go. The split is the ecosystem's convention (libraries emit,
//! binaries subscribe) and it is also load-bearing here: `tracing-subscriber`
//! pulls the `time` crate, which the workspace manifest states must never enter
//! the core tree.
//!
//! Output is a rotating file under the app data directory, because the thing
//! being replaced is `eprintln!` in a `windows_subsystem = "windows"` binary -
//! a stderr that is not merely unread but, in a release build, not attached to
//! anything at all.

use std::str::FromStr;

use reposync_core::logging as core_logging;
use reposync_core::paths::AppPaths;
use tracing::level_filters::LevelFilter;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::fmt::time::UtcTime;

/// Daily files the appender retains. The AGE half of retention; the SIZE half is
/// [`core_logging::LOG_DIR_MAX_BYTES`], swept at startup.
///
/// Fourteen days is chosen against how this app actually fails. The failures
/// worth diagnosing here are INTERMITTENT - a scheduler tick that misbehaves
/// once a week, a remote that only rejects at renewal - so a window that spans
/// two of them beats a tighter one.
pub const MAX_LOG_FILES: usize = 14;

/// Environment variable overriding the log level, e.g. `REPOSYNC_LOG=debug`.
///
/// Deliberately not `RUST_LOG`: that name is shared with every other Rust tool
/// on the machine, and inheriting someone else's debugging session would turn a
/// user's log into noise at exactly the wrong moment.
pub const LOG_LEVEL_ENV: &str = "REPOSYNC_LOG";

/// The level used when [`LOG_LEVEL_ENV`] is unset or unparseable.
pub const DEFAULT_LEVEL: LevelFilter = LevelFilter::INFO;

/// Keeps the background log writer alive.
///
/// THIS MUST BE HELD FOR THE LIFE OF THE PROCESS. `tracing_appender::non_blocking`
/// moves writing onto a worker thread, and dropping its guard shuts that worker
/// down: logging then becomes a silent no-op, and any events still buffered are
/// lost. That failure has the shape this project has already been bitten by once
/// with the white screen - total, silent, and invisible to every automated gate,
/// because the logging calls still compile and still "succeed".
///
/// Hence `#[must_use]`: binding this to `_` is a compile-time warning rather than
/// a runtime mystery. It is held in [`crate::run`]'s own scope, which lives
/// exactly as long as the app does, so its `Drop` also flushes on the way out.
#[must_use = "dropping the LogGuard shuts down the log writer and silently \
              stops all logging; hold it for the life of the process"]
pub struct LogGuard {
    _worker: WorkerGuard,
}

/// Read the level from [`LOG_LEVEL_ENV`], falling back to [`DEFAULT_LEVEL`].
///
/// An unparseable value falls back rather than failing: a typo in a debugging
/// env var must not be the reason an app refuses to log.
fn level_from_env(raw: Option<&str>) -> LevelFilter {
    raw.map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| LevelFilter::from_str(s).ok())
        .unwrap_or(DEFAULT_LEVEL)
}

/// Install the global subscriber and return the guard that keeps it running.
///
/// Ordering matters: the size sweep runs BEFORE the appender opens today's file,
/// so the sweep never contends with a handle this process is about to hold.
///
/// Returns `Err` rather than panicking. Failing to open a log file is a reason
/// to run without logs, never a reason not to start - the same best-effort
/// principle the activity writer follows, and for the same reason: the
/// diagnostic must not become the outage.
pub fn init(paths: &AppPaths) -> Result<LogGuard, String> {
    paths
        .ensure_dirs()
        .map_err(|e| format!("could not create the log directory: {e}"))?;

    let dir = paths.log_dir();

    // Bound the directory before opening anything. A failure here is reported
    // and then ignored: an oversized log directory is a far smaller problem than
    // no logging at all.
    let pruned = core_logging::prune_to_size_budget(&dir, core_logging::LOG_DIR_MAX_BYTES);

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(core_logging::LOG_FILE_PREFIX)
        .filename_suffix(core_logging::LOG_FILE_SUFFIX)
        .max_log_files(MAX_LOG_FILES)
        .build(&dir)
        .map_err(|e| {
            format!(
                "could not open the rolling log file in {}: {e}",
                dir.display()
            )
        })?;

    let (writer, worker) = tracing_appender::non_blocking(appender);

    let level = level_from_env(std::env::var(LOG_LEVEL_ENV).ok().as_deref());

    // `try_init` rather than `init`: a second call must not abort the app. In
    // practice only a test harness does this, but a panic during startup of a
    // tray app is invisible and unrecoverable.
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_timer(UtcTime::rfc_3339())
        .with_max_level(level)
        .with_target(true)
        .try_init()
        .map_err(|e| format!("a tracing subscriber was already installed: {e}"))?;

    // Now that the subscriber exists, report what the sweep did. Doing this
    // earlier would have written into the void.
    match pruned {
        Ok(removed) if !removed.is_empty() => tracing::info!(
            event = core_logging::event::LOG_PRUNED,
            removed = removed.len(),
            budget_bytes = core_logging::LOG_DIR_MAX_BYTES,
            "pruned old log files to fit the size budget"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!(
            event = core_logging::event::LOG_PRUNE_FAILED,
            error = %e,
            "could not prune the log directory; continuing"
        ),
    }

    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        level = %level,
        log_dir = %dir.display(),
        max_log_files = MAX_LOG_FILES,
        "RepoSync starting"
    );

    Ok(LogGuard { _worker: worker })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_or_empty_level_falls_back_to_the_default() {
        assert_eq!(level_from_env(None), DEFAULT_LEVEL);
        assert_eq!(level_from_env(Some("")), DEFAULT_LEVEL);
        assert_eq!(level_from_env(Some("   ")), DEFAULT_LEVEL);
    }

    #[test]
    fn a_valid_level_is_honored_and_whitespace_tolerated() {
        assert_eq!(level_from_env(Some("debug")), LevelFilter::DEBUG);
        assert_eq!(level_from_env(Some("TRACE")), LevelFilter::TRACE);
        assert_eq!(level_from_env(Some("  warn  ")), LevelFilter::WARN);
        assert_eq!(level_from_env(Some("off")), LevelFilter::OFF);

        // NUMERIC levels parse too, which is worth pinning because it is not
        // obvious and it is easy to mistake for a typo when reading a bug
        // report: 1=ERROR through 5=TRACE. Discovered by a test that assumed
        // "2" was garbage and was told otherwise.
        assert_eq!(level_from_env(Some("1")), LevelFilter::ERROR);
        assert_eq!(level_from_env(Some("2")), LevelFilter::WARN);
        assert_eq!(level_from_env(Some("5")), LevelFilter::TRACE);
    }

    /// A typo in a debugging env var must not be why an app stops logging. The
    /// failure mode of the alternative is the worst one available: you set
    /// `REPOSYNC_LOG` precisely because something is wrong, and the typo means
    /// the incident produces no evidence at all.
    #[test]
    fn an_unparseable_level_falls_back_instead_of_disabling_logging() {
        assert_eq!(level_from_env(Some("verbose")), DEFAULT_LEVEL);
        assert_eq!(level_from_env(Some("9")), DEFAULT_LEVEL);
        assert_eq!(level_from_env(Some("-1")), DEFAULT_LEVEL);
        assert_ne!(
            level_from_env(Some("nonsense")),
            LevelFilter::OFF,
            "a bad value must never resolve to OFF"
        );
    }
}
