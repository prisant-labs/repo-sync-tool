import { cn } from "@/lib/utils";

/**
 * A pill-shaped toggle for narrowing a list, with an optional count badge.
 *
 * Lifted verbatim out of the Repos screen so the Activity screen can use the
 * same control rather than growing a second filter vocabulary. The only change
 * from the original is that `count` is now OPTIONAL, and that is a deliberate
 * honesty constraint rather than a convenience.
 *
 * The Repos screen filters a list it already holds in full, so it can label
 * every chip with a real total. The Activity screen cannot: `activity_list`
 * applies its `LIMIT` server-side, AFTER the filter, so the rows on screen are
 * one capped page of a filtered query and nothing in them says how many rows
 * the filter actually matched. A count derived from that page would be a number
 * that looks authoritative and is not. Omitting the badge says less and lies
 * less.
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
        "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors",
        "focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
        active
          ? "border-primary bg-primary/10 text-primary"
          : "border-border text-muted-foreground hover:bg-muted",
      )}
    >
      <span className={cn(!active && tone)}>{label}</span>
      {count !== undefined && (
        <span
          className={cn(
            "rounded-full px-1.5 font-mono text-[10px]",
            active ? "bg-primary/15" : "bg-muted",
          )}
        >
          {count}
        </span>
      )}
    </button>
  );
}
