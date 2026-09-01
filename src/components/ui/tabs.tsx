import { createContext, useContext, useId, useRef } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * Hand-rolled ARIA tabs (D2: jp decided against Radix for N4, 2026-08-31).
 * Full pattern: role=tablist/tab/tabpanel, aria-selected, roving tabindex,
 * Left/Right/Home/End, automatic activation (moving focus also selects -
 * the W3C APG's simpler of its two variants, and the one that keeps focus
 * and selection from ever being able to diverge: there is no state where a
 * tab is focused but a DIFFERENT tab is still marked selected, so the
 * arrow-key math and `aria-selected` can never disagree about which tab is
 * "current").
 *
 * `TabPanel` keeps ALL panels MOUNTED and toggles the native `hidden`
 * attribute, rather than unmounting inactive ones. Two reasons, both from
 * the Codex adversarial review of the first N4 cut (finding 2, confirmed):
 * every `tab` needs a STABLE `aria-controls` naming its panel per the W3C
 * pattern, which is impossible to give an inactive tab if that panel does
 * not exist in the DOM; and switching tabs should not discard a panel's own
 * scroll position, which unmounting did as a side effect. `hidden` still
 * needs a companion fix: `querySelectorAll` (which the shared modal focus
 * trap in `use-modal-a11y.ts` uses to compute Tab/Shift+Tab boundaries)
 * finds focusable descendants regardless of CSS visibility, so a
 * mounted-but-hidden panel's own controls would otherwise pollute that
 * trap's first/last calculation - `focusableIn` there now filters out
 * anything inside a `[hidden]` or `[inert]` ancestor for exactly this case.
 */

interface TabsContextValue {
  value: string;
  setValue: (v: string) => void;
  baseId: string;
}

const TabsContext = createContext<TabsContextValue | null>(null);

function useTabsContext(name: string): TabsContextValue {
  const ctx = useContext(TabsContext);
  if (!ctx) throw new Error(`<${name}> must render inside <Tabs>`);
  return ctx;
}

export function Tabs({
  value,
  onValueChange,
  className,
  children,
}: {
  value: string;
  onValueChange: (value: string) => void;
  className?: string;
  children: ReactNode;
}) {
  const baseId = useId();
  return (
    <TabsContext.Provider value={{ value, setValue: onValueChange, baseId }}>
      <div className={className}>{children}</div>
    </TabsContext.Provider>
  );
}

