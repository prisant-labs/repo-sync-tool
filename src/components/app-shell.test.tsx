// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type { DailySummary, GroupSummary, RepoSummary } from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { AppShell } from "@/components/app-shell";

/**
 * N5 (sidebar restructure and toolbar consolidation; ui-delivery-plan.md
 * ledger B1): the ratified sidebar order (Dashboard,
 * Activity, Repos with Groups nested one level beneath it, Settings
 * bottom-docked) and the cross-component contract between GroupsNav's
 * delete flow and the shell's group-filter state (E-16 (groups and tags)
 * known defect 6: deleting the ACTIVE group's filter must clear it without
 * forcing navigation).
 *
 * `AppShell` mounts `DashboardScreen` by default (the initial view), which
 * pulls in `repoList` and `summaryToday`; every command below is mocked
 * purely so the tree renders without throwing, not because this file is
 * about Dashboard's own behaviour (that lives in dashboard's own tests, if
 * any land later).
 */

vi.mock("@tauri-apps/api/app", () => ({ getVersion: vi.fn(async () => "9.9.9") }));

const EMPTY_SUMMARY: DailySummary = {
  date: "2026-09-01",
  updatedCount: 0,
  releasesCount: 0,
  attentionCount: 0,
  noChangeCount: 0,
  updated: [],
  newReleases: [],
  attention: [],
};

const GROUPS: GroupSummary[] = [{ id: 1, name: "Work", color: "#4477ff", repoCount: 1 }];

// One tracked repo, in the Work group - needed so the Repos toolbar's group
// filter control has something to render at all (it is gated by
// `list.length > 0`, same as the rest of the toolbar; see repos.tsx).
const REPO: RepoSummary = {
  id: 1,
  localName: "repo-a",
  localPath: "E:\\Projects\\repo-a",
  remoteOriginUrl: null,
  hostType: "github",
  aheadCount: 0,
  behindCount: 0,
  isDirty: false,
  isDetached: false,
  enabled: true,
  autoPaused: false,
  lastCheckedAt: 1_700_000_000,
  lastErrorCode: null,
  latestReleaseTag: null,
  openPrCount: null,
  lastLocalCommitAt: null,
  activeBranch: "main",
  upstreamState: "tracking",
  stars: null,
  forks: null,
  license: null,
  size: null,
  visibility: null,
  homepage: null,
};

function mockShellCommands(groups: GroupSummary[] = GROUPS, repos: RepoSummary[] = [REPO]) {
  mockCommand(commands, "dbRecoveryNotice", async () => ok({ recovered: false, backupPath: null }));
  mockCommand(commands, "groupList", async () => ok(groups));
  mockCommand(commands, "repoList", async () => ok(repos));
  mockCommand(commands, "summaryToday", async () => ok(EMPTY_SUMMARY));
  mockCommand(commands, "repoGroupMemberships", async () => ok([{ repoId: 1, groupIds: [1] }]));
}

