import type { CSSProperties, ReactNode } from "react";
import { ToastProvider } from "@/components/ui/toast";

/**
 * A labelled gallery cell. `contained` turns the body into the containing
 * block for `position: fixed` descendants, so a real drawer, dialog or toast
 * stack stays inside the cell instead of covering the whole gallery.
 *
 * Any CSS `transform` establishes that containing block. `translateZ(0)` is
 * used because it moves nothing and is the usual idiom for it.
 */
export function Cell({
  title,
  note,
  height,
  contained = false,
  children,
}: {
  title: string;
  note?: string;
  height?: number;
  contained?: boolean;
  children: ReactNode;
}) {
  const style: CSSProperties = {};
  if (height !== undefined) style.height = height;
  if (contained) {
    style.transform = "translateZ(0)";
    style.overflow = "auto";
  }
  return (
    <section className="mb-6" aria-label={title}>
      <div className="mb-2 flex flex-wrap items-baseline gap-x-3">
        <h2 className="text-body font-semibold">{title}</h2>
        {note && <span className="text-cell text-muted-foreground">{note}</span>}
      </div>
      <div
        data-cell={title}
        style={style}
        className="rounded-lg border border-dashed border-border bg-background p-4"
      >
        {/* Each cell has its own toast provider so the fixed toast stack lands
            inside a contained cell instead of at the corner of the window. */}
        <ToastProvider>{children}</ToastProvider>
      </div>
    </section>
  );
}

/** A row that wraps, for showing variants side by side. */
export function Row({ children }: { children: ReactNode }) {
  return <div className="flex flex-wrap items-center gap-3">{children}</div>;
}
