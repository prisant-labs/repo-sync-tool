import { useCallback, useEffect, useRef } from "react";
import type { KeyboardEvent, RefObject } from "react";

const FOCUSABLE_SELECTOR =
  'a[href], button:not([disabled]), textarea:not([disabled]), input:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])';

/**
 * `querySelectorAll` matches `FOCUSABLE_SELECTOR` regardless of CSS
 * visibility - it does not know about `hidden`, `display:none`, or `inert`.
 * A mounted-but-hidden subtree (the Tabs primitive keeps inactive
 * `TabPanel`s mounted and toggles the `hidden` attribute rather than
 * unmounting them, and a nested `Drawer`'s own `<aside>` stays mounted with
 * `inert` while closed) would otherwise pollute this trap's computed
 * first/last boundary with controls nobody can actually see or reach,
 * exactly the class of bug a real browser caught once already (N4 adversarial
 * review, finding 2). `closest` walks the element itself and every ancestor,
 * so this excludes anything inside either attribute, not just elements that
 * carry it directly.
 */
function focusableIn(container: HTMLElement): HTMLElement[] {
  return Array.from(container.querySelectorAll<HTMLElement>(FOCUSABLE_SELECTOR)).filter(
    (el) => el.closest("[hidden], [inert]") === null,
  );
}

/**
 * Minimal hand-rolled modal accessibility for `Drawer` / `Dialog` (no
 * focus-trap dependency exists in this codebase; findings 12/13, BL-NI-29).
 *
 * While `open`, focuses the first focusable descendant (or the container
 * itself, as a fallback) and restores focus to whatever had it beforehand
 * once the modal closes. Returns an `onKeyDown` handler the caller wires onto
 * the modal's root element, which traps Tab / Shift+Tab inside the container
 * and calls `onClose` on Escape.
 */
export function useModalA11y(
  open: boolean,
  onClose: () => void,
  containerRef: RefObject<HTMLElement | null>,
) {
  const previouslyFocused = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    previouslyFocused.current = document.activeElement as HTMLElement | null;
    const container = containerRef.current;
    const [first] = container ? focusableIn(container) : [];
    (first ?? container)?.focus();
    return () => {
      previouslyFocused.current?.focus?.();
    };
    // Re-run only on open/close; containerRef is a stable ref object.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  return useCallback(
    (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onClose();
        return;
      }
      if (e.key !== "Tab") return;
      const container = containerRef.current;
      const items = container ? focusableIn(container) : [];
      if (items.length === 0) {
        e.preventDefault();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    },
    [containerRef, onClose],
  );
}
