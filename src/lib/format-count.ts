/**
 * Compact form of a commit count for a width-constrained chip or cell.
 *
 * One rule: below 1,000 the number is shown exactly. From 1,000 up it is
 * rounded DOWN to whole thousands and suffixed "k+" (1,000 to 1,999 reads
 * "1k+", 98,767 reads "98k+"). From 1,000,000 up it is rounded down to whole
 * millions and suffixed "M+". The "+" means "at least this many", which is
 * honest because rounding is always downward.
 *
 * Callers that abbreviate must also expose the exact figure, see
 * `countTitle`.
 */
export function formatCount(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${Math.floor(n / 1000)}k+`;
  return `${Math.floor(n / 1_000_000)}M+`;
}

/**
 * Tooltip text with the exact figure, e.g. "98,767 commits behind".
 * Returns undefined for a count that is shown exactly, so no title is added.
 */
export function countTitle(n: number, direction: "ahead" | "behind"): string | undefined {
  if (n < 1000) return undefined;
  return `${n.toLocaleString("en-US")} commits ${direction}`;
}
