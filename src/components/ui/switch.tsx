import { cn } from "@/lib/utils";

export function Switch({
  checked,
  onCheckedChange,
  disabled,
  className,
  // A `role="switch"` button whose only child is a decorative span has no
  // accessible name of its own, so a screen reader announces it as "switch, on"
  // with nothing to say WHICH switch. The visible `Field` label sits in a plain
  // div rather than a <label>, so it does not supply one either. Optional
  // because the existing call sites do not pass it yet (BL-NI-90); required in
  // spirit, and any new call site should.
  "aria-label": ariaLabel,
}: {
  checked: boolean;
  onCheckedChange: (value: boolean) => void;
  disabled?: boolean;
  className?: string;
  "aria-label"?: string;
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={ariaLabel}
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
