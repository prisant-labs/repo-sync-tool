// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@/gallery/stub-ipc";
import { GalleryApp } from "@/gallery/gallery-app";
import { STRESS_GROUPS } from "@/gallery/fixtures-stress";

/**
 * The quiet Repos proposal. These pin what the proposal MEANS, not how it is
 * styled: it renders beside today's real screen, every state that needs
 * attention still names itself, no fact is said twice, and a healthy row is
 * still announced to assistive technology even though nothing is drawn.
 */
afterEach(() => {
  cleanup();
  document.documentElement.classList.remove("dark");
});

async function openSection(dataState?: string) {
  const user = userEvent.setup();
  render(<GalleryApp />);
  if (dataState) await user.click(screen.getByRole("button", { name: dataState }));
  await user.click(screen.getByRole("button", { name: "Quiet Repos table (proposal)" }));
  const today = await screen.findByRole("table", { name: "Tracked repositories" });
  const quiet = await screen.findByRole("table", { name: "Tracked repositories, quiet proposal" });
  return { today, quiet };
}

describe("quiet Repos proposal", () => {
  it("renders today's screen and the proposal, labelled as a proposal", async () => {
    await openSection();
    expect(screen.getByRole("region", { name: "Today" })).toBeTruthy();
    expect(screen.getByRole("region", { name: "Quiet (proposal, not shipped)" })).toBeTruthy();
  });

  it("still names every state that needs attention, and nothing healthy is lost", async () => {
    const { today, quiet } = await openSection();
    for (const word of ["Failed", "Dirty", "No upstream", "Paused"]) {
      expect(within(quiet).getByText(word), word).toBeTruthy();
    }
    // Healthy rows draw nothing, but the state is still there for a screen reader.
    expect(within(quiet).getAllByText("In sync").length).toBeGreaterThan(0);
    // Every fixture repository is a row in both tables.
    const rowCount = (t: HTMLElement) => within(t).getAllByRole("row").length;
    expect(rowCount(quiet)).toBe(rowCount(today));
  });

  it("says ahead and behind once, in their own columns", async () => {
    const { today, quiet } = await openSection();
    // Today's chip repeats the count the Ahead and Behind columns already show.
    expect(within(today).getByText("14 behind")).toBeTruthy();
    expect(within(quiet).queryByText(/\d+ (ahead|behind)/)).toBeNull();
    expect(within(quiet).getByText("14")).toBeTruthy();
    expect(within(quiet).getByText("3")).toBeTruthy();
  });

  it("keeps a filter for every status present", async () => {
    await openSection();
    const filters = screen.getByRole("group", { name: "Filter by status" });
    for (const label of ["All", "Behind", "Dirty", "Failed", "Paused", "No upstream", "Ahead", "In sync"]) {
      expect(within(filters).getByRole("button", { name: new RegExp(`^${label}`) }), label).toBeTruthy();
    }
  });

  it("hands both tables the groups of the chosen data state, so a stress-only group name renders in each", async () => {
    // A name that exists only in the stress groups, never in the populated fixture.
    const longName = STRESS_GROUPS.find((g) => g.name.startsWith("archive-of-things"))!.name;
    const { today, quiet } = await openSection("Stress (200)");
    expect(within(today).getAllByText(longName).length).toBeGreaterThan(0);
    expect(within(quiet).getAllByText(longName).length).toBeGreaterThan(0);
    // Two 200-row tables in jsdom: about 2 s locally, but past the 5 s default on the
    // Windows CI runner (PR #127's first run timed out). The generous limit fails a real hang
    // without failing a slow runner.
  }, 30_000);
});
