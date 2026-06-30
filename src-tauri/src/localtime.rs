//! Edge local-time helpers (the edge-wiring effort).
//!
//! `reposync-core` is deliberately timezone-free (no `chrono` / `time`), and the
//! headless scheduler cannot ask the webview for the user's local time. So the EDGE
//! owns the local-time math, via the `time` crate's `local-offset` feature.
//!
//! On Windows this is sound (the Win32 `GetTimeZoneInformation` path). Where the
//! offset is indeterminate (the documented multithreaded-Unix soundness guard), it
//! falls back to UTC so a summary / quiet-hours decision still yields a value rather
//! than failing - the fallback only bites on not-yet-supported platforms.
//!
//! The timezone arithmetic ([`day_window_for`]) is a pure function of an instant +
//! an offset, so it is unit-tested here without touching the OS clock; only the
//! one-line offset probe ([`local_offset`]) is launch-only.

use reposync_core::summary::DayWindow;
use time::{OffsetDateTime, Time, UtcOffset};

/// The current local UTC offset, or UTC if it cannot be determined.
///
/// `time` returns an error for an indeterminate offset (its multithreaded-Unix
/// soundness guard); RepoSync is Windows-first, where the Win32 path always
/// resolves, so the UTC fallback only applies on not-yet-supported platforms.
fn local_offset() -> UtcOffset {
    UtcOffset::local_offset_at(OffsetDateTime::now_utc()).unwrap_or(UtcOffset::UTC)
}

/// Build the [`DayWindow`] for the local day containing `now_utc`, evaluated at
/// `offset`. Pure (instant + offset in, window out) so the timezone arithmetic is
/// testable without the OS clock.
///
/// The window is `[local-midnight-today, local-midnight-today + 24h)`. Using a fixed
/// 24h span (rather than recomputing the next local midnight) keeps it simple and is
/// exact except across a DST change, where that one day's roll-up boundary may be off
/// by the DST delta - acceptable for a coarse daily summary. The fields are built
/// consistently (label from the local date; `end = start + 24h`), so the window is
/// always valid; the core's own `validate_window` is the backstop.
fn day_window_for(now_utc: OffsetDateTime, offset: UtcOffset) -> DayWindow {
    let local = now_utc.to_offset(offset);
    let date = format!(
        "{:04}-{:02}-{:02}",
        local.year(),
        u8::from(local.month()),
        local.day()
    );
    let start_unix =
        OffsetDateTime::new_in_offset(local.date(), Time::MIDNIGHT, offset).unix_timestamp();
    // Construct the struct directly (its fields are public): a value that somehow
    // failed `DayWindow::new`'s validation should reach the core and surface as a
    // typed error there, never panic in an IPC command.
    DayWindow {
        date,
        start_unix,
        end_unix: start_unix + 86_400,
    }
}

/// The [`DayWindow`] for the user's current local day - the edge clock the
/// `summary_today` command (and, later, the scheduler's daily cadence) reads.
pub fn local_day_window() -> DayWindow {
    day_window_for(OffsetDateTime::now_utc(), local_offset())
}

#[cfg(test)]
mod tests {
    use super::*;
    use time::{Date, Month};

    /// Construct a UTC `OffsetDateTime` without the `macros` feature.
    fn utc(y: i32, m: Month, d: u8, h: u8, min: u8) -> OffsetDateTime {
        OffsetDateTime::new_in_offset(
            Date::from_calendar_date(y, m, d).unwrap(),
            Time::from_hms(h, min, 0).unwrap(),
            UtcOffset::UTC,
        )
    }

    #[test]
    fn window_uses_the_local_day_not_the_utc_day() {
        // 2026-06-30 01:30 UTC at offset -05:00 is still 2026-06-29 20:30 LOCAL, so
        // the local day is the 29th and the window starts at the 29th's local
        // midnight (-05:00 => 05:00 UTC on the 29th), NOT the UTC 30th.
        let now = utc(2026, Month::June, 30, 1, 30);
        let offset = UtcOffset::from_hms(-5, 0, 0).unwrap();
        let w = day_window_for(now, offset);

        assert_eq!(
            w.date, "2026-06-29",
            "labels the local day, not the UTC day"
        );
        let expected_start = OffsetDateTime::new_in_offset(
            Date::from_calendar_date(2026, Month::June, 29).unwrap(),
            Time::MIDNIGHT,
            offset,
        )
        .unix_timestamp();
        assert_eq!(w.start_unix, expected_start, "start is local midnight");
        assert_eq!(w.end_unix, expected_start + 86_400, "24h span");
        assert!(
            now.unix_timestamp() >= w.start_unix && now.unix_timestamp() < w.end_unix,
            "the instant falls inside its own day window"
        );
    }

    #[test]
    fn window_at_utc_offset_is_the_utc_day() {
        let now = utc(2026, Month::June, 30, 12, 0);
        let w = day_window_for(now, UtcOffset::UTC);
        assert_eq!(w.date, "2026-06-30");
        assert_eq!(
            w.start_unix,
            utc(2026, Month::June, 30, 0, 0).unix_timestamp()
        );
        assert_eq!(w.end_unix, w.start_unix + 86_400);
    }

    #[test]
    fn window_handles_a_positive_offset_crossing_into_the_next_local_day() {
        // 2026-06-29 22:00 UTC at +05:30 (IST) is 2026-06-30 03:30 LOCAL: the local
        // day has already ticked to the 30th while UTC is still the 29th.
        let now = utc(2026, Month::June, 29, 22, 0);
        let offset = UtcOffset::from_hms(5, 30, 0).unwrap();
        let w = day_window_for(now, offset);
        assert_eq!(w.date, "2026-06-30");
        assert!(now.unix_timestamp() >= w.start_unix && now.unix_timestamp() < w.end_unix);
    }
}
