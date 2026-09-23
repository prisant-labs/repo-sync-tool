// @vitest-environment jsdom
import { act } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import { commands } from "@/lib/bindings";
import type { Diagnostics, Settings } from "@/lib/bindings";
import { mockCommand, ok } from "@/test/mock-ipc";
import { SettingsScreen } from "@/screens/settings";

/**
 * AC-3 and AC-4 (E-21, composite pins 28 and 29, register F.3 / STG1 and
 * STG4).
 *
 * AC-3: a docked nav lists Settings' own sections and marks the one
 * currently in view as the page scrolls, and every section is reachable by
 * keyboard. These tests check STRUCTURE (does a link exist per section, does
 * each href resolve to a real element, does the active marker move when the
 * scroll-spy observer reports a different section intersecting, is each
 * link a genuinely focusable element) rather than matching on label text,
 * so a rename of a section's visible label does not break them.
 *
 * AC-4: an About section names the running version and links out to the
 * releases page. It deliberately does NOT assert that the link opens a
 * browser - this app has no generic "open a URL" Tauri command or plugin
 * (see the comment above `AboutCard` in settings.tsx), so the link is a
 * plain anchor whose runtime behaviour inside the packaged webview is not
 * verified by a DOM-only test.
 */

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "9.9.9") }));

function settings(overrides: Partial<Settings> = {}): Settings {
  return {
    globalCheckMinutes: 60,
    quietHoursStart: null,
    quietHoursEnd: null,
    notifyOnRelease: true,
    notifyOnFailure: true,
    gitExecutablePath: null,
    editorCommand: null,
    terminalCommand: null,
    autostart: false,
    activityRetentionD: 30,
    githubTokenPresent: false,
    autoUpdateCheck: true,
    closeMinimizesToTray: true,
    ...overrides,
  };
}

// Copied from diagnostics-card.test.tsx's own `HEALTHY` fixture (not
// exported there) rather than imported, matching this codebase's existing
// per-file-fixture convention (see dashboard.test.tsx's own `repo()`).
function diagnostics(): Diagnostics {
  return {
    appVersion: "9.9.9",
    dataDir: "C:\\Users\\test\\AppData\\Local\\RepoSync",
    dbPath: "C:\\Users\\test\\AppData\\Local\\RepoSync\\reposync.db",
    logDir: "C:\\Users\\test\\AppData\\Local\\RepoSync\\logs",
    loggingActive: true,
    logLevel: "info",
    logMaxFiles: 14,
    logMaxBytes: 33554432,
    logDirReadable: true,
    logFileCount: 1,
    logBytes: 190,
    logWriteFailures: 0,
    logLastWriteFailureAt: null,
    logBytesWritten: 190,
    logDroppedLines: 0,
    onedriveRooted: false,
    gitPath: "C:\\Program Files\\Git\\cmd\\git.exe",
    gitVersion: "2.47.1",
    gitResolved: true,
    gitExplicitPath: null,
    gitExplicitPathHonored: null,
    gitMeetsFloor: true,
    schedulerCycles: 12,
    schedulerReposChecked: 36,
    schedulerOutcomePersistFailures: 0,
    dbRecovered: false,
  };
}

/**
 * jsdom does not implement `IntersectionObserver` at all, so the real one
 * cannot be exercised here. This stub records every element `observe()` is
 * called with (so a test can find "the element the component registered for
 * section X") and lets a test invoke the captured callback directly with a
 * fabricated entry, standing in for a real scroll crossing the observer's
 * threshold.
 */
class FakeIntersectionObserver {
  static instances: FakeIntersectionObserver[] = [];
  callback: IntersectionObserverCallback;
  observed: Element[] = [];

  constructor(callback: IntersectionObserverCallback) {
    this.callback = callback;
    FakeIntersectionObserver.instances.push(this);
  }
  observe(el: Element) {
    this.observed.push(el);
  }
  unobserve(el: Element) {
    this.observed = this.observed.filter((o) => o !== el);
  }
  disconnect() {
    this.observed = [];
  }
  takeRecords() {
    return [];
  }
}

function fireIntersecting(target: Element) {
  const observer = FakeIntersectionObserver.instances.at(-1);
  if (!observer) throw new Error("no IntersectionObserver was constructed");
  act(() => {
    observer.callback(
      [{ target, isIntersecting: true } as IntersectionObserverEntry],
      observer as unknown as IntersectionObserver,
    );
  });
}

