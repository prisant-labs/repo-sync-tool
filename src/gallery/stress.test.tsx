// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@/gallery/stub-ipc";
import { GalleryApp } from "@/gallery/gallery-app";
import { createHandlers, createWorld } from "@/gallery/handlers";
import * as stress from "@/gallery/fixtures-stress";
import { ALL_FAILED_REPOS, STRESS_GROUPS, STRESS_REPOS, STRESS_REPO_COUNT } from "@/gallery/fixtures-stress";
import { encodeFrameHash, parseFrameHash } from "@/gallery/window-frame";
import { deriveStatus } from "@/lib/status";
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

  it("shows a Screens cell in an iframe of the chosen viewport size, and keeps the cell contained", async () => {
    const user = userEvent.setup();
    render(<GalleryApp />);
    await user.click(screen.getByRole("button", { name: "Settings" }));
    const cell = () => document.querySelector<HTMLElement>('[data-cell="Settings screen"]')!;
    expect(cell().querySelector("iframe")).toBeNull();
    expect(cell().style.transform).toContain("translateZ");

    await user.click(screen.getByRole("button", { name: "Stress (200)" }));
    await user.click(screen.getByRole("button", { name: "1024px" }));
    const f1 = cell().querySelector<HTMLIFrameElement>("iframe")!;
    expect(f1.style.width).toBe("1024px");
    expect(f1.style.transform).toBe("");
    expect(parseFrameHash(new URL(f1.src).hash)).toMatchObject({ screen: "settings", scenario: "stress", look: "today", dark: false });

    await user.click(screen.getByRole("button", { name: "1024px at 150%" }));
    const f2 = cell().querySelector<HTMLIFrameElement>("iframe")!;
    // 683 CSS px of viewport, drawn 1.5x from the top left, in a wrapper of the scaled size.
    expect(f2.style.width).toBe("683px");
    expect(f2.style.transform).toBe("scale(1.5)");
    expect(f2.style.transformOrigin).toBe("top left");
    const wrapper = cell().querySelector<HTMLElement>("[data-window-wrapper]")!;
    expect(Number.parseFloat(wrapper.style.width)).toBeCloseTo(1024.5, 1);
    expect(cell().style.transform).toContain("translateZ");

    await user.click(screen.getByRole("button", { name: "Natural" }));
    expect(cell().querySelector("iframe")).toBeNull();
  });
});

describe("the frame hash", () => {
  it("round-trips a spec and ignores a hash that names no screen", () => {
    const spec = { screen: "repos", scenario: "allFailed", look: "slate", dark: true } as const;
    expect(parseFrameHash(encodeFrameHash(spec))).toEqual(spec);
    expect(parseFrameHash("")).toBeNull();
    expect(parseFrameHash("#frame=nonsense")).toBeNull();
  });
});

describe("fixture invariants", () => {
  const SETS = [
    ["stress", STRESS_REPOS],
    ["all failed", ALL_FAILED_REPOS],
  ] as const;

  it("derives Failed for every repository in the all-failed state", () => {
    for (const r of ALL_FAILED_REPOS) expect(deriveStatus(r), r.localName).toBe("failed");
  });

  for (const [name, repos] of SETS) {
    it(`never contradicts itself about HEAD (${name})`, () => {
      for (const r of repos) {
        // `isDetached` and `headState === "detached"` are the same fact.
        expect(r.isDetached, r.localName).toBe(r.headState === "detached");
        // Only a repo on a branch has a branch name; detached, unborn and
        // not-observed HEADs have none.
        expect(r.activeBranch !== null, r.localName).toBe(r.headState === "branch");
        // No branch means no upstream, so nothing to count ahead or behind.
        if (r.headState !== "branch") {
          expect(r.upstreamState, r.localName).toBeNull();
          expect(r.aheadCount, r.localName).toBeNull();
          expect(r.behindCount, r.localName).toBeNull();
        }
      }
    });

    it(`covers every head state (${name})`, () => {
      expect(new Set(repos.map((r) => r.headState))).toEqual(new Set(["branch", "detached", "unborn", null]));
    });
  }

  it("reaches the real Repos column's detached rendering", async () => {
    const user = userEvent.setup();
    render(<GalleryApp />);
    await user.click(screen.getByRole("button", { name: "Stress (200)" }));
    await user.click(screen.getByRole("button", { name: "Repos" }));
    // The first detached repo is index 17, so the row exists in the 200.
    expect(STRESS_REPOS.some((r) => r.headState === "detached")).toBe(true);
    await waitFor(() => expect(screen.getAllByText("detached").length).toBeGreaterThan(0));
  });
});
