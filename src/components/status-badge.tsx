import { cn } from "@/lib/utils";
import { STATUS_ICON, STATUS_STYLE, type RepoStatus } from "@/lib/status";

/**
 * The status taxonomy rendered as color + icon + word. `count` folds the
 * ahead/behind magnitude into the label (e.g. "14 behind") when relevant.
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
        "inline-flex items-center gap-1.5 font-mono text-xs font-semibold",
        style.text,
        className,
      )}
    >
      <Icon className="size-3.5" />
      {label}
    </span>
  );
}
