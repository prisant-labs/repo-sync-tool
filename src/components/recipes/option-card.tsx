import type { ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * The filled-or-empty radio dot that marks a choice card. It is a picture, not
 * a control: the card around it (or the button that is the card) owns the
 * click and the accessible state.
 *
 * Used by `OptionCard` below and by the "Custom interval" card in
 * `repo-detail.tsx`, which cannot be an `OptionCard` because it holds a number
 * field and an Apply button inside the card.
 */
export function RadioDot({ active }: { active: boolean }) {
  return (
    <span
      className={cn(
        "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border",
        active ? "border-primary" : "border-muted-foreground/40",
      )}
    >
      {active && <span className="size-2 rounded-full bg-primary" />}
    </span>
  );
}

/**
 * A radio-style choice card: a radio dot, a label, and a description that is
 * always visible (the maintainer's "show the meaning up front" preference),
 * all inside one button.
 *
 * The chosen card is not clickable: choosing what is already chosen does
 * nothing, so the button is disabled whenever `active` is true, on top of any
 * `disabled` the caller passes. `dimmed` fades the card for a choice that is
 * not offered yet; it does not disable the card by itself.
 *
 * Callers: the update-policy cards and the "Inherit global" cadence card, both
 * in `repo-detail.tsx`.
 */
export function OptionCard({
  label,
  description,
  active,
  disabled = false,
  dimmed = false,
  onSelect,
}: {
  label: ReactNode;
  description: ReactNode;
  active: boolean;
  disabled?: boolean;
  dimmed?: boolean;
  onSelect: () => void;
}) {
  return (
    <button
      type="button"
      disabled={disabled || active}
      onClick={onSelect}
      className={cn(
        "flex items-start gap-3 rounded-md border px-3 py-2 text-left transition-colors disabled:cursor-not-allowed",
        active ? "border-primary bg-primary/10" : "border-border hover:bg-muted",
        dimmed && "opacity-50",
      )}
    >
      <RadioDot active={active} />
      <span className="min-w-0">
        <span className="block text-body font-medium">{label}</span>
        <span className="block text-cell text-muted-foreground">{description}</span>
      </span>
    </button>
  );
}
