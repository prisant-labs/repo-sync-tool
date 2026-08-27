import type { ReactNode } from "react";

import { cn } from "@/lib/utils";

/**
 * The one component that owns a screen's outer geometry: the page inset, the
 * heading row, the sticky toolbar region and the scroll rhythm.
 *
 * It exists because those were per-screen decisions and therefore drifted.
 * Dashboard and Repos began their content at one left inset and Settings at
 * another, not because either was wrong but because nothing said what the inset
 * was, so each screen picked. Once every screen renders through here, that class
 * of inconsistency stops being possible rather than being repeatedly corrected.
 *
 * Scrolling deliberately belongs to `AppShell`, not here: `AppShell` owns the
 * viewport-height layout, and a second scroll container nested inside it is how
 * you get two scrollbars. `PageShell` only marks which of its own children stick
 * to the top of that scroller.
 */
export function PageShell({
  title,
  actions,
  toolbar,
  width = "wide",
  children,
}: {
  /** The screen title. Rendered as the page's h1, see the note below. */
  title: string;
  /** Optional trailing controls on the heading row (Refresh, Add repos). */
  actions?: ReactNode;
  /**
   * Optional search and filter row. Sticks to the top with the heading rather
   * than scrolling away, because filters that leave the screen force a scroll
   * back up to change what you are looking at.
   */
  toolbar?: ReactNode;
  /**
   * `"wide"` fills the available width (tables and lists).
   *
   * `"narrow"` caps the measure for reading and form-filling (Settings), and
   * does it with a max-width and NO horizontal centring. Centring is what made
   * Settings look misaligned: a centred column's left edge moves with the window
   * while every other screen's stays put. Capped-and-left-aligned keeps the
   * comfortable measure and keeps the left edge where the eye expects it.
   */
  width?: "wide" | "narrow";
  children: ReactNode;
}) {
  return (
    <div className="flex min-h-full flex-col">
      {/*
        `sticky top-0` works against AppShell's scroll container. It needs an
        opaque background or the content scrolling underneath shows through, and
        it needs a z-index above the page body but below any floating surface
        (drawer, dialog, toast), which is why it is a low one.
      */}
      <header className="sticky top-0 z-20 bg-background px-page pt-page">
        <div className="flex items-center gap-3">
          {/*
            h1, not h2. Removing the topbar breadcrumb took the app's only h1
            with it and left every screen opening at h2, so the document outline
            had no top level and screen titles sat at the same level as dialog
            titles. The screen title is the page's top-level heading; this is
            where it belongs.
          */}
          <h1 className="text-page tracking-tight">{title}</h1>
          {actions ? <div className="ml-auto flex gap-2">{actions}</div> : null}
        </div>
        {toolbar ? <div className="pt-4">{toolbar}</div> : null}
        {/*
          The spacer is inside the sticky header on purpose. As padding on the
          body below it, the gap would scroll away and content would arrive
          flush against the toolbar the moment the page moved.
        */}
        <div className="h-section" />
      </header>

      <div
        className={cn(
          "flex flex-1 flex-col gap-section px-page pb-page",
          width === "narrow" && "max-w-3xl",
        )}
      >
        {children}
      </div>
    </div>
  );
}
