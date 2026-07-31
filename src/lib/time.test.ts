import { describe, expect, it } from "vitest";
import { hhMmToMinutes, minutesToHhMm } from "@/lib/time";

/**
 * Quiet hours are stored as a minute-of-day integer and edited through a native
 * "HH:MM" time input, so every edit is a round trip through these two functions.
 * A bug here does not throw: it silently reschedules the user's quiet window,
 * which they would only notice as notifications at the wrong hour.
 */

describe("minutesToHhMm", () => {
  it("formats the boundaries and the common defaults", () => {
    expect(minutesToHhMm(0)).toBe("00:00");
    expect(minutesToHhMm(22 * 60)).toBe("22:00"); // default quiet start
    expect(minutesToHhMm(7 * 60)).toBe("07:00"); // default quiet end
    expect(minutesToHhMm(1439)).toBe("23:59"); // last minute of the day
  });

  it("zero-pads both fields, because a time input rejects '7:5'", () => {
    expect(minutesToHhMm(7 * 60 + 5)).toBe("07:05");
    expect(minutesToHhMm(9)).toBe("00:09");
  });

  /**
   * Wrapping rather than clamping matters: an out-of-range value must still
   * produce a VALID time string. "24:00" or "-1:00" is not a time, and a native
   * input silently renders it blank, so the user sees an empty control with no
   * explanation.
   */
  it("wraps out-of-range values into a real time instead of an invalid string", () => {
    expect(minutesToHhMm(1440)).toBe("00:00"); // exactly one day
    expect(minutesToHhMm(1440 + 90)).toBe("01:30"); // more than a day
    expect(minutesToHhMm(-60)).toBe("23:00"); // negative wraps backwards
    expect(minutesToHhMm(-1)).toBe("23:59");
  });

  it("truncates a fractional minute rather than emitting a decimal", () => {
    expect(minutesToHhMm(90.7)).toBe("01:30");
  });
});

describe("hhMmToMinutes", () => {
  it("parses the boundaries and the common defaults", () => {
    expect(hhMmToMinutes("00:00")).toBe(0);
    expect(hhMmToMinutes("22:00")).toBe(22 * 60);
    expect(hhMmToMinutes("07:00")).toBe(7 * 60);
    expect(hhMmToMinutes("23:59")).toBe(1439);
  });

  /**
   * A native time input hands back "" when cleared. Midnight is the safe
   * reading: it is a real time, so the setting stays valid. Returning NaN would
   * poison the payload sent to the backend, and NaN serializes to null in JSON,
   * which would silently DISABLE quiet hours rather than fail loudly.
   */
  it("falls back to midnight on unparseable input rather than producing NaN", () => {
    expect(hhMmToMinutes("")).toBe(0);
    expect(hhMmToMinutes("not a time")).toBe(0);
    expect(hhMmToMinutes("12:xx")).toBe(0);
    expect(Number.isNaN(hhMmToMinutes("garbage"))).toBe(false);
  });
});

describe("the quiet-hours round trip", () => {
  /**
   * The property that actually matters. Every user edit is
   * minutes -> string -> minutes, and a mismatch anywhere in that loop moves
   * their quiet window without telling them.
   */
  it("preserves every minute of the day", () => {
    for (let minute = 0; minute < 1440; minute++) {
      expect(hhMmToMinutes(minutesToHhMm(minute))).toBe(minute);
    }
  });
});
