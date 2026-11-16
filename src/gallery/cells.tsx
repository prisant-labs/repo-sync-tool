import { useContext, type CSSProperties, type ReactNode } from "react";
import { WINDOWS, WindowContext, encodeFrameHash, type FrameScreen } from "@/gallery/window-frame";
import { ThemeContext } from "@/gallery/looks";
import { ToastProvider } from "@/components/ui/toast";

/**
 * A labelled gallery cell. `contained` turns the body into the containing
 * block for `position: fixed` descendants, so a real drawer, dialog or toast
 * stack stays inside the cell instead of covering the whole gallery.
 *
 * Any CSS `transform` establishes that containing block. `translateZ(0)` is
 * used because it moves nothing and is the usual idiom for it.
 *
 * `frameScreen` marks a whole-screen cell. With the Window control at 1024px or
 * 1024px at 150%, the cell shows an iframe of that screen instead of its children,
 * because only an iframe has a CSS viewport of its own (see `window-frame.ts`).
 */
export function Cell({
  title,
  note,
  height,
  contained = false,
  frameScreen,
  children,
}: {
  title: string;
  note?: string;
  height?: number;
  contained?: boolean;
  frameScreen?: FrameScreen;
  children: ReactNode;
}) {
  const controls = useContext(WindowContext);
  const dark = useContext(ThemeContext);
  const win = WINDOWS.find((w) => w.id === controls.windowId) ?? WINDOWS[0];
  const framed = frameScreen !== undefined && win.cssWidth !== null;
  const style: CSSProperties = {};
  if (height !== undefined) style.height = height;
  if (contained) {
    style.transform = "translateZ(0)";
    style.overflow = "auto";
  }

  // The iframe's CSS height is the visual height over the scale, so the scaled
  // box ends up the same height as the cell body.
  const visualHeight = (height ?? 760) - 34;
  const src = framed
    ? `${window.location.href.split("#")[0]}${encodeFrameHash({ screen: frameScreen, scenario: controls.scenario, look: controls.look, dark })}`
    : null;

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
          {framed && src !== null ? (
            <div
              data-window-wrapper={controls.windowId}
              style={{ width: (win.cssWidth ?? 0) * win.scale, height: visualHeight }}
              className="overflow-hidden rounded-md border border-border"
            >
              <iframe
                // Every control is part of the key, so a change reloads the
                // frame instead of leaving it on the old hash.
                key={src}
                title={`${title} at ${win.label}`}
                data-window-frame={controls.windowId}
                src={src}
                style={{
                  width: win.cssWidth ?? undefined,
                  height: visualHeight / win.scale,
                  transform: win.scale === 1 ? undefined : `scale(${win.scale})`,
                  transformOrigin: "top left",
                  border: 0,
                  display: "block",
                }}
              />
            </div>
          ) : (
            children
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
