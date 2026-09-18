// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type { DailySummary, GroupSummary, RepoSummary } from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { AppShell } from "@/components/app-shell";

/**
 * The sidebar's settled shape (ui-delivery-plan.md section H.1 / J.1, the
 * decisions the round-three and round-four walks marked Keep): the nav reads
 * Dashboard, Repos, Activity (SB6), Groups is a plain line under the WHOLE
 * nav rather than a subtree of Repos (A1), Settings is bottom-docked (SB5),
 * and the engaged group stays marked on every screen (A2).
 *
 * SB6 REVERSES the earlier N5 / ledger-B1 order this file used to assert
 * (Dashboard, Activity, Repos), and A1 reverses N5's nesting. Both reversals
 * are recorded in the register, which is the only file that may record a
 * decision; the code and its tests follow it.
 *
 * Also the cross-component contract between GroupsNav's delete flow and the
 * shell's group-filter state (E-16 (groups and tags) known defect 6: deleting
 * the ACTIVE group's filter must clear it without forcing navigation).
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
  headState: "branch",
  updateMode: "pull_ff_only",
  stars: null,
  forks: null,
  license: null,
  size: null,
  visibility: null,
  homepage: null,
};

function mockShellCommands(
  groups: GroupSummary[] = GROUPS,
  repos: RepoSummary[] = [REPO],
  summary: DailySummary = EMPTY_SUMMARY,
) {
  mockCommand(commands, "dbRecoveryNotice", async () => ok({ recovered: false, backupPath: null }));
  mockCommand(commands, "groupList", async () => ok(groups));
  mockCommand(commands, "repoList", async () => ok(repos));
  // D6: the backend scopes the summary, so the mock does. A mock that ignored
  // `groupId` would let a sidebar that dropped its scoping still pass SB3.
  mockCommand(commands, "summaryToday", async (groupId) =>
    ok(groupId === null ? summary : { ...summary, attentionCount: 0, attention: [] }),
  );
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

describe("AppShell sidebar", () => {
  it("renders the settled nav order top to bottom: Dashboard, Repos, Activity, then Settings bottom-docked separately (SB6)", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    // Two <nav> elements: the primary list, and the bottom-docked Settings
    // nav - kept structurally separate rather than one flat list, which is
    // itself part of what "bottom-docked" means (see the Playwright pass for
    // the visual pinning this jsdom test cannot see).
    const navs = screen.getAllByRole("navigation");
    expect(navs).toHaveLength(2);

    // `textContent` picks up the SB4 count badge and its screen-reader
    // wording, so the order assertion reads the visible label only - the
    // first child span, which is the one carrying the word.
    const primaryLabels = within(navs[0])
      .getAllByRole("button")
      .map((b) => b.querySelector("span")?.textContent);
    expect(primaryLabels).toEqual(["Dashboard", "Repos", "Activity"]);

    const bottomLabels = within(navs[1])
      .getAllByRole("button")
      .map((b) => b.textContent);
    expect(bottomLabels).toEqual(["Settings"]);
  });

  // A1: Groups is a plain line under the WHOLE nav - no indent, no guide
  // rail, no tree. The guide rail is the thing being removed, and a class
  // assertion is the only handle jsdom gives on it (it computes no layout),
  // so this pins the absence of the exact wrapper that used to draw it.
  it("Groups sits under the whole nav with no indent and no guide rail (A1)", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    const groupsHeading = screen.getByText("Groups");
    const aside = groupsHeading.closest("aside")!;
    // Nothing between the Groups block and the sidebar may indent it or draw
    // a vertical rule down its left edge.
    for (let el: HTMLElement | null = groupsHeading; el && el !== aside; el = el.parentElement) {
      const classes = el.className.split(/\s+/);
      expect(classes.some((c) => c.startsWith("ml-"))).toBe(false);
      expect(classes).not.toContain("border-l");
    }
  });

  // A2, wired end to end: the sidebar marks the engaged group on Activity,
  // so Activity has to actually honour it. The backend resolves group
  // membership server-side, before its own LIMIT, so the scope belongs on
  // the wire - this asserts the shell hands it down rather than leaving the
  // mark to stand over an unfiltered list.
  it("the engaged group reaches the Activity screen's query, not just the sidebar (A2)", async () => {
    mockShellCommands();
    const seen: (number | null)[] = [];
    mockCommand(commands, "activityList", async (filter) => {
      seen.push(filter.groupId);
      return ok([]);
    });
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Work" }));
    await screen.findByRole("heading", { name: "Repos" });

    await user.click(within(screen.getAllByRole("navigation")[0]).getByRole("button", { name: "Activity" }));
    await screen.findByRole("heading", { name: "Activity" });

    await waitFor(() => expect(seen.at(-1)).toBe(1));
  });

  // SB4. The count is real information a sighted user gets from the badge,
  // so it belongs in the accessible name too - but as words. A bare "Repos 2"
  // could be a count, a version, or a keyboard hint.
  it("the Repos nav item carries the repo count, as a badge and as words (SB4)", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });

    const repos = within(screen.getAllByRole("navigation")[0]).getByRole("button", {
      name: "Repos, 1 repository",
    });
    expect(repos.textContent).toContain("1");
  });

  // SB3 + SB4, the honesty rule both share: the sidebar reports on a library
  // it may only be seeing part of. Its numbers are scoped the same way the
  // screens they point at are scoped, or the rail contradicts its own
  // destination.
  it("scopes the repo count to the engaged group (SB4)", async () => {
    // Two repos, only one of them in Work.
    mockShellCommands(GROUPS, [REPO, { ...REPO, id: 2, localName: "repo-b" }]);
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const nav = () => within(screen.getAllByRole("navigation")[0]);
    await waitFor(() => nav().getByRole("button", { name: "Repos, 2 repositories" }));

    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Work" }));

    await waitFor(() => nav().getByRole("button", { name: "Repos, 1 repository" }));
  });

  it("shows the attention dot only when something in scope needs attention (SB3)", async () => {
    mockShellCommands(GROUPS, [REPO], {
      ...EMPTY_SUMMARY,
      attentionCount: 1,
      // The repo needing attention is id 2, which is NOT in Work (the
      // membership mock puts only repo 1 there).
      attention: [{ repoId: 2, localName: "repo-b", detail: "Uncommitted changes" }],
    });
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const nav = () => within(screen.getAllByRole("navigation")[0]);

    // Unscoped: something needs attention, so the dot is there.
    await waitFor(() =>
      nav().getByRole("button", { name: /Dashboard.*need attention/ }),
    );

    // Scoped to Work, which does not contain that repo: the dot must go, or
    // it claims attention over a Dashboard that will say All clear.
    const user = userEvent.setup();
    await user.click(await screen.findByRole("button", { name: "Work" }));
    await waitFor(() =>
      expect(nav().queryByRole("button", { name: /need attention/ })).toBeNull(),
    );
  });

  // L4. The dialog stays owned by the Repos screen because `repo_add` emits
  // no backend event, so only that screen's own `onAdded` refetch makes a new
  // repo appear. The sidebar asks; it does not own.
  it("the sidebar's add-repo button opens the Repos screen's Add dialog (L4)", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Add repositories" }));

    await screen.findByRole("heading", { name: "Repos" });
    expect(await screen.findByRole("dialog")).toBeDefined();
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

    await user.click(within(navs[0]).getByRole("button", { name: /^Repos/ }));
    expect(await screen.findByRole("heading", { name: "Repos" })).toBeDefined();

    await user.click(within(navs[1]).getByRole("button", { name: "Settings" }));
    expect(await screen.findByRole("heading", { name: "Settings" })).toBeDefined();
  });

  it("selecting a group from the Groups section navigates to Repos", async () => {
    mockShellCommands();
    render(<AppShell />);
    await screen.findByRole("heading", { name: "Dashboard" });
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Work" }));

    expect(await screen.findByRole("heading", { name: "Repos" })).toBeDefined();
  });

  // A2 is settled: the sidebar keeps marking the engaged group on EVERY
  // screen. What must stay exclusive is the DESTINATION, and destination and
  // filter are two different signals carried on two different attributes -
  // `aria-current` for where you are, `aria-pressed` for what is engaged. So
  // the group row can paint its fill everywhere without ever competing for
  // "current", which is what this test pins. It replaces an earlier
  // assertion that gated the fill itself on being on Repos.
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
    await user.click(within(screen.getAllByRole("navigation")[0]).getByRole("button", { name: /^Repos/ }));
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
