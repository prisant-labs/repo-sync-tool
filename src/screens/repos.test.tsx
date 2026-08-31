// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type {
  CheckAllSummary,
  CheckResult,
  GroupSummary,
  RepoDetail,
  RepoGroupMembership,
  RepoSummary,
  Settings,
} from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { ToastContext } from "@/hooks/use-toast";
import { ReposScreen } from "@/screens/repos";

/**
 * N2 (ui-delivery-plan.md ledger B5), plus the fix round after the Codex
 * adversarial review of PR #73: the Repos screen rebuilt on the shared
 * `DataTable`. These tests are about MEANING, not the grid: the ratified
 * column set actually renders (Branch and Folder now included, since PR #74
 * added `activeBranch`/`localPath` to `RepoSummary`), Check now and Check all
 * still reach the right IPC calls and report honestly from the structured
 * `CheckAllSummary`, the existing filter/banner/empty-state behaviour
 * survives, and the LagSignal bar is gone from the row (jp's 2026-08-28
 * decision, D1).
 */

function repo(overrides: Partial<RepoSummary> = {}): RepoSummary {
  return {
    id: 1,
    localName: "repo-a",
    localPath: "E:\\Projects\\repo-a",
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
    ...overrides,
  };
}

/** A fully-clean `CheckAllSummary`: every targeted repo completed and succeeded. */
function cleanSummary(targeted: number): CheckAllSummary {
  return { targeted, completed: targeted, succeeded: targeted, noResult: 0, failedCheck: 0 };
}

/** Minimal `RepoDetail`/`Settings` fixtures for the drawer, mirroring `repo-detail.test.tsx`. */
const MINIMAL_DETAIL: RepoDetail = {
  id: 9,
  localName: "repo-nine",
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
  localPath: "E:\\Projects\\repo-nine",
  remoteOriginUrl: "https://github.com/example/repo-nine.git",
  defaultBranch: "main",
  updateMode: "fetch_only",
  checkFrequencyMin: 0,
  createdAt: 1_690_000_000,
  notes: null,
  activeBranch: "main",
  headSha: "abcdef1234567890",
  upstreamBranch: "origin/main",
  upstreamState: null,
  lastLocalCommitAt: null,
  lastUpdatedAt: null,
  lastAttemptedAt: null,
  nextCheckAt: null,
  consecutiveFailures: 0,
  description: null,
  topicsJson: null,
  latestReleaseAt: null,
  latestReleaseUrl: null,
  isArchived: false,
  lastRemoteSha: null,
  lastFetchedAt: null,
  openPrCount: null,
  defaultBranchPrCount: null,
  prLastCheckedAt: null,
  stars: null,
  forks: null,
  license: null,
  size: null,
  visibility: null,
  homepage: null,
};

const MINIMAL_SETTINGS: Settings = {
  globalCheckMinutes: 360,
  quietHoursStart: null,
  quietHoursEnd: null,
  notifyOnRelease: true,
  notifyOnFailure: true,
  gitExecutablePath: null,
  editorCommand: "code",
  terminalCommand: "wt",
  autostart: false,
  activityRetentionD: 90,
  githubTokenPresent: false,
  autoUpdateCheck: true,
  closeMinimizesToTray: true,
};

const GROUPS: GroupSummary[] = [{ id: 1, name: "Work", color: "#4477ff", repoCount: 1 }];

function renderScreen(repos: RepoSummary[], memberships: RepoGroupMembership[] = []) {
  mockCommand(commands, "repoList", async () => ok(repos));
  mockCommand(commands, "repoGroupMemberships", async () => ok(memberships));
  const toast = vi.fn();
  const onClearGroup = vi.fn();
  const onGroupsChanged = vi.fn();
  const view = render(
    <ToastContext.Provider value={toast}>
      <ReposScreen
        activeGroupId={null}
        groups={GROUPS}
        onClearGroup={onClearGroup}
        onGroupsChanged={onGroupsChanged}
      />
    </ToastContext.Provider>,
  );
  return { toast, onClearGroup, onGroupsChanged, ...view };
}

