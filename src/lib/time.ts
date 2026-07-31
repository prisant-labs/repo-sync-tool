/**
 * Minute-of-day conversions for the quiet-hours controls.
 *
 * The backend stores `quietHoursStart` / `quietHoursEnd` as minutes since local
 * midnight (0..1439); a native `<input type="time">` speaks "HH:MM". These two
 * functions are that boundary, and they are the only place the conversion
 * happens.
 *
 * Extracted from `src/screens/settings.tsx` so they can be tested directly. They
 * were correct there, but private to a component, which meant the quiet-hours
 * round trip - the setting most likely to silently mangle a user's schedule -
 * had no coverage at all.
 */

/** Minutes in a day. */
const MINUTES_PER_DAY = 1440;

/**
 * A minute-of-day integer to the "HH:MM" string a native time input expects.
 *
 * Wraps rather than clamps, and handles negatives, so an out-of-range value
 * still produces a valid time string instead of "24:30" or "-1:00", which the
 * input would silently reject and render as blank.
 */
export function minutesToHhMm(minutes: number): string {
  const wrapped = ((Math.trunc(minutes) % MINUTES_PER_DAY) + MINUTES_PER_DAY) % MINUTES_PER_DAY;
  const hh = Math.floor(wrapped / 60);
  const mm = wrapped % 60;
  return `${String(hh).padStart(2, "0")}:${String(mm).padStart(2, "0")}`;
}

/**
 * The inverse: "HH:MM" back to a minute-of-day integer.
 *
 * Returns 0 for anything unparseable. A native time input can hand back an empty
 * string when cleared, and 0 (midnight) is the safe reading: it is a real time,
 * so the setting stays valid rather than becoming NaN and poisoning the payload
 * sent to the backend.
 */
export function hhMmToMinutes(value: string): number {
  const [h, m] = value.split(":");
  const hours = Number(h);
  const mins = Number(m);
  if (Number.isNaN(hours) || Number.isNaN(mins)) return 0;
  return hours * 60 + mins;
}
