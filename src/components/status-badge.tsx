import { cn } from "@/lib/utils";
import { STATUS_ICON, STATUS_STYLE, type RepoStatus } from "@/lib/status";

/**
 * The status taxonomy rendered as a FILLED chip: tint + icon + word (ratified
 * 4B). `count` folds the ahead/behind magnitude into the label (e.g. "14
 * behind") when relevant.
 *
 * A chip rather than coloured text, for two reasons. It gives status a fixed
 * shape, so a column of them scans as a column rather than as differently
 * coloured words of differing length. And it separates status from a GROUP,
 * which is an outlined pill with a user-chosen dot: three properties differ
 * (fill vs outline, glyph vs dot, weight 600 vs 500) so the distinction
 * survives a group that happens to be named "Behind" and coloured purple.
 *
 * `justify-self-start` is not optional. A grid item defaults to
 * `justify-self: stretch`, and `display: inline-flex` blockifies under it, so
 * without this the chip silently grows to fill its whole column and every chip
 * in the table ends up a different width. That defect shipped once already and
 * passed every assertion, because nothing asked whether two chips matched.
 */
export function StatusBadge({
  status,
  count,
  className,
}: {
  status: RepoStatus;
  count?: number;
  className?: string;
}) {
  const style = STATUS_STYLE[status];
  const Icon = STATUS_ICON[status];
  const label =
    (status === "behind" || status === "ahead") && count != null && count > 0
      ? `${count} ${style.label.toLowerCase()}`
      : style.label;

  return (
    <span
      className={cn(
        "inline-flex w-fit justify-self-start items-center gap-1.5 rounded-md",
        "px-[0.5em] pt-[0.2em] pb-[0.25em] font-mono text-xs font-semibold whitespace-nowrap",
        style.chip,
        className,
      )}
    >
      <Icon className="size-3.5 shrink-0" />
      {label}
    </span>
  );
}
