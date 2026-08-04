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

/// Daily files the appender retains by default. The AGE half of retention; the
/// SIZE half is [`core_logging::LOG_DIR_MAX_BYTES`], swept at startup.
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

/// Environment variable overriding how many daily files are kept.
pub const LOG_DAYS_ENV: &str = "REPOSYNC_LOG_DAYS";

/// Environment variable overriding the total size budget, in MiB.
pub const LOG_MAX_MB_ENV: &str = "REPOSYNC_LOG_MAX_MB";

/// Bounds on [`LOG_DAYS_ENV`]. Zero would silently disable logging by deleting
/// every rotated file, which is not something anyone means to type; the upper
/// bound just stops a fat-fingered value from defeating retention entirely.
const MIN_LOG_DAYS: usize = 1;
const MAX_LOG_DAYS: usize = 365;

/// Bounds on [`LOG_MAX_MB_ENV`], for the same reasons.
const MIN_LOG_MB: u64 = 1;
const MAX_LOG_MB: u64 = 4096;

/// Retention resolved from the environment, or the defaults.
///
/// These are environment variables rather than Settings rows on purpose. The
/// logger initializes BEFORE the database opens - deliberately, since a database
/// that fails to open is exactly the failure most worth logging - so a
/// DB-backed setting could not be read at the point the appender is configured
/// without either logging the startup path into the void or inventing a second
/// config store. An env var is readable at exactly the right moment, and it
/// serves the only population that ever changes this: someone actively
/// reproducing a problem.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Retention {
    /// Daily files kept.
    pub max_files: usize,
    /// Total bytes the log directory may occupy.
    pub max_bytes: u64,
}

impl Default for Retention {
    fn default() -> Self {
        Retention {
            max_files: MAX_LOG_FILES,
            max_bytes: core_logging::LOG_DIR_MAX_BYTES,
        }
    }
}

/// The logging configuration ACTUALLY in effect, captured at init.
///
/// Returned from [`init`] rather than recomputed on demand. Re-reading the
/// environment when the Diagnostics view asks would report what the environment
/// says NOW, which is not necessarily what the appender was built with - and a
/// diagnostics panel that reports the configuration it wishes it had is worse
/// than one that reports nothing.
#[derive(Debug, Clone)]
pub struct LogConfig {
    /// The directory the rolling appender writes into.
    pub dir: std::path::PathBuf,
    /// The maximum level being recorded.
    pub level: LevelFilter,
    /// The age + size budget in force.
    pub retention: Retention,
}

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

/// Resolve retention from raw environment values.
///
/// CLAMPS rather than rejects. A value outside the bounds is a typo, and the
/// two failure modes are not symmetric: falling back to a sane number costs
/// nothing, while honoring `REPOSYNC_LOG_DAYS=0` would delete the evidence
/// someone set the variable to collect. Unparseable input falls back for the
/// same reason.
fn retention_from_env(days: Option<&str>, max_mb: Option<&str>) -> Retention {
    let default = Retention::default();

    let max_files = days
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<usize>().ok())
        .map(|n| n.clamp(MIN_LOG_DAYS, MAX_LOG_DAYS))
        .unwrap_or(default.max_files);

    let max_bytes = max_mb
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .and_then(|s| s.parse::<u64>().ok())
        .map(|n| n.clamp(MIN_LOG_MB, MAX_LOG_MB))
        .map(|mb| mb * 1024 * 1024)
        .unwrap_or(default.max_bytes);

    Retention {
        max_files,
        max_bytes,
    }
}

/// Install the global subscriber and return the guard that keeps it running,
/// together with the configuration it was built with.
///
/// Ordering matters: the size sweep runs BEFORE the appender opens today's file,
/// so the sweep never contends with a handle this process is about to hold.
///
/// Returns `Err` rather than panicking. Failing to open a log file is a reason
/// to run without logs, never a reason not to start - the same best-effort
/// principle the activity writer follows, and for the same reason: the
/// diagnostic must not become the outage.
pub fn init(paths: &AppPaths) -> Result<(LogGuard, LogConfig), String> {
    paths
        .ensure_dirs()
        .map_err(|e| format!("could not create the log directory: {e}"))?;

    let dir = paths.log_dir();

    let retention = retention_from_env(
        std::env::var(LOG_DAYS_ENV).ok().as_deref(),
        std::env::var(LOG_MAX_MB_ENV).ok().as_deref(),
    );

    // Bound the directory before opening anything. A failure here is reported
    // and then ignored: an oversized log directory is a far smaller problem than
    // no logging at all.
    let pruned = core_logging::prune_to_size_budget(&dir, retention.max_bytes);

    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(core_logging::LOG_FILE_PREFIX)
        .filename_suffix(core_logging::LOG_FILE_SUFFIX)
        .max_log_files(retention.max_files)
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
            budget_bytes = retention.max_bytes,
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
        max_log_files = retention.max_files,
        max_bytes = retention.max_bytes,
        "RepoSync starting"
    );

    Ok((
        LogGuard { _worker: worker },
        LogConfig {
            dir,
            level,
            retention,
        },
    ))
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

    #[test]
    fn retention_defaults_when_unset_or_blank() {
        assert_eq!(retention_from_env(None, None), Retention::default());
        assert_eq!(
            retention_from_env(Some(""), Some("  ")),
            Retention::default()
        );
    }

    #[test]
    fn retention_honors_valid_values_and_converts_mib_to_bytes() {
        let r = retention_from_env(Some("30"), Some("64"));
        assert_eq!(r.max_files, 30);
        assert_eq!(r.max_bytes, 64 * 1024 * 1024);

        // Whitespace is tolerated, since these get set by hand mid-debugging.
        let r = retention_from_env(Some("  7 "), Some(" 8 "));
        assert_eq!(r.max_files, 7);
        assert_eq!(r.max_bytes, 8 * 1024 * 1024);
    }

    /// The asymmetry that makes clamping the right call rather than rejecting:
    /// honoring `REPOSYNC_LOG_DAYS=0` would delete the evidence the variable was
    /// set to collect, which is the worst possible reading of a typo.
    #[test]
    fn out_of_range_values_clamp_instead_of_disabling_retention() {
        let zero = retention_from_env(Some("0"), Some("0"));
        assert_eq!(zero.max_files, MIN_LOG_DAYS);
        assert_eq!(zero.max_bytes, MIN_LOG_MB * 1024 * 1024);
        assert!(
            zero.max_files >= 1 && zero.max_bytes > 0,
            "zero must never mean 'keep nothing'"
        );

        let huge = retention_from_env(Some("999999"), Some("999999"));
        assert_eq!(huge.max_files, MAX_LOG_DAYS);
        assert_eq!(huge.max_bytes, MAX_LOG_MB * 1024 * 1024);
    }

    #[test]
    fn unparseable_values_fall_back_to_the_defaults() {
        let d = Retention::default();
        for bad in ["lots", "-5", "12.5", "7d", "64MB"] {
            let r = retention_from_env(Some(bad), Some(bad));
            assert_eq!(r, d, "{bad:?} should have fallen back");
        }
    }

    /// One variable being set must not disturb the other.
    #[test]
    fn the_two_variables_are_independent() {
        let d = Retention::default();

        let only_days = retention_from_env(Some("3"), None);
        assert_eq!(only_days.max_files, 3);
        assert_eq!(only_days.max_bytes, d.max_bytes);

        let only_size = retention_from_env(None, Some("5"));
        assert_eq!(only_size.max_files, d.max_files);
        assert_eq!(only_size.max_bytes, 5 * 1024 * 1024);
    }
}
