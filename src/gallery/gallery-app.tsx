import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { ToastProvider } from "@/components/ui/toast";
import { SECTIONS, GROUP_ORDER } from "@/gallery/sections";
import { setScenario } from "@/gallery/stub-ipc";
import type { Scenario } from "@/gallery/fixtures";

/** The looks the gallery can show. Later slices add more; today there is one. */
const LOOKS = ["today"] as const;
type Look = (typeof LOOKS)[number];

const SCENARIOS: { value: Scenario; label: string }[] = [
  { value: "populated", label: "Populated" },
  { value: "empty", label: "Empty" },
  { value: "loading", label: "Loading" },
];

/**
 * The gallery shell: a docked section list, a control bar, and one section at
 * a time. Everything switches in-page, because a double-clicked file has no
 * query string and no server.
 *
 * One section at a time rather than one long page, on purpose. Several real
 * overlays (drawer, dialog) focus themselves on mount; mounting many at once
 * would make them fight over focus and scroll the page to the last one.
 */
export function GalleryApp() {
  const [sectionId, setSectionId] = useState(SECTIONS[0].id);
  const [dark, setDark] = useState(false);
  const [look, setLook] = useState<Look>("today");
  const [scenario, setScenarioState] = useState<Scenario>("populated");
  // Bumped on every scenario change so the section remounts and its data hooks
  // refetch under the new scenario.
  const [nonce, setNonce] = useState(0);

  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
  }, [dark]);

  function chooseScenario(next: Scenario) {
    // The stub is outside React, so set it before the remount that re-reads it.
    setScenario(next);
    setScenarioState(next);
    setNonce((n) => n + 1);
  }

  const section = SECTIONS.find((s) => s.id === sectionId) ?? SECTIONS[0];

  return (
    <div
      data-look={look}
      className="flex h-screen bg-background text-foreground"
      data-testid="gallery-root"
    >
      <nav
        aria-label="Gallery sections"
        className="flex w-60 shrink-0 flex-col gap-4 overflow-y-auto border-r border-border bg-sidebar px-3 py-4"
      >
        <div className="px-2 text-body font-semibold">RepoSync gallery</div>
        {GROUP_ORDER.map((group) => (
          <div key={group} className="flex flex-col gap-0.5">
            <div className="px-2 pb-1 font-mono text-meta font-bold tracking-wider text-muted-foreground uppercase">
              {group}
            </div>
            {SECTIONS.filter((s) => s.group === group).map((s) => (
              <button
                key={s.id}
                type="button"
                data-section={s.id}
                aria-current={s.id === sectionId ? "page" : undefined}
                onClick={() => setSectionId(s.id)}
                className={cn(
                  "rounded-md px-2 py-1.5 text-left text-body",
                  s.id === sectionId
                    ? "bg-sidebar-accent font-semibold text-foreground"
                    : "text-muted-foreground hover:text-foreground",
                )}
              >
                {s.title}
              </button>
            ))}
          </div>
        ))}
      </nav>

      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex flex-wrap items-center gap-x-6 gap-y-2 border-b border-border px-6 py-3 text-body">
          <label className="flex items-center gap-2">
            <input type="checkbox" checked={dark} onChange={(e) => setDark(e.target.checked)} />
            Dark
          </label>
          <label className="flex items-center gap-2">
            Look
            <select
              value={look}
              onChange={(e) => setLook(e.target.value as Look)}
              className="rounded-md border border-input bg-card px-2 py-1"
            >
              {LOOKS.map((l) => (
                <option key={l} value={l}>
                  {l}
                </option>
              ))}
            </select>
          </label>
          <div className="flex items-center gap-2" role="group" aria-label="Data state">
            <span className="text-muted-foreground">Data</span>
            {SCENARIOS.map((s) => (
              <button
                key={s.value}
                type="button"
                aria-pressed={scenario === s.value}
                onClick={() => chooseScenario(s.value)}
                className={cn(
                  "rounded-md border px-2.5 py-1",
                  scenario === s.value
                    ? "border-primary bg-primary/10 font-semibold text-primary-ink"
                    : "border-border text-muted-foreground hover:text-foreground",
                )}
              >
                {s.label}
              </button>
            ))}
          </div>
        </header>

        <main className="min-h-0 flex-1 overflow-y-auto p-6" data-testid="gallery-main">
          <h1 className="mb-1 text-title font-semibold">{section.title}</h1>
          {section.note && <p className="mb-5 max-w-3xl text-body text-muted-foreground">{section.note}</p>}
          <ToastProvider key={`${section.id}-${nonce}`}>{section.render()}</ToastProvider>
        </main>
      </div>
    </div>
  );
}
