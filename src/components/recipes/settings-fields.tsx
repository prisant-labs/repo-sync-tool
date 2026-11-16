import { useId } from "react";
import type { ReactNode } from "react";
import { Input } from "@/components/ui/input";
import { FieldLabelContext } from "@/components/ui/field-label";
import { hhMmToMinutes, minutesToHhMm } from "@/lib/time";

/**
 * The building blocks of a Settings row: `SettingsField` (a label, an optional
 * hint, and the control on the right) and the three inputs that go in it -
 * `NumberInput`, `TextInput` and `TimeInput`.
 *
 * Caller: `screens/settings.tsx`, which is the only place a setting is edited.
 * They live here so the gallery can show them without rendering the whole
 * Settings screen.
 *
 * The diagnostics card has a similar-looking row (`Row` in
 * `diagnostics-card.tsx`). It is a different layout (wider label column, a
 * value beside the label rather than a control, an optional action button) and
 * is left separate on purpose.
 */

// Owns the accessible-name association for whatever control it wraps
// (BL-NI-90). The label text carries a generated id and every descendant
// control reads it through `FieldLabelContext`, so a control placed in a
// `Field` is named without its call site doing anything - see
// `ui/field-label.tsx` for why the association is `aria-labelledby` rather
// than a `<label htmlFor>`.
//
// The hint is deliberately NOT part of the name. It is a sentence of
// explanation, and gluing it to the name would make a screen reader read the
// whole paragraph before saying on or off.
export function SettingsField({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  const labelId = useId();
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border px-4 py-3 last:border-b-0">
      <div className="min-w-0">
        <div id={labelId} className="text-body font-medium">
          {label}
        </div>
        {hint && <div className="text-cell text-muted-foreground">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">
        <FieldLabelContext.Provider value={labelId}>{children}</FieldLabelContext.Provider>
      </div>
    </div>
  );
}

export function NumberInput({
  value,
  min,
  max,
  suffix,
  onChange,
}: {
  value: number;
  min?: number;
  max?: number;
  suffix?: string;
  onChange: (value: number) => void;
}) {
  return (
    <div className="flex items-center gap-1.5">
      <Input
        type="number"
        className="w-20 text-right"
        value={value}
        min={min}
        max={max}
        onChange={(e) => onChange(Number(e.target.value))}
      />
      {suffix && <span className="text-cell text-muted-foreground">{suffix}</span>}
    </div>
  );
}

export function TextInput({
  value,
  placeholder,
  onChange,
}: {
  value: string | null;
  placeholder?: string;
  onChange: (value: string | null) => void;
}) {
  return (
    <Input
      className="w-48"
      value={value ?? ""}
      placeholder={placeholder}
      spellCheck={false}
      onChange={(e) => onChange(e.target.value === "" ? null : e.target.value)}
    />
  );
}

/** A native time picker bound to a minute-of-day integer (0..1439). */
// `aria-label` is REQUIRED here, unlike on `Switch` and `Input` where an
// enclosing `Field` supplies the name. Two TimeInputs share one `Field`
// ("Quiet window"), and a `Field` hands the SAME label id to every control
// inside it, so inheriting would name both endpoints identically and leave a
// screen-reader user unable to tell start from end - they could invert their
// own quiet hours. Caught by the Codex adversarial review of this change; the
// automatic naming was right for the twelve single-control fields and wrong
// for the one field with two.
export function TimeInput({
  value,
  onChange,
  "aria-label": ariaLabel,
}: {
  value: number;
  onChange: (value: number) => void;
  "aria-label": string;
}) {
  return (
    <Input
      type="time"
      className="w-32"
      aria-label={ariaLabel}
      value={minutesToHhMm(value)}
      onChange={(e) => onChange(hhMmToMinutes(e.target.value))}
    />
  );
}

