// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type {
  DailySummary,
  GroupSummary,
  RepoDetail,
  RepoGroupMembership,
  RepoSummary,
  Settings,
  SummaryItem,
} from "@/lib/bindings";
import { mockCommand, ok } from "@/test/mock-ipc";
import { DashboardScreen } from "@/screens/dashboard";

/**
 * N6 (ui-delivery-plan.md ledger B4/B13): the four stat tiles become the
 * ratified M5 filter tiles (real labels and hint lines restored) and the
 * Needs-attention rows carry a structured WHY with local/remote icons. These
 * tests are about MEANING: the real labels and hints survive, the
 * Need-attention numeral's failed-ink behaviour is pinned, tile wiring is
 * honest (only "Under watch" navigates; the other three are inert, never a
 * fake affordance), group scoping never fabricates a zero while membership
 * is loading, and the attention row's local/remote reason structure renders.
 */

function repo(overrides: Partial<RepoSummary> = {}): RepoSummary {
  return {
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
    ...overrides,
  };
}

function summaryItem(overrides: Partial<SummaryItem> = {}): SummaryItem {
  return { repoId: 1, localName: "repo-a", detail: null, ...overrides };
}

function summary(overrides: Partial<DailySummary> = {}): DailySummary {
  return {
    date: "2026-09-01",
    updatedCount: 0,
    releasesCount: 0,
    attentionCount: 0,
    noChangeCount: 0,
    updated: [],
    newReleases: [],
    attention: [],
    ...overrides,
  };
}