export function TabList({
  tabs,
  "aria-label": ariaLabel,
  className,
}: {
  tabs: { value: string; label: string }[];
  "aria-label": string;
  className?: string;
}) {
  const { value, setValue, baseId } = useTabsContext("TabList");
  const buttonRefs = useRef<Record<string, HTMLButtonElement | null>>({});

  function activate(next: string) {
    setValue(next);
    buttonRefs.current[next]?.focus();
  }

  /**
   * The arrow-key math advances from whichever tab is CURRENTLY FOCUSED
   * (`e.target`, the actual DOM element the keydown bubbled from), never
   * from the "selected" `value` in context. Those are the same tab in every
   * path this component itself drives (`activate` always focuses the tab it
   * just selected), but the two are only guaranteed identical if nothing
   * else can move focus onto a tab without going through `activate` first -
   * a roving-tabindex button (`tabIndex={-1}` on every inactive tab) is
   * still a legal target for PROGRAMMATIC focus even though it is excluded
   * from sequential Tab navigation, so an external `.focus()` call (a
   * screen reader's browse-mode cursor, or any future caller) could land on
   * an inactive tab without this component knowing about it. Keying off
   * `e.target` means the arrow keys always operate on reality - whatever
   * the browser says has focus right now - rather than on this component's
   * own possibly-stale belief about it (Codex adversarial review, finding
   * 2b, confirmed).
   */
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const target = e.target as HTMLElement;
    const i = tabs.findIndex((t) => buttonRefs.current[t.value] === target);
    if (i === -1) return;
    if (e.key === "ArrowRight") {
      e.preventDefault();
      activate(tabs[(i + 1) % tabs.length].value);
    } else if (e.key === "ArrowLeft") {
      e.preventDefault();
      activate(tabs[(i - 1 + tabs.length) % tabs.length].value);
    } else if (e.key === "Home") {
      e.preventDefault();
      activate(tabs[0].value);
    } else if (e.key === "End") {
      e.preventDefault();
      activate(tabs[tabs.length - 1].value);
    }
  }

  return (
    <div
      role="tablist"
      aria-label={ariaLabel}
      onKeyDown={onKeyDown}
      className={cn("flex gap-1 border-b border-border", className)}
    >
      {tabs.map((t) => {
        const active = t.value === value;
        return (
          <button
            key={t.value}
            ref={(el) => {
              buttonRefs.current[t.value] = el;
            }}
            type="button"
            role="tab"
            id={`${baseId}-tab-${t.value}`}
            aria-selected={active}
            // Every tab names its panel, active or not - the W3C tabs
            // pattern requires aria-controls on every tab, and TabPanel now
            // keeps every panel mounted (hidden, not removed), so the id
            // this points at always resolves to a real element.
            aria-controls={`${baseId}-panel-${t.value}`}
            tabIndex={active ? 0 : -1}
            onClick={() => setValue(t.value)}
            // Automatic activation means focus IS selection - completing
            // that model requires selection to follow focus by whatever
            // path focus arrives (not only this component's own arrow-key
            // `activate`), so a tab focused programmatically (a screen
            // reader's browse-mode cursor, a future caller) is immediately
            // the selected one too, and never sits in a
            // focused-but-unselected state. Idempotent with `activate`,
            // which already focuses the tab it selects.
            onFocus={() => setValue(t.value)}
            className={cn(
              "-mb-px border-b-2 px-3 py-2 text-sm font-medium transition-colors",
              active
                ? "border-primary text-foreground"
                : "border-transparent text-muted-foreground hover:text-foreground",
            )}
          >
            {t.label}
          </button>
        );
      })}
    </div>
  );
}

export function TabPanel({
  value,
  className,
  children,
}: {
  value: string;
  className?: string;
  children: ReactNode;
}) {
  const { value: active, baseId } = useTabsContext("TabPanel");
  const isHidden = value !== active;
  // Stays mounted regardless of `active`; see the file doc comment for why
  // (a stable `aria-controls` target on every tab, and per-panel scroll
  // position surviving a switch away and back). `hidden` removes it from
  // layout, paint, and the accessibility tree without discarding its DOM
  // state; `use-modal-a11y.ts`'s `focusableIn` excludes anything inside a
  // `[hidden]` ancestor from the modal focus trap's own boundary
  // calculation, so an inactive panel's controls cannot leak into it.
  //
  // The `hidden` BOOLEAN ATTRIBUTE alone is not enough: every caller's
  // `className` carries a `display` utility of its own (`flex`, in every
  // current usage), and an author-origin utility class always beats the
  // browser's default `[hidden] { display: none }` UA rule, regardless of
  // specificity - author CSS outranks the UA stylesheet unconditionally.
  // Left alone, the panel would keep `display: flex` and stay fully
  // visible despite carrying `hidden`. Appending Tailwind's own `hidden`
  // UTILITY class through `cn` (backed by `tailwind-merge`, which treats
  // `hidden`/`flex`/`grid`/etc. as one mutually-exclusive "display" group)
  // drops whatever display utility `className` supplied and keeps `hidden`
  // instead, so the attribute and the rendered style agree.
  return (
    <div
      role="tabpanel"
      id={`${baseId}-panel-${value}`}
      aria-labelledby={`${baseId}-tab-${value}`}
      tabIndex={0}
      hidden={isHidden}
      className={cn(className, isHidden && "hidden")}
    >
      {children}
    </div>
  );
}