beforeEach(() => {
  for (const ev of [
    events.navigateRequested,
    events.errorRaised,
    events.repoCheckCompleted,
    events.repoUpdateCompleted,
    events.schedulerTick,
    events.repoMetadataRefreshed,
  ]) {
    vi.spyOn(ev, "listen").mockResolvedValue(() => {});
  }
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("AppShell sidebar (N5)", () => {
  it("renders the ratified nav order top to bottom: Dashboard, Activity, Repos, then Settings bottom-docked separately", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    // Two <nav> elements: the primary list, and the bottom-docked Settings
    // nav - kept structurally separate rather than one flat list, which is
    // itself part of what "bottom-docked" means (see the Playwright pass for
    // the visual pinning this jsdom test cannot see).
    const navs = screen.getAllByRole("navigation");
    expect(navs).toHaveLength(2);

    const primaryLabels = within(navs[0])
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(primaryLabels).toEqual(["Dashboard", "Activity", "Repos"]);

    const bottomLabels = within(navs[1])
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(bottomLabels).toEqual(["Settings"]);
  });

  it("the logo block (R square, RepoSync wordmark, live version) survives untouched", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(screen.getByText("R")).toBeDefined();
    expect(screen.getByText("Sync")).toBeDefined();
    expect(await screen.findByText("9.9.9")).toBeDefined();
  });

  it("clicking each primary nav item switches the visible screen", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();
    const navs = screen.getAllByRole("navigation");

    await user.click(within(navs[0]).getByRole("button", { name: "Activity" }));
    expect(await screen.findByRole("heading", { name: "Activity" })).toBeDefined();

    await user.click(within(navs[0]).getByRole("button", { name: "Repos" }));
    expect(await screen.findByRole("heading", { name: "Repos" })).toBeDefined();

    await user.click(within(navs[1]).getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeDefined();
  });

  it("selecting a group from the nested Groups section navigates to Repos", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Work" }));

    expect(await screen.findByRole("heading", { name: "Repos" })).toBeDefined();
  });

  // Codex adversarial review finding 2: GroupsNav rendered its selected
  // row's accent fill on every screen, not only Repos, so Dashboard's own
  // active nav item and an accent-active group row both read as "current" at
  // once. Destination (aria-current) and filter (aria-pressed) are now two
  // separate, non-competing signals.
  it("exactly one destination reads as current at any time, even with a group filter still selected on a different screen", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Work" }));
    await screen.findByRole("heading", { name: "Repos" });

    // Navigate away while the group filter stays selected.
    await user.click(within(screen.getAllByRole("navigation")[0]).getByRole("button", { name: "Dashboard" }));
    await screen.findByRole("heading", { name: "Dashboard" });

    const current = document.querySelectorAll('[aria-current="page"]');
    expect(current).toHaveLength(1);
    expect(current[0].textContent).toBe("Dashboard");

    // The filter's own state is identified separately (aria-pressed), not
    // via aria-current, and it persists across the navigation above.
    expect(screen.getByRole("button", { name: "Work" }).getAttribute("aria-pressed")).toBe("true");
  });

  it("deleting the ACTIVE group's filter clears it WITHOUT forcing navigation away from the current screen (E-16 known defect 6)", async () => {
    mockShellCommands();
    mockCommand(commands, "groupDelete", async () => ok(null));
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    // Select the group (this DOES navigate, by design - selecting is not
    // clearing), then move to a different screen entirely so the delete
    // below has something other than Repos to (not) force us back to.
    await user.click(await screen.findByRole("button", { name: "Work" }));
    await screen.findByRole("heading", { name: "Repos" });
    await user.click(screen.getAllByRole("navigation")[0].querySelector("button")!); // Dashboard is first
    await screen.findByRole("heading", { name: "Dashboard" });

    await user.click(screen.getByRole("button", { name: "Delete Work" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));

    await waitFor(() => expect(commands.groupDelete).toHaveBeenCalledWith(1));
    // Still on Dashboard: the delete did not force-navigate to Repos.
    expect(screen.getByRole("heading", { name: "Dashboard" })).toBeDefined();

    // And the filter actually cleared: opening Repos now shows no active
    // group filter control (the group is gone from the refetched list too,
    // but even before that refetch resolves, activeGroupId itself is null).
    await user.click(within(screen.getAllByRole("navigation")[0]).getByRole("button", { name: "Repos" }));
    await screen.findByRole("heading", { name: "Repos" });
    expect(screen.queryByRole("button", { name: /Clear .* filter/ })).toBeNull();
  });

  it("a failed delete of the active group's filter does not clear it (no false navigation-free clear on error)", async () => {
    mockShellCommands();
    // db.locked (AppError::DbLocked) is a real, plausible failure - see the
    // same note in groups-nav.test.tsx (group_delete is otherwise idempotent).
    mockCommand(commands, "groupDelete", async () => err("db.locked", "the database is locked"));
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Work" }));
    await screen.findByRole("heading", { name: "Repos" });

    await user.click(screen.getByRole("button", { name: "Delete Work" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));

    await waitFor(() => expect(commands.groupDelete).toHaveBeenCalledWith(1));
    // The delete failed, so the group (and its active filter) is still
    // there - Repos should still show the group control for it.
    expect(await screen.findByRole("button", { name: "Clear Work filter" })).toBeDefined();
  });
});

/**
 * Q1 -> 1A (ui-delivery-plan.md decision queue, N7 consistency sweep): the
 * database-recovery banner moved off a full-fill `bg-status-dirty/12` region
 * onto a thin left-edge stripe on a neutral surface. This banner had ZERO
 * jsdom coverage before N7 (`mockShellCommands` always returned
 * `recovered: false`), so these tests pin the wording and dismissal
 * behaviour the restyle must not touch; the visual stripe treatment itself
 * (computed background/border-left colours, both themes) is real-browser
 * Playwright evidence, not jsdom's job (AGENTS.md: assert on meaning, not
 * markup or styling).
 */
describe("AppShell database recovery banner (N7)", () => {
  it("shows the recovery notice with the backup path, and dismisses on click", async () => {
    mockShellCommands();
    mockCommand(commands, "dbRecoveryNotice", async () =>
      ok({ recovered: true, backupPath: "C:\\Users\\test\\AppData\\Local\\RepoSync\\reposync.db.bak" }),
    );
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    expect(await screen.findByText("Database was reset after a failed migration")).toBeDefined();
    expect(screen.getByText(/could not migrate your existing database/i)).toBeDefined();
    expect(screen.getByText("C:\\Users\\test\\AppData\\Local\\RepoSync\\reposync.db.bak")).toBeDefined();

    await user.click(screen.getByRole("button", { name: "Dismiss database recovery notice" }));

    expect(screen.queryByText("Database was reset after a failed migration")).toBeNull();
  });

  it("falls back to generic wording when no backup path was recorded", async () => {
    mockShellCommands();
    mockCommand(commands, "dbRecoveryNotice", async () => ok({ recovered: true, backupPath: null }));
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(await screen.findByText(/preserved alongside it/i)).toBeDefined();
  });

  it("shows no banner at all when the database was not recovered", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    expect(screen.queryByText("Database was reset after a failed migration")).toBeNull();
  });
});
