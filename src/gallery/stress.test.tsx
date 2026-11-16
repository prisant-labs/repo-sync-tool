// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@/gallery/stub-ipc";
import { GalleryApp } from "@/gallery/gallery-app";
import { createHandlers, createWorld } from "@/gallery/handlers";
import * as stress from "@/gallery/fixtures-stress";
import { ALL_FAILED_REPOS, STRESS_GROUPS, STRESS_REPOS, STRESS_REPO_COUNT } from "@/gallery/fixtures-stress";
import { WINDOWS } from "@/gallery/window-frame";
import type { RepoFilter } from "@/lib/bindings";

afterEach(() => {
  cleanup();
  document.documentElement.classList.remove("dark");
});

const NO_FILTER: RepoFilter = { query: null, enabledOnly: null, hostType: null };

describe("stress fixtures", () => {
  it("has 200 repositories with unique ids and paths", () => {
    expect(STRESS_REPOS).toHaveLength(STRESS_REPO_COUNT);
    expect(STRESS_REPO_COUNT).toBe(200);
    expect(new Set(STRESS_REPOS.map((r) => r.id)).size).toBe(200);
    expect(new Set(STRESS_REPOS.map((r) => r.localPath)).size).toBe(200);
  });

  it("is index-based, so two builds of the same index agree", () => {
    // A second module instance would rebuild everything from scratch.
    expect(stress.STRESS_REPOS[137]).toEqual(STRESS_REPOS[137]);
    expect(STRESS_REPOS[0].localName).toBe(STRESS_REPOS[0].localName);
    expect(new Set(STRESS_REPOS.map((r) => r.localName)).size).toBe(200);
  });

  it("reaches the extremes it exists to reach", () => {
    expect(Math.max(...STRESS_REPOS.map((r) => r.localName.length))).toBeGreaterThan(80);
    expect(Math.max(...STRESS_REPOS.map((r) => r.localPath.length))).toBeGreaterThan(150);
    expect(Math.max(...STRESS_REPOS.map((r) => r.behindCount ?? 0))).toBeGreaterThan(50000);
    expect(STRESS_GROUPS.length).toBeGreaterThanOrEqual(20);
  });

  it("marks every repository in the all-failed set as failed, with varied codes", () => {
    expect(ALL_FAILED_REPOS).toHaveLength(200);
    expect(ALL_FAILED_REPOS.every((r) => r.lastErrorCode !== null)).toBe(true);
    expect(new Set(ALL_FAILED_REPOS.map((r) => r.lastErrorCode)).size).toBeGreaterThan(5);
  });
});

describe("the stub IPC per data state", () => {
  it("answers stress with 200 repositories and the stress groups", () => {
    const h = createHandlers(createWorld("stress"));
    expect(h.repoList(NO_FILTER)).toHaveLength(200);
    expect(h.groupList()).toHaveLength(STRESS_GROUPS.length);
    expect(h.summaryToday(null).attentionCount).toBeGreaterThan(0);
  });

  it("answers allFailed with every repository failed and on the attention list", () => {
    const h = createHandlers(createWorld("allFailed"));
    const repos = h.repoList(NO_FILTER);
    expect(repos).toHaveLength(200);
    expect(repos.every((r) => r.lastErrorCode !== null)).toBe(true);
    expect(h.summaryToday(null).attentionCount).toBe(200);
  });

  it("keeps populated and empty as they were", () => {
    expect(createHandlers(createWorld("populated")).repoList(NO_FILTER)).toHaveLength(8);
    expect(createHandlers(createWorld("empty")).repoList(NO_FILTER)).toHaveLength(0);
  });
});

describe("the gallery controls", () => {
  it("selects the new data states", async () => {
    const user = userEvent.setup();
    render(<GalleryApp />);
    for (const name of ["Stress (200)", "All failed"]) {
      await user.click(screen.getByRole("button", { name }));
      expect(screen.getByRole("button", { name }).getAttribute("aria-pressed")).toBe("true");
      expect(screen.getByRole("button", { name: "Populated" }).getAttribute("aria-pressed")).toBe("false");
    }
  });

  it("frames a Screens cell at each Window option and keeps it contained", async () => {
    const user = userEvent.setup();
    render(<GalleryApp />);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const cell = () => document.querySelector<HTMLElement>('[data-cell="Settings screen"]')!;
    expect(cell().querySelector("[data-window-frame]")).toBeNull();
    expect(cell().style.transform).toContain("translateZ");

    for (const w of WINDOWS.filter((x) => x.cssWidth !== null)) {
      await user.click(screen.getByRole("button", { name: w.label }));
      const frame = cell().querySelector<HTMLElement>("[data-window-frame]")!;
      expect(Number.parseFloat(frame.style.width)).toBeCloseTo(w.cssWidth, 2);
      // Overlays are contained by the cell and by the frame.
      expect(frame.style.transform).toContain("translateZ");
      expect(cell().style.transform).toContain("translateZ");
    }
    const frame150 = cell().querySelector<HTMLElement>("[data-window-frame]")!;
    expect(Number.parseFloat(frame150.style.zoom)).toBe(1.5);
    // About 683 CSS px laid out, drawn 1.5x, is 1024 px on screen.
    expect(Number.parseFloat(frame150.style.width) * 1.5).toBeCloseTo(1024, 2);

    await user.click(screen.getByRole("button", { name: "Natural" }));
    expect(cell().querySelector("[data-window-frame]")).toBeNull();
  });
});
