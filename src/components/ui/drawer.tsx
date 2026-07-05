import { useRef } from "react";
import type { ReactNode } from "react";
import { cn } from "@/lib/utils";
import { useModalA11y } from "@/hooks/use-modal-a11y";

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
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
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
        aria-hidden={!open || undefined}
        inert={!open}
        tabIndex={-1}
        onKeyDown={onKeyDown}
        className={cn(
          "fixed inset-y-0 right-0 z-40 flex w-[480px] max-w-[92vw] flex-col border-l border-border bg-card shadow-2xl outline-none transition-transform duration-300",
          open ? "translate-x-0" : "translate-x-full",
        )}
      >
        {children}
      </aside>
    </>
  );
}
