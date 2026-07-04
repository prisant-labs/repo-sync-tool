import { cn } from "@/lib/utils";
import { STATUS_STYLE, type RepoStatus } from "@/lib/status";

/**
 * The bespoke lag indicator: a thin bar whose fill length encodes lag
 * magnitude (not just "behind"). Animated with transform: scaleX only, so
 * there is no layout thrash.
 */
export function LagSignal({
  status,
  magnitude,
  label,
  className,
}: {
  status: RepoStatus;
  magnitude: number;
  label: string;
  className?: string;
}) {
  const clamped = Math.max(0, Math.min(1, magnitude));
  return (
    <div className={className}>
      <div className="h-1.5 overflow-hidden rounded-full bg-muted">
        <div
          className={cn(
            "h-full origin-left rounded-full transition-transform duration-500",
            STATUS_STYLE[status].bar,
          )}
          style={{ transform: `scaleX(${clamped})` }}
        />
      </div>
      <div className="mt-1.5 font-mono text-[11px] text-muted-foreground">{label}</div>
    </div>
  );
}
