import { useContext, type CSSProperties, type ReactNode } from "react";
import { WINDOWS, WindowContext } from "@/gallery/window-frame";
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
  framed = false,
  children,
}: {
  title: string;
  note?: string;
  height?: number;
  contained?: boolean;
  /** A whole-screen cell: the Window control frames its body at a window size. */
  framed?: boolean;
  children: ReactNode;
}) {
  const windowId = useContext(WindowContext);
  const win = WINDOWS.find((w) => w.id === windowId) ?? WINDOWS[0];
  const frame = framed && win.cssWidth !== null ? win : null;
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
        <ToastProvider>
          {frame === null ? (
            children
          ) : (
            // The frame is the window. It carries its own transform so a fixed
            // drawer or dialog is contained by the window, not by the wider cell.
            // Under zoom, its CSS height is the visual height divided by the zoom.
            <div
              data-window-frame={windowId}
              style={{
                width: frame.cssWidth,
                height: height === undefined ? undefined : (height - 34) / frame.zoom,
                zoom: frame.zoom === 1 ? undefined : frame.zoom,
                transform: "translateZ(0)",
                overflow: "auto",
              }}
              className="rounded-md border border-border bg-background"
            >
              {children}
            </div>
          )}
        </ToastProvider>
      </div>
    </section>
  );
}

/** A row that wraps, for showing variants side by side. */
export function Row({ children }: { children: ReactNode }) {
  return <div className="flex flex-wrap items-center gap-3">{children}</div>;
}
