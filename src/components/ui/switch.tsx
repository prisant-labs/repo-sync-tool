import { cn } from "@/lib/utils";
import { useFieldLabelId } from "@/components/ui/field-label";

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  className,
  // A `role="switch"` button whose only child is a decorative span has no
  // accessible name of its own, so a screen reader announces it as "switch, on"
  // with nothing to say WHICH switch. The visible `Field` label sits in a plain
  // div rather than a <label>, so it does not supply one either.
  //
  // BL-NI-90 (switch accessible names) closed 2026-09-04 by making the
  // enclosing `Field` own the association, so a switch inside one needs
  // nothing here. This stays as the escape hatch for a switch rendered
  // OUTSIDE a `Field` - the case that created the bug in the first place, when
  // the theme toggle moved out of the topbar and lost the `aria-label` its old
  // header button carried.
  "aria-label": ariaLabel,
}: {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  // An explicit name wins; otherwise take the enclosing `Field`'s label. Both
  // absent means no `Field` and no name passed, which is the BL-NI-90 shape
  // and is what the test in `switch.test.tsx` guards against returning.
  const fieldLabelId = useFieldLabelId();
  const labelledBy = ariaLabel === undefined ? fieldLabelId : undefined;
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
      aria-labelledby={labelledBy}
      disabled={disabled}
      onClick={() => onCheckedChange(!checked)}
      className={cn(
        "relative h-6 w-11 shrink-0 rounded-full transition-colors disabled:opacity-50",
        checked ? "bg-primary" : "bg-muted",
        className,
      )}
    >
      <span
        className={cn(
          "absolute top-0.5 left-0.5 size-5 rounded-full bg-background shadow transition-transform",
          checked && "translate-x-5",
        )}
      />
    </button>
  );
}