const MINIMAL_DETAIL: RepoDetail = {
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
  localPath: "E:\\Projects\\repo-a",
  remoteOriginUrl: "https://github.com/example/repo-a.git",
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

function renderScreen(
  repos: RepoSummary[],
  dailySummary: DailySummary,
  memberships: RepoGroupMembership[] = [],
  options: { activeGroupId?: number | null; membershipsPending?: boolean; onOpenRepos?: () => void } = {},
) {
  mockCommand(commands, "repoList", async () => ok(repos));
  mockCommand(commands, "summaryToday", async () => ok(dailySummary));
  if (options.membershipsPending) {
    mockCommand(commands, "repoGroupMemberships", () => new Promise(() => {}));
  } else {
    mockCommand(commands, "repoGroupMemberships", async () => ok(memberships));
  }
  const onOpenRepos = options.onOpenRepos ?? vi.fn();
  const view = render(
    <DashboardScreen
      onOpenRepos={onOpenRepos}
      activeGroupId={options.activeGroupId ?? null}
      groups={GROUPS}
    />,
  );
  return { onOpenRepos, ...view };
}

beforeEach(() => {
  // `useBackendEvents` subscribes to all four aggregate events on mount.
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

describe("Dashboard tiles", () => {
  it("renders the four real labels and their hint lines, not the prototype's invented ones", async () => {
    renderScreen(
      [repo()],
      summary({ updatedCount: 2, releasesCount: 1, noChangeCount: 3 }),
    );
    await screen.findByText("Under watch");

    expect(screen.getByText("Need attention")).toBeDefined();
    expect(screen.getByText("Updated today")).toBeDefined();
    expect(screen.getByText("New releases")).toBeDefined();

    expect(screen.getByText("3 in sync")).toBeDefined();
    expect(screen.getByText("dirty or failed")).toBeDefined();
    expect(screen.getByText("fast-forwarded, clean")).toBeDefined();
    expect(screen.getByText("upstream tags")).toBeDefined();
  });

  it("turns the Need-attention numeral status-failed ink only when it is non-zero", async () => {
    const { rerender } = renderScreen([repo()], summary({ attentionCount: 0 }));
    await screen.findByText("Under watch");
    const zeroValue = screen.getByText("Need attention").closest("div")?.parentElement?.querySelector(".text-3xl");
    expect(zeroValue?.className).not.toContain("text-status-failed");
    cleanup();

    renderScreen(
      [repo({ id: 1, localName: "repo-a", isDirty: true })],
      summary({ attentionCount: 1, attention: [summaryItem({ repoId: 1, localName: "repo-a" })] }),
    );
    await screen.findByText("Under watch");
    const nonZeroValue = screen.getByText("Need attention").closest("div")?.parentElement?.querySelector(".text-3xl");
    expect(nonZeroValue?.className).toContain("text-status-failed");
    void rerender;
  });

  it("wires only Under watch as a real button that navigates to Repos; the other three are inert", async () => {
    const onOpenRepos = vi.fn();
    renderScreen([repo()], summary({ updatedCount: 1, releasesCount: 1 }), [], { onOpenRepos });
    await screen.findByText("Under watch");

    const underWatch = screen.getByRole("button", { name: /Under watch/ });
    await userEvent.setup().click(underWatch);
    expect(onOpenRepos).toHaveBeenCalledTimes(1);

    // The other three tiles carry no button role and no click affordance: a
    // click on the tile lands nowhere, because no Repos filter can express
    // "dirty or failed" as one selection, and Repos has no "updated today" or
    // "has a new release" filter at all (see the PR body).
    for (const label of ["Need attention", "Updated today", "New releases"]) {
      expect(screen.queryByRole("button", { name: new RegExp(label) })).toBeNull();
    }
  });
});

describe("Dashboard group scoping", () => {
  it("scopes Under watch, Need attention, Updated today and New releases to the active group, and marks the unscopable in-sync hint explicitly", async () => {
    const repos = [
      repo({ id: 1, localName: "in-group", isDirty: true }),
      repo({ id: 2, localName: "out-of-group", lastErrorCode: "git.auth_failed" }),
    ];
    const dailySummary = summary({
      attentionCount: 2,
      noChangeCount: 40,
      attention: [
        summaryItem({ repoId: 1, localName: "in-group", detail: "uncommitted changes" }),
        summaryItem({ repoId: 2, localName: "out-of-group", detail: "git.auth_failed" }),
      ],
    });
    renderScreen(repos, dailySummary, [{ repoId: 1, groupIds: [1] }], { activeGroupId: 1 });

    await screen.findByText("Scoped to Work");
    // Under watch: only repo 1 is a member of group 1.
    const underWatchTile = screen.getByRole("button", { name: /Under watch/ });
    expect(within(underWatchTile).getByText("1")).toBeDefined();
    // The in-sync hint cannot be scoped (no per-repo id list backs
    // noChangeCount) and says so rather than silently sitting under a
    // scoped headline.
    expect(within(underWatchTile).getByText("40 in sync (all repos)")).toBeDefined();

    // Need attention scopes its count AND its list together (same filter,
    // one source): only "in-group" qualifies, "out-of-group" is excluded.
    expect(screen.getByText("in-group")).toBeDefined();
    expect(screen.queryByText("out-of-group")).toBeNull();
  });

  it("never renders a fabricated zero while the group membership read is pending", async () => {
    renderScreen(
      [repo({ id: 1, isDirty: true })],
      summary({ attentionCount: 1, attention: [summaryItem({ repoId: 1 })] }),
      [],
      { activeGroupId: 1, membershipsPending: true },
    );

    await screen.findByText("Loading...");
    expect(screen.queryByText("Need attention")).toBeNull();
    expect(screen.queryByText("0")).toBeNull();
  });

  it("all-clear copy names the active group rather than claiming every watched repo when scoped", async () => {
    renderScreen(
      [repo({ id: 1, localName: "in-group" })],
      summary({ attentionCount: 0, attention: [] }),
      [{ repoId: 1, groupIds: [1] }],
      { activeGroupId: 1 },
    );
    await screen.findByText("All clear");
    expect(screen.getByText("Every repo in Work is in sync or intentionally paused.")).toBeDefined();
  });
});

describe("Needs attention rows", () => {
  it("shows the all-clear state when nothing needs attention", async () => {
    renderScreen([repo()], summary({ attentionCount: 0, attention: [] }));
    await screen.findByText("All clear");
    expect(
      screen.getByText("Every watched repo is in sync or intentionally paused."),
    ).toBeDefined();
  });

  it("structures a row's reasons under local and remote icons, not a single joined string", async () => {
    renderScreen(
      [
        repo({ id: 1, localName: "dirty-repo", isDirty: true }),
        repo({
          id: 2,
          localName: "failed-repo",
          lastErrorCode: "git.auth_failed",
          behindCount: 4,
          openPrCount: 2,
        }),
      ],
      summary({
        attentionCount: 2,
        attention: [
          summaryItem({ repoId: 1, localName: "dirty-repo", detail: "uncommitted changes" }),
          summaryItem({ repoId: 2, localName: "failed-repo", detail: "git.auth_failed" }),
        ],
      }),
    );
    await screen.findByText("dirty-repo");

    // The local cause renders as a word, not the raw backend code.
    expect(screen.getByText("Uncommitted changes")).toBeDefined();

    // The remote row carries three distinct reason pills: the short failure
    // label (not the raw "git.auth_failed" code), the behind count, and the
    // open-PR count.
    const failedRow = screen.getByText("failed-repo").closest("li") as HTMLElement;
    expect(within(failedRow).getByText("Auth failed")).toBeDefined();
    expect(within(failedRow).getByText("4 behind")).toBeDefined();
    expect(within(failedRow).getByText("2 open PRs")).toBeDefined();
    // The full sentence is available on hover, not dropped.
    expect(within(failedRow).getByText("Auth failed").title).toContain("Authentication failed");
  });

  it("clicking a row opens the repo detail drawer for that repo", async () => {
    const repoGet = mockCommand(commands, "repoGet", async () => ok(MINIMAL_DETAIL));
    mockCommand(commands, "groupList", async () => ok([]));
    mockCommand(commands, "groupsForRepo", async () => ok([]));
    mockCommand(commands, "settingsGet", async () => ok(MINIMAL_SETTINGS));
    for (const ev of [events.repoStateChanged]) {
      vi.spyOn(ev, "listen").mockResolvedValue(() => {});
    }

    renderScreen(
      [repo({ id: 1, localName: "repo-a", isDirty: true })],
      summary({ attentionCount: 1, attention: [summaryItem({ repoId: 1, localName: "repo-a" })] }),
    );
    await screen.findByText("repo-a");

    await userEvent.setup().click(screen.getByText("repo-a"));
    await waitFor(() => expect(repoGet).toHaveBeenCalledWith(1));
  });
});
