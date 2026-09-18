import { cn } from "@/lib/utils";

/**
 * A toggle for narrowing a list, with an optional count badge.
 *
 * Lifted verbatim out of the Repos screen so the Activity screen can use the
 * same control rather than growing a second filter vocabulary. `count` is
 * OPTIONAL, and that is a deliberate honesty constraint rather than a
 * convenience.
 *
 * The Repos screen filters a list it already holds in full, so it can label
 * every chip with a real total. The Activity screen cannot: `activity_list`
 * applies its `LIMIT` server-side, AFTER the filter, so the rows on screen are
 * one capped page of a filtered query and nothing in them says how many rows
 * the filter actually matched. A count derived from that page would be a number
 * that looks authoritative and is not. Omitting the badge says less and lies
 * less.
 *
 * C1 - one filled language. This control used to be an outlined round pill
 * while `StatusBadge` was a filled square-ish chip, which meant the filter row
 * and the status column directly beneath it spoke two different visual
 * languages about the same words ("Behind" as a filter, "Behind" as a fact).
 * It now adopts the chip's language: `rounded-md`, `font-mono`, and a FILL in
 * every state rather than an outline. What still separates a filter from a
 * status is that a filter's fill is NEUTRAL until engaged, and accent when it
 * is - never a status hue. The `tone` prop colours the resting label only, so
 * "Behind" can still read in its own hue without becoming a status chip.
 *
 * Measured with `_local/design/2-options/2026-08-28_iterations/_generators/
 * contrast.py`, compositing alpha in gamma-encoded sRGB the way CSS does, on
 * `--background` (the surface `PageShell`'s sticky header paints):
 *
 * | | light | dark |
 * |---|---|---|
 * | resting label, `muted-foreground` on `muted` | 4.71:1 | 5.83:1 |
 * | engaged label, `primary-ink` on `primary/15` | 5.06:1 | 5.43:1 |
 * | count, `muted-foreground` on `background` in the chip | 5.20:1 | 7.63:1 |
 * | count, `primary-ink` on `background` in the chip | 6.23:1 | 6.24:1 |
 *
 * Two things that look arbitrary and are not. The badge is `bg-background`
 * rather than the old `bg-muted`, because a muted badge inside a now-muted
 * chip is 1:1 - invisible. And hover carries almost nothing on the fill
 * (`foreground/5` is ~1.1:1 against the chip); its load-bearing signal is the
 * text jump to `text-foreground`, the same lever and the same reasoning as the
 * sidebar's nav items, where a measured pass established that no alpha on the
 * neutral ramp can separate two neutral states.
 */
export function FilterChip({
  label,
  count,
  active,
  tone,
  onClick,
}: {
  label: string;
  count?: number;
  active: boolean;
  tone?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={active}
      className={cn(
        // The border is reserved rather than conditional, and transparent in
        // both states: it costs a pixel of width that never moves, where
        // adding a border only when engaged would shift every chip to its
        // right on each toggle.
        "flex items-center gap-1.5 rounded-md border border-transparent px-2.5 py-1",
        "font-mono text-xs font-medium transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        active
          ? "bg-primary/15 text-primary-ink"
          : "bg-muted text-muted-foreground hover:bg-foreground/5 hover:text-foreground",
      )}
    >
      <span className={cn(!active && tone)}>{label}</span>
      {count !== undefined && (
        <span className="rounded-sm bg-background px-1.5 font-mono text-[10px]">{count}</span>
      )}
    </button>
  );
}
