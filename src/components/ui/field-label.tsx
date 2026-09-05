import { createContext, useContext } from "react";

/**
 * The id of the visible label text rendered by an enclosing `Field`.
 *
 * BL-NI-90 (Settings switches have no accessible name) closed on this. A
 * `Field` draws its label in a plain `<div>` beside the control, so nothing
 * associated the two and every switch in Settings announced as "switch, on"
 * with no indication of WHICH setting. Six of them shipped that way, one for
 * two months without even appearing in the backlog row tracking it.
 *
 * The row named two possible fixes and called the second the right one:
 * `aria-label` at each call site, or making `Field` own the association so it
 * is fixed for every control type at once rather than switch by switch. This
 * is the second. A control inside a `Field` gets its accessible name for free,
 * and a control type added later inherits the fix rather than repeating the
 * bug.
 *
 * WHY `aria-labelledby` RATHER THAN THE ROW'S LITERAL `<label htmlFor>`: the
 * association has to work for `Switch`, which is a `<button role="switch">`.
 * A button's accessible name comes from its own content first, and a `<label
 * for>` pointing at one is not honored consistently across assistive
 * technology - the very reason `Switch` needed an explicit name to begin with.
 * `aria-labelledby` is unambiguous for every role here, buttons and inputs
 * alike, so it serves what the row asked for even though it spells it
 * differently.
 *
 * `undefined` outside a `Field`, which is why every consumer keeps an explicit
 * `aria-label` escape hatch for controls that live somewhere else.
 */
export const FieldLabelContext = createContext<string | undefined>(undefined);

/** The enclosing `Field`'s label id, or `undefined` when there is no `Field`. */
export function useFieldLabelId(): string | undefined {
  return useContext(FieldLabelContext);
}
