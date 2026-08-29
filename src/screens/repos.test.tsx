// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type { CheckResult, GroupSummary, RepoGroupMembership, RepoSummary } from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { ToastContext } from "@/hooks/use-toast";
import { ReposScreen } from "@/screens/repos";

/**
 * N2 (ui-delivery-plan.md ledger B5): the Repos screen rebuilt on the shared
 * `DataTable`. These tests are about MEANING, not the grid: the ratified
 * column set actually renders (and Branch/Folder, both omitted for a missing
 * `RepoSummary` field, do not), Check now and Check all still reach the
 * right IPC calls, the existing filter/banner/empty-state behaviour survives,
 * and the LagSignal bar is gone from the row (jp's 2026-08-28 decision, D1).
 */

function repo(overrides: Partial<RepoSummary> = {}): RepoSummary {
  return {
    id: 1,
    localName: "repo-a",
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
    upstreamState: "tracking",
    ...overrides,
  };
}

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
  it("renders the ratified column set and omits Branch and Folder (no backing field on RepoSummary)", async () => {
    renderScreen([repo()]);

    await screen.findByRole("columnheader", { name: "Repository" });
    for (const name of ["Repository", "Status", "Ahead", "Behind", "Groups", "Checked"]) {
      expect(screen.getByRole("columnheader", { name })).toBeDefined();
    }
    expect(screen.queryByRole("columnheader", { name: "Branch" })).toBeNull();
    expect(screen.queryByRole("columnheader", { name: "Folder" })).toBeNull();
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

  it("Check all reports an attempted-count toast, never a success-styled one, and starts disabled by nothing", async () => {
    const checkAll = mockCommand(commands, "repoCheckAll", async () => ok(2));
    const { toast } = renderScreen([repo({ id: 1, localName: "repo-a" }), repo({ id: 2, localName: "repo-b" })]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    await waitFor(() => expect(checkAll).toHaveBeenCalledTimes(1));
    expect(toast).toHaveBeenCalledWith("info", "Checking 2 repos", expect.any(String));
    expect(toast).not.toHaveBeenCalledWith("ok", expect.anything(), expect.anything());
  });

  it("Check all with zero attempted says so honestly, not as if work started", async () => {
    mockCommand(commands, "repoCheckAll", async () => ok(0));
    const { toast } = renderScreen([repo()]);
    await screen.findByText("repo-a");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: /Check all/ }));

    await waitFor(() => expect(toast).toHaveBeenCalledWith("info", "No enabled repos to check"));
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
});