beforeEach(() => {
  FakeIntersectionObserver.instances = [];
  vi.stubGlobal("IntersectionObserver", FakeIntersectionObserver);
  mockCommand(commands, "settingsGet", async () => ok(settings()));
  mockCommand(commands, "diagnosticsGet", async () => ok(diagnostics()));
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

/** Renders the screen and waits for every section (including the async ones behind `settingsGet`) to exist. */
async function renderSettings() {
  render(<SettingsScreen dark={false} onToggleTheme={() => {}} />);
  // The About section is the last one to mount and only exists once
  // `settingsGet` has resolved, so waiting for it is also waiting for every
  // earlier section.
  await screen.findByTestId("settings-section-about");
}

describe("Settings section nav (AC-3)", () => {
  it("lists one link per section, each pointing at a section that actually exists in the document", async () => {
    await renderSettings();

    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    const links = within(nav).getAllByRole("link");

    expect(links.length).toBeGreaterThan(1);
    for (const link of links) {
      const href = link.getAttribute("href");
      expect(href).toMatch(/^#/);
      const target = document.getElementById(href!.slice(1));
      expect(target).not.toBeNull();
    }
  });

  it("every section link is independently focusable and sits in the default tab order", async () => {
    await renderSettings();

    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    const links = within(nav).getAllByRole("link");
    expect(links.length).toBeGreaterThan(1);

    for (const link of links) {
      const el = link as HTMLElement;
      // `.focus()` alone would also succeed on `tabindex="-1"`, which is
      // NOT in the tab order - checking `tabIndex` (0 for a plain `<a
      // href>`, negative if a `tabindex="-1"` were ever added) is what
      // actually rules that out.
      expect(el.tabIndex).toBeGreaterThanOrEqual(0);
      el.focus();
      expect(document.activeElement).toBe(el);
    }
  });

  it("marks exactly one section current on initial render, before any scroll has happened", async () => {
    await renderSettings();

    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    const links = within(nav).getAllByRole("link");
    const current = links.filter((l) => l.hasAttribute("aria-current"));
    expect(current).toHaveLength(1);
  });

  it("moves the current marker to whichever section the scroll-spy observer reports intersecting, not only on click", async () => {
    await renderSettings();

    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    const initiallyCurrent = within(nav)
      .getAllByRole("link")
      .find((l) => l.hasAttribute("aria-current"));
    expect(initiallyCurrent).toBeDefined();

    // Pick a section that is NOT the initially-current one so the test can
    // tell a real move from a marker that never changes.
    const diagnosticsSection = screen.getByTestId("settings-section-diagnostics");
    const diagnosticsLink = within(nav)
      .getAllByRole("link")
      .find((l) => l.getAttribute("href") === "#diagnostics");
    expect(diagnosticsLink).toBeDefined();
    expect(diagnosticsLink!.hasAttribute("aria-current")).toBe(false);

    fireIntersecting(diagnosticsSection);

    expect(diagnosticsLink!.hasAttribute("aria-current")).toBe(true);
    expect(initiallyCurrent!.hasAttribute("aria-current")).toBe(false);
    const current = within(nav)
      .getAllByRole("link")
      .filter((l) => l.hasAttribute("aria-current"));
    expect(current).toHaveLength(1);
  });

  it("registers each section it lists with the scroll-spy observer", async () => {
    await renderSettings();

    const observer = FakeIntersectionObserver.instances.at(-1);
    expect(observer).toBeDefined();

    const nav = screen.getByRole("navigation", { name: "Settings sections" });
    const links = within(nav).getAllByRole("link");
    for (const link of links) {
      const id = link.getAttribute("href")!.slice(1);
      const section = document.getElementById(id);
      expect(observer!.observed).toContain(section);
    }
  });
});

describe("Settings About section (AC-4)", () => {
  it("names the running version", async () => {
    await renderSettings();

    const about = screen.getByTestId("settings-section-about");
    expect(await within(about).findByText("9.9.9")).toBeDefined();
  });

  it("offers NO outbound link, because the webview refuses one (AC-4, AC-12)", async () => {
    // CORRECTED after the audit of this change. The About section briefly carried
    // a "View releases" anchor. `on_navigation` at `src-tauri/src/lib.rs:440`
    // routes every navigation through `allow_navigation` (`lib.rs:272`), which
    // permits only the `tauri` scheme, the `tauri.localhost` host, and
    // `localhost` under `tauri dev`. An https anchor is refused, so that link
    // rendered, took keyboard focus, and did nothing.
    //
    // This asserts the link's ABSENCE rather than its shape. A control that
    // renders and cannot act is worse than no control - the exact thing
    // `src/lib/ui-reachability.test.ts` guards one level up. A working link needs
    // a backend opener command, which is a capability (AC-12), not this change.
    await renderSettings();

    const about = screen.getByTestId("settings-section-about");
    expect(within(about).queryAllByRole("link")).toHaveLength(0);
    // Still no release LIST either - AC-4 excludes that outright.
    expect(within(about).queryAllByRole("listitem")).toHaveLength(0);
  });
});
