import { useRef } from "react";
import type { MouseEvent, ReactNode } from "react";
import { cn } from "@/lib/utils";
import { useModalA11y } from "@/hooks/use-modal-a11y";

/**
 * A centered modal dialog. Click the backdrop to dismiss. Its content stays
 * mounted while closed (some callers keep form state alive across opens), so
 * it is made `inert` and hidden from assistive tech while closed; while open
 * it traps focus and closes on Escape (findings 12/13, BL-NI-29).
 */
export function Dialog({
  open,
  onClose,
  children,
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const onKeyDown = useModalA11y(open, onClose, containerRef);

  function onBackdrop(e: MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }
  return (
    <div
      ref={containerRef}
      role="dialog"
      aria-modal="true"
      aria-hidden={!open || undefined}
      inert={!open}
      tabIndex={-1}
      onClick={onBackdrop}
      onKeyDown={onKeyDown}
      className={cn(
        "fixed inset-0 z-50 grid place-items-center bg-black/50 p-6 outline-none transition-opacity duration-200",
        open ? "opacity-100" : "pointer-events-none opacity-0",
      )}
    >
      <div
        className={cn(
          "flex max-h-[86vh] w-full max-w-xl flex-col overflow-hidden rounded-xl border border-border bg-card shadow-2xl transition-transform duration-200",
          open ? "scale-100" : "scale-95",
        )}
      >
        {children}
      </div>
    </div>
  );
}