beforeEach(() => {
  // `useBackendEvents` (repos.tsx) subscribes to all four AGGREGATE events on
  // mount; a repo-detail test stubs a different set (three, repo-scoped).
  // Unstubbed `listen` rejects with no Tauri runtime present.
  for (const ev of [
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

describe("ReposScreen table", () => {
  it("renders the full ratified column set, Branch and Folder included", async () => {
    renderScreen([repo()]);

    await screen.findByRole("columnheader", { name: "Repository" });
    for (const name of ["Repository", "Status", "Branch", "Ahead", "Behind", "Groups", "Folder", "Checked"]) {
      expect(screen.getByRole("columnheader", { name })).toBeDefined();
    }
  });

  it("renders the branch name, and 'detached' only for a real detached HEAD (not the never-inspected/unborn-HEAD null cases)", async () => {
    renderScreen([
      repo({ id: 1, localName: "repo-a", activeBranch: "feature/x" }),
      repo({ id: 2, localName: "repo-b", activeBranch: null, isDetached: true }),
      repo({ id: 3, localName: "repo-c", activeBranch: null, isDetached: false }),
    ]);
    await screen.findByText("repo-a");

    // Column order (repos.tsx): repo, status, branch, ahead, behind, groups,
    // folder, checked, actions - branch is cell index 2.
    const branchCellFor = (name: string) =>
      within(screen.getByText(name).closest('[role="row"]') as HTMLElement).getAllByRole("cell")[2];

    expect(branchCellFor("repo-a").textContent).toBe("feature/x");
    expect(branchCellFor("repo-b").textContent).toBe("detached");
    // repo-c: never inspected or an unborn HEAD - neither gets the word
    // "detached"; it renders the empty dash like any other empty cell.
    expect(branchCellFor("repo-c").textContent).toBe("-");
  });

  it("renders the Folder column from localPath, presentationally (no click wiring)", async () => {
    renderScreen([repo({ localPath: "E:\\Projects\\repo-a" })]);
    await screen.findByText("repo-a");

    const folderCell = screen.getByTitle("Open in File Explorer");
    expect(folderCell.textContent).toBe("E:\\Projects\\repo-a");
  });

  it("renders Ahead/Behind values with an icon, and a dash with no icon when zero", async () => {
    renderScreen([repo({ id: 1, localName: "repo-a", behindCount: 3 }), repo({ id: 2, localName: "repo-b" })]);
    await screen.findByText("repo-a");

    // repo-a is 3 behind: the value renders in the Behind column.
    expect(screen.getByText("3")).toBeDefined();
    // repo-b has ahead=0/behind=0 on both columns: two dashes for that row
    // (plus repo-a's Ahead column also dashes at 0).
    expect(screen.getAllByText("-").length).toBeGreaterThanOrEqual(2);
  });

  it("shows the group's outlined pill in the Groups column, not inline under the repo name", async () => {
    renderScreen([repo({ id: 1 })], [{ repoId: 1, groupIds: [1] }]);
    await screen.findByText("repo-a");

    expect(screen.getByText("Work")).toBeDefined();
  });

  it("LagSignal is gone from the row: its label never renders even for a dirty repo", async () => {
    renderScreen([repo({ id: 1, isDirty: true })]);
    await screen.findByText("repo-a");
    const table = screen.getByRole("table");

    // "uncommitted, skipped" is LagSignal's own dirty-state label
    // (lib/status.ts `lagLabel`); StatusBadge renders "Dirty" instead, a
    // different string, so this only fires if the bar itself is present.
    expect(within(table).queryByText(/uncommitted, skipped/)).toBeNull();
    expect(within(table).getByText("Dirty")).toBeDefined();
  });

  it("Check now still fires repoCheckNow for the clicked repo", async () => {
    const CHECK_RESULT: CheckResult = {
      repoId: 7,
      decision: "up-to-date",
      reason: null,
      ahead: 0,
      behind: 0,
      isDirty: false,
      isDetached: false,
      checkedAt: 1_700_000_100,
      failed: false,
    };
    const checkNow = mockCommand(commands, "repoCheckNow", async () => ok(CHECK_RESULT));
    renderScreen([repo({ id: 7, localName: "repo-seven" })]);
    await screen.findByText("repo-seven");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Check now" }));

    await waitFor(() => expect(checkNow).toHaveBeenCalledWith(7));
  });

  it("clicking a status filter chip narrows the visible rows", async () => {
    renderScreen([repo({ id: 1, localName: "repo-behind", behindCount: 2 }), repo({ id: 2, localName: "repo-sync" })]);
    await screen.findByText("repo-behind");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Behind/ }));

    expect(screen.getByText("repo-behind")).toBeDefined();
    expect(screen.queryByText("repo-sync")).toBeNull();
  });

  it("Check all: a fully clean summary gets the 'ok' toast, reported as completed (not in-progress)", async () => {
    const checkAll = mockCommand(commands, "repoCheckAll", async () => ok(cleanSummary(2)));
    const { toast } = renderScreen([repo({ id: 1, localName: "repo-a" }), repo({ id: 2, localName: "repo-b" })]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    await waitFor(() => expect(checkAll).toHaveBeenCalledTimes(1));
    expect(toast).toHaveBeenCalledWith("ok", "All 2 repos checked");
  });

  it("Check all: targeted === 0 (and only that) reads as no enabled repos, not inferred from a bare zero elsewhere", async () => {
    mockCommand(commands, "repoCheckAll", async () =>
      ok({ targeted: 0, completed: 0, succeeded: 0, noResult: 0, failedCheck: 0 }),
    );
    const { toast } = renderScreen([repo()]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    await waitFor(() => expect(toast).toHaveBeenCalledWith("info", "No enabled repos to check"));
  });

  it("Check all: every targeted repo failed before producing a result gets an error toast, not the zero-means-nothing-enabled reading", async () => {
    mockCommand(commands, "repoCheckAll", async () =>
      ok({ targeted: 3, completed: 0, succeeded: 0, noResult: 3, failedCheck: 0 }),
    );
    const { toast } = renderScreen([repo()]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith("error", "Nothing produced a result", "See Activity for details."),
    );
    expect(toast).not.toHaveBeenCalledWith("ok", expect.anything(), expect.anything());
    expect(toast).not.toHaveBeenCalledWith("info", "No enabled repos to check");
  });

  it("Check all: a mixed result (some succeeded, some failed) reports the failure count and points at Activity, never the ok tone", async () => {
    mockCommand(commands, "repoCheckAll", async () =>
      ok({ targeted: 5, completed: 3, succeeded: 2, noResult: 2, failedCheck: 1 }),
    );
    const { toast } = renderScreen([repo()]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    // problems = failedCheck (1) + noResult (2) = 3
    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith("error", "3 of 5 repos failed", "See Activity for details."),
    );
    expect(toast).not.toHaveBeenCalledWith("ok", expect.anything(), expect.anything());
  });

  it("Check all surfaces a genuine IPC failure as an error toast with a real error code's message", async () => {
    mockCommand(commands, "repoCheckAll", async () => err("net.offline", "no network connection"));
    const { toast } = renderScreen([repo()]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith("error", "Could not check all", "no network connection"),
    );
  });

  it("the empty state still renders when there are no tracked repos", async () => {
    renderScreen([]);
    expect(await screen.findByText("No repositories yet")).toBeDefined();
  });

  it("the table has a valid accessible tree: table > rowgroup > row, and a row carries no button role or tabIndex", async () => {
    renderScreen([repo()]);
    await screen.findByText("repo-a");

    const table = screen.getByRole("table", { name: "Tracked repositories" });
    expect(within(table).getAllByRole("rowgroup")).toHaveLength(2);
    const dataRow = screen.getByText("repo-a").closest('[role="row"]') as HTMLElement;
    expect(dataRow.getAttribute("role")).toBe("row");
    expect(dataRow.hasAttribute("tabindex")).toBe(false);
  });

  it("Enter on Check now only checks; the chevron is the keyboard path into the drawer", async () => {
    const CHECK_RESULT: CheckResult = {
      repoId: 9,
      decision: "up-to-date",
      reason: null,
      ahead: 0,
      behind: 0,
      isDirty: false,
      isDetached: false,
      checkedAt: 1_700_000_100,
      failed: false,
    };
    const checkNow = mockCommand(commands, "repoCheckNow", async () => ok(CHECK_RESULT));
    const repoGet = mockCommand(commands, "repoGet", async () => ok(MINIMAL_DETAIL));
    mockCommand(commands, "groupList", async () => ok([]));
    mockCommand(commands, "groupsForRepo", async () => ok([]));
    mockCommand(commands, "settingsGet", async () => ok(MINIMAL_SETTINGS));
    for (const ev of [events.repoStateChanged]) {
      vi.spyOn(ev, "listen").mockResolvedValue(() => {});
    }
    renderScreen([repo({ id: 9, localName: "repo-nine" })]);
    await screen.findByText("repo-nine");
    const user = userEvent.setup();

    screen.getByRole("button", { name: "Check now" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(checkNow).toHaveBeenCalledWith(9));
    expect(repoGet).not.toHaveBeenCalled();

    screen.getByRole("button", { name: "Open details" }).focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(repoGet).toHaveBeenCalledWith(9));
  });
});
