import { useRef } from "react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { useModalA11y } from "@/hooks/use-modal-a11y";

/**
 * The two widths this primitive ships. `default` is the original fixed
 * 480px, unchanged since before N4. `wide` is N4's roughly-two-thirds
 * treatment (ui-delivery-plan.md ledger B2, ratified 2026-08-27: "the drawer
 * too cramped" for the repo detail panel specifically).
 *
 * N4's first cut applied `wide` unconditionally to every `Drawer` consumer,
 * because the width lived on the shared primitive with no variant - which
 * silently widened the Activity screen's own receipt drawer (and the repo
 * detail panel's nested receipt) from 480px to ~66vw, well past the ratified
 * scope (Codex adversarial review, finding 1, confirmed). Only the two
 * detail-panel call sites (`screens/repos.tsx`, `screens/dashboard.tsx`) opt
 * into `wide`; every receipt drawer stays on the default.
 *
 * `min-w-[480px]` on `wide` is a deliberate addition beyond the literal
 * ratification (which named only "roughly 66vw"): without it a window
 * narrower than ~730px would hand the drawer LESS room than `default`
 * itself, since `min-width` always wins over `max-width` in the CSS cascade
 * regardless of source order.
 */
const SIZE_CLASSES: Record<"default" | "wide", string> = {
  default: "w-[480px] max-w-[92vw]",
  wide: "w-[66vw] min-w-[480px] max-w-[92vw]",
};

/**
 * A right-side slide-over. Scrim + transform-only motion (no layout thrash).
 * Stays mounted across open/close (for the slide transition) but is made
 * `inert` and hidden from assistive tech while closed, and while open traps
 * focus and closes on Escape (findings 12/13, BL-NI-29).
 */
export function Drawer({
  open,
  onClose,
  children,
  size = "default",
  "aria-label": ariaLabel,
  "aria-labelledby": ariaLabelledBy,
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
  size?: "default" | "wide";
  /**
   * Accessible name for the `role="dialog"` element. Pass one of these two -
   * `aria-labelledby` when the content already renders a visible heading
   * (preferred, so the name can never drift from what is on screen: see
   * `repo-detail.tsx`'s `REPO_DETAIL_TITLE_ID` and `activity-receipt.tsx`'s
   * `ACTIVITY_RECEIPT_TITLE_ID`), `aria-label` otherwise. Neither is
   * required, but without one a screen reader announces every drawer
   * identically ("dialog"), and two open at once (the repo detail panel and
   * its own nested receipt) become indistinguishable modal boundaries
   * (Codex adversarial review, finding 3).
   */
  "aria-label"?: string;
  "aria-labelledby"?: string;
}) {
  const asideRef = useRef<HTMLElement>(null);
  const onKeyDown = useModalA11y(open, onClose, asideRef);

  return (
    <>
      <div
        className={cn(
          "fixed inset-0 z-30 bg-black/40 transition-opacity duration-200",
          open ? "opacity-100" : "pointer-events-none opacity-0",
        )}
        onClick={onClose}
        aria-hidden
      />
      <aside
        ref={asideRef}
        role="dialog"
        aria-modal="true"
        aria-label={ariaLabel}
        aria-labelledby={ariaLabelledBy}
        aria-hidden={!open || undefined}
        inert={!open}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className={cn(
          "fixed inset-y-0 right-0 z-40 flex flex-col border-l border-border bg-card shadow-float outline-none transition-transform duration-300",
          SIZE_CLASSES[size],
          open ? "translate-x-0" : "translate-x-full",
        )}
      >
        {children}
      </aside>
    </>
  );
}
