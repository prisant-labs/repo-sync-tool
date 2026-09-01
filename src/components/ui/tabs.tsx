import { createContext, useContext, useId, useRef } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import { cn } from "@/lib/utils";

/**
 * Hand-rolled ARIA tabs (D2: jp decided against Radix for N4, 2026-08-31).
 * Full pattern: role=tablist/tab/tabpanel, aria-selected, roving tabindex,
 * Left/Right/Home/End. Automatic activation (moving focus also selects),
 * the simplest correct model for a small, static strip like this one.
 *
 * `TabPanel` UNMOUNTS its children when inactive rather than hiding them
 * with the `hidden` attribute. The drawer's shared focus trap
 * (`use-modal-a11y.ts`) computes its Tab/Shift+Tab boundaries with
 * `querySelectorAll`, which finds focusable descendants regardless of CSS
 * visibility - a `hidden` (display:none) panel's own controls would still
 * enter that list and could become the trap's wrongly-computed "last"
 * element. Unmounting removes them from the DOM entirely, so the trap only
 * ever sees what is actually visible; the cost is that a hidden tab's own
 * scroll position resets when revisited, which is an acceptable trade here.
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

  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    const i = tabs.findIndex((t) => t.value === value);
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
            // Only the active tab names a panel that actually exists in the DOM
            // (TabPanel unmounts inactive content, see the file doc comment).
            aria-controls={active ? `${baseId}-panel-${t.value}` : undefined}
            tabIndex={active ? 0 : -1}
            onClick={() => setValue(t.value)}
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
  if (value !== active) return null;
  return (
    <div
      role="tabpanel"
      id={`${baseId}-panel-${value}`}
      aria-labelledby={`${baseId}-tab-${value}`}
      tabIndex={0}
      className={className}
    >
      {children}
    </div>
  );
}
