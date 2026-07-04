import type { MouseEvent, ReactNode } from "react";
import { cn } from "@/lib/utils";

/** A centered modal dialog. Click the backdrop to dismiss. */
export function Dialog({
  open,
  onClose,
  children,
}: {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}) {
  function onBackdrop(e: MouseEvent<HTMLDivElement>) {
    if (e.target === e.currentTarget) onClose();
  }
  return (
    <div
      role="dialog"
      aria-modal="true"
      onClick={onBackdrop}
      className={cn(
        "fixed inset-0 z-50 grid place-items-center bg-black/50 p-6 transition-opacity duration-200",
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
