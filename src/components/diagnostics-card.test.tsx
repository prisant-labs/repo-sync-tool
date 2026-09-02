// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { commands } from "@/lib/bindings";
import type { Diagnostics } from "@/lib/bindings";
import { mockCommand, ok } from "@/test/mock-ipc";
import { DiagnosticsCard } from "@/components/diagnostics-card";

/**
 * The bug these tests exist to prevent: collapsing `GitAvailability`'s THREE
 * states into one boolean.
 *
 * This codebase has already made that mistake once. PR #40 shipped a
 * `git_available: boolean` payload in which a below-floor git mapped to
 * `false`, documented as "whether git is usable" - which contradicts
 * `crates/reposync-core/src/git/mod.rs`, where a below-floor git is still run
 * and merely flagged (E-03 AC7). A user would have read "git: not found" beside
 * a fully populated git path. It was caught in a pre-merge read, not by a gate,
 * because no gate could see it.
 *
 * So the assertions here are deliberately about MEANING rather than markup: that
 * the absent case and the below-floor case produce different, non-interchangeable
 * text. A restyle should not touch these; a re-collapse should break all three.
 */

afterEach(cleanup);

/**
 * A healthy snapshot. Every test overrides only the fields it is about, so a
 * failure names the field that broke rather than the whole payload.
 */
