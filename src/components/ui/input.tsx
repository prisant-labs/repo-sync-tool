import type { InputHTMLAttributes } from "react";
import { cn } from "@/lib/utils";
import { useFieldLabelId } from "@/components/ui/field-label";

export function Input({ className, ...props }: InputHTMLAttributes<HTMLInputElement>) {
  // Same accessible-name rule as `Switch` (BL-NI-90): an enclosing `Field`
  // names the control, an explicit `aria-label` or `aria-labelledby` on the
  // call site wins over it, and outside a `Field` nothing is invented. The
  // retention and Git-path inputs in Settings are named this way, which is the
  // "every control type at once" the backlog row asked for rather than a
  // switch-only fix.
  const fieldLabelId = useFieldLabelId();
  const labelledBy =
    props["aria-label"] === undefined && props["aria-labelledby"] === undefined ? fieldLabelId : props["aria-labelledby"];
  return (
    <input
      className={cn(
        "h-9 w-full rounded-md border border-input bg-transparent px-3 text-sm shadow-sm outline-none transition-colors placeholder:text-muted-foreground focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
        className,
      )}
      {...props}
      aria-labelledby={labelledBy}
    />
  );
}
