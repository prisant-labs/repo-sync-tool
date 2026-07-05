import type { LucideIcon } from "lucide-react";
import { CircleCheck } from "lucide-react";
import type { ReactNode } from "react";
import { Card } from "@/components/ui/card";
import { cn } from "@/lib/utils";

/**
 * A friendly empty / first-run panel: icon + headline + one-line explanation
 * + an optional call to action. Used wherever a bare "nothing here" sentence
 * would otherwise stand alone (first-run screens, empty lists).
 */
export function EmptyState({
  icon: Icon,
  title,
  description,
  action,
  compact = false,
  className,
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  action?: ReactNode;
  compact?: boolean;
  className?: string;
}) {
  return (
    <Card
      className={cn(
        "flex flex-col items-center gap-3 text-center",
        compact ? "px-6 py-10" : "px-6 py-16",
        className,
      )}
    >
      <div
        className={cn("grid place-items-center rounded-full bg-muted", compact ? "size-9" : "size-12")}
      >
        <Icon className={cn("text-muted-foreground", compact ? "size-4" : "size-6")} />
      </div>
      <div className="flex flex-col gap-1">
        <h3 className={cn("font-semibold", compact ? "text-sm" : "text-base")}>{title}</h3>
        <p className={cn("max-w-sm text-muted-foreground", compact ? "text-xs" : "text-sm")}>
          {description}
        </p>
      </div>
      {action}
    </Card>
  );
}

/**
 * A calm, positive "everything is fine" callout. Uses the sync status tint,
 * the one status-colored focal region a screen is allowed, so an all-clear
 * dashboard reads as intentional rather than merely empty.
 */
export function AllClearState({ title, description }: { title: string; description: string }) {
  return (
    <div className="flex items-center gap-3 rounded-lg border border-status-sync/40 bg-status-sync/12 p-4">
      <CircleCheck className="size-5 shrink-0 text-status-sync" />
      <div className="min-w-0">
        <div className="text-sm font-semibold text-status-sync">{title}</div>
        <div className="text-xs text-foreground/80">{description}</div>
      </div>
    </div>
  );
}