const HEALTHY: Diagnostics = {
  appVersion: "0.9.0",
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

function renderWith(overrides: Partial<Diagnostics>) {
  mockCommand(commands, "diagnosticsGet", async () => ok({ ...HEALTHY, ...overrides }));
  return render(<DiagnosticsCard />);
}

describe("DiagnosticsCard git states", () => {
  it("reports a usable git by its version", async () => {
    renderWith({ gitResolved: true, gitMeetsFloor: true, gitVersion: "2.47.1" });

    expect(await screen.findByText("2.47.1")).toBeDefined();
    expect(screen.queryByText(/not found/i)).toBeNull();
    expect(screen.queryByText(/below 2\.30/i)).toBeNull();
  });

  it("reports an absent git as not found", async () => {
    renderWith({ gitResolved: false, gitMeetsFloor: false, gitVersion: null, gitPath: null });

    expect(await screen.findByText(/not found/i)).toBeDefined();
    expect(screen.queryByText(/below 2\.30/i)).toBeNull();
  });

  it("reports a below-floor git as its version plus a floor warning, never as absent", async () => {
    renderWith({ gitResolved: true, gitMeetsFloor: false, gitVersion: "2.20.0" });

    // The whole point: a below-floor git IS resolved, IS run, and must not be
    // described with the same words as a missing one.
    expect(await screen.findByText(/2\.20\.0 \(below 2\.30\)/i)).toBeDefined();
    expect(screen.queryByText(/not found/i)).toBeNull();
  });

  it("keeps the absent and below-floor cases textually distinct", async () => {
    renderWith({ gitResolved: false, gitMeetsFloor: false, gitVersion: null, gitPath: null });
    const absent = (await screen.findByText(/not found/i)).textContent;
    cleanup();

    renderWith({ gitResolved: true, gitMeetsFloor: false, gitVersion: "2.20.0" });
    const belowFloor = (await screen.findByText(/below 2\.30/i)).textContent;

    // A single boolean cannot produce two different strings here. That is the
    // regression this pins, stated as directly as it can be.
    expect(absent).not.toEqual(belowFloor);
  });

  it("still shows the git path when the version is below the floor", async () => {
    renderWith({
      gitResolved: true,
      gitMeetsFloor: false,
      gitVersion: "2.20.0",
      gitPath: "C:\\old\\git.exe",
    });

    // A user told "below floor" needs to know WHICH git that is, and the path
    // row is the only thing that answers it.
    expect(await screen.findByText("C:\\old\\git.exe")).toBeDefined();
  });
});

describe("DiagnosticsCard warnings", () => {
  it("says nothing when everything is healthy", async () => {
    renderWith({});

    // Wait for the panel to resolve before asserting an absence, otherwise this
    // passes against a still-loading card.
    await screen.findByText("2.47.1");
    expect(screen.queryByText(/could not be read/i)).toBeNull();
    expect(screen.queryByText(/did not start/i)).toBeNull();
    expect(screen.queryByText(/no log files/i)).toBeNull();
  });

  it("distinguishes an unreadable log folder from an empty one", async () => {
    renderWith({ logDirReadable: false, logFileCount: 0, logBytes: 0 });

    // "We looked and found nothing" and "we could not look" are different facts,
    // and only the second is itself a problem.
    expect(await screen.findByText(/cannot be read/i)).toBeDefined();
    expect(screen.queryByText(/contains no log files/i)).toBeNull();
  });

  it("prefers a counted write failure over the empty-folder inference", async () => {
    renderWith({
      logWriteFailures: 3,
      logLastWriteFailureAt: 1_700_000_000,
      logFileCount: 0,
      logBytesWritten: 0,
    });

    // The writer reporting its own io errors is a more specific fact than the UI
    // deducing trouble from an empty directory, so it wins and the weaker
    // message stays out of the way.
    expect(await screen.findByText(/failed 3 time/i)).toBeDefined();
    expect(screen.queryByText(/contains no log files/i)).toBeNull();
    expect(screen.queryByText(/nothing has reached the writer/i)).toBeNull();
  });

  it("names a dropped line as a gap rather than a failure", async () => {
    renderWith({ logDroppedLines: 7 });

    // A dropped line means RepoSync chose to lose output rather than stall the
    // work producing it. Calling that a write failure sends someone to check
    // folder permissions that are working correctly.
    const msg = await screen.findByText(/7 log line\(s\) were dropped/i);
    expect(msg.textContent).toMatch(/nothing is broken/i);
  });

  it("flags a git path that was configured and then ignored", async () => {
    renderWith({
      gitExplicitPath: "C:\\typo\\git.exe",
      gitExplicitPathHonored: false,
      gitPath: "C:\\Program Files\\Git\\cmd\\git.exe",
    });

    // Silent until PR #52: Settings showed the configured path, Diagnostics
    // showed the resolved one, and nothing compared them.
    const msg = await screen.findByText(/could not be used/i);
    expect(msg.textContent).toContain("C:\\typo\\git.exe");
  });

  /**
   * N7 consistency sweep, Q1 -> 1A: the warnings band moved from a tinted
   * `bg-status-failed/10` fill to a neutral stripe, but the message ORDER is
   * explicitly load-bearing (coverage-matrix.md section 7a) and untouched by
   * that restyle. Every existing test above pins one condition (or a
   * precedence PAIR) in isolation; this one triggers five independent
   * conditions at once and pins the FULL sequence, which nothing above does.
   */
  it("orders every independent warning exactly as the source lists them, even with five true at once", async () => {
    renderWith({
      gitExplicitPath: "C:\\typo\\git.exe",
      gitExplicitPathHonored: false,
      gitPath: "C:\\Program Files\\Git\\cmd\\git.exe",
      logDroppedLines: 4,
      onedriveRooted: true,
      schedulerOutcomePersistFailures: 2,
      dbRecovered: true,
    });

    // Each phrase below is unique to its OWN warning message, deliberately
    // distinct from the "Scheduled checks since launch" row's own hint text
    // ("...outcomes that could not be saved"), which would otherwise
    // false-match the scheduler warning's more generic wording and collapse
    // two elements into what looks like one.
    const messages = await screen.findAllByText(
      /could not be used, so RepoSync fell back to|were dropped because RepoSync|OneDrive-synced tree|Those repos retried|moved aside/i,
    );
    const order = messages.map((m) => m.textContent ?? "");
    expect(order).toHaveLength(5);
    expect(order[0]).toMatch(/^The git path set in Settings/);
    expect(order[1]).toMatch(/^4 log line\(s\) were dropped/);
    expect(order[2]).toMatch(/^Your data folder is inside a OneDrive-synced tree/);
    expect(order[3]).toMatch(/^2 scheduled check outcome\(s\) could not be saved/);
    expect(order[4]).toMatch(/^The database could not be migrated at startup/);
  });
});

describe("DiagnosticsCard row shape (N7, Q2 -> 2A)", () => {
  it("keeps every row's label, hint and value after the unified fact-row reshape", async () => {
    renderWith({});

    // Label + hint text is untouched content, per the matrix's KEEP on row
    // content and order - only the shape changed.
    expect(await screen.findByText("Log folder")).toBeDefined();
    expect(screen.getByText("Everything RepoSync records goes here.")).toBeDefined();
    expect(screen.getByText("RepoSync version")).toBeDefined();
    expect(screen.getByText("0.9.0")).toBeDefined();
  });

  it("keeps the Log folder row's action button reachable by role", async () => {
    renderWith({});

    expect(await screen.findByRole("button", { name: /open logs/i })).toBeDefined();
  });

  it("keeps the full-width PathRow treatment for every path row", async () => {
    renderWith({});

    expect(await screen.findByText(HEALTHY.logDir)).toBeDefined();
    expect(screen.getByText(HEALTHY.dataDir)).toBeDefined();
    expect(screen.getByText(HEALTHY.dbPath)).toBeDefined();
  });

  it("still shows the copy-footer privacy sentence untouched", async () => {
    renderWith({});

    expect(
      await screen.findByText(
        "Copying includes the folder paths above, which contain your Windows account name.",
      ),
    ).toBeDefined();
    expect(screen.getByRole("button", { name: /copy details/i })).toBeDefined();
  });
});
