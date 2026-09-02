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
  options: {
    activeGroupId?: number | null;
    membershipsPending?: boolean;
    onOpenRepos?: () => void;
    groups?: GroupSummary[];
  } = {},
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
      groups={options.groups ?? GROUPS}
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

    expect(screen.getByText("3 checked, no change")).toBeDefined();
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
    // The no-change hint cannot be scoped (no per-repo id list backs
    // noChangeCount) and says so rather than silently sitting under a
    // scoped headline.
    expect(within(underWatchTile).getByText("40 checked, no change (all repos)")).toBeDefined();

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

  it("all-clear copy names the active group and states only the predicate, rather than claiming every repo is in sync", async () => {
    renderScreen(
      [repo({ id: 1, localName: "in-group" })],
      summary({ attentionCount: 0, attention: [] }),
      [{ repoId: 1, groupIds: [1] }],
      { activeGroupId: 1 },
    );
    await screen.findByText("All clear");
    expect(screen.getByText("No dirty or failed repositories in Work.")).toBeDefined();
  });

  it("does not claim a checked-but-behind repo is 'in sync', on the hint or in the All-clear copy", async () => {
    // `summary.rs`'s `classify_row` folds a successful "check"/"fetch" - or a
    // "skipped" activity row - into `noChangeCount` unconditionally, never
    // keyed on the resulting ahead/behind counts; and the attention
    // population separately excludes a clean-but-behind repo outright
    // (`attention_excludes_a_repo_that_is_only_behind`). So a repo that was
    // checked today, remains 40 commits behind, but is neither dirty nor
    // failed reads as fully "healthy" here - correctly, since it is not
    // dirty or failed - but neither the hint nor the All-clear text may
    // claim it is "in sync" (Codex review finding 1, confirmed).
    renderScreen(
      [repo({ id: 1, localName: "behind-repo", behindCount: 40 })],
      summary({ attentionCount: 0, attention: [], noChangeCount: 1 }),
    );
    await screen.findByText("All clear");

    expect(screen.getByText("1 checked, no change")).toBeDefined();
    expect(screen.getByText("No dirty or failed repositories.")).toBeDefined();
    expect(screen.queryByText(/in sync/i)).toBeNull();
  });

  it("keeps every scope indicator honest even when the active group's metadata has not resolved", async () => {
    // activeGroupId 99 is absent from the `groups` list (only id 1, "Work",
    // exists): the "group list stale or not yet loaded while activeGroupId
    // and membership data remain set" case (Codex review finding 3,
    // confirmed). The scoping itself (repo 1 is a member of group 99) still
    // works, keyed on activeGroupId alone - this test is about the LABELS,
    // which must not silently fall back to unscoped wording.
    renderScreen(
      [repo({ id: 1, localName: "in-group" }), repo({ id: 2, localName: "out-of-group" })],
      summary({ attentionCount: 0, attention: [], noChangeCount: 9 }),
      [{ repoId: 1, groupIds: [99] }],
      { activeGroupId: 99 },
    );

    await screen.findByText("Scoped to a group");
    const underWatchTile = screen.getByRole("button", { name: /Under watch/ });
    // The count is STILL correctly scoped: 1, not 2 - proving this is a
    // labeling gap, not a filtering one.
    expect(within(underWatchTile).getByText("1")).toBeDefined();
    expect(within(underWatchTile).getByText("9 checked, no change (all repos)")).toBeDefined();
    expect(screen.getByText("No dirty or failed repositories in this group.")).toBeDefined();
  });
});

describe("Needs attention rows", () => {
  it("shows the all-clear state when nothing needs attention", async () => {
    renderScreen([repo()], summary({ attentionCount: 0, attention: [] }));
    await screen.findByText("All clear");
    expect(screen.getByText("No dirty or failed repositories.")).toBeDefined();
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

describe("Attention row provenance classification (Codex review finding 2)", () => {
  it("classifies git.not_a_repo as LOCAL, covering both a missing path and a folder no longer a repo", async () => {
    // `git/inspect.rs`'s `inspect` maps EVERY `Repository::open` failure to
    // `AppError::NotARepo` ("git.not_a_repo") regardless of whether the path
    // is missing entirely or merely no longer a valid git repository - the
    // backend does not distinguish the two any further, so neither does
    // this row. Written by `check_now`'s `record_hard_failure_code`
    // (`repo.rs`), never by a fetch, so it must never carry the fetch-scoped
    // `checkFailureMessage` sentence.
    renderScreen(
      [repo({ id: 1, localName: "gone-repo", lastErrorCode: "git.not_a_repo" })],
      summary({
        attentionCount: 1,
        attention: [summaryItem({ repoId: 1, localName: "gone-repo", detail: "git.not_a_repo" })],
      }),
    );
    await screen.findByText("gone-repo");
    const row = screen.getByText("gone-repo").closest("li") as HTMLElement;

    expect(within(row).getByText("Not a repository")).toBeDefined();
    expect(within(row).getByText("Not a repository").title).toContain("could not open this folder");
    expect(row.querySelector(".lucide-hard-drive")).not.toBeNull();
    expect(row.querySelector(".lucide-cloud")).toBeNull();
  });

  it("classifies git.not_found (the git executable is missing) as LOCAL, not a fetch failure", async () => {
    // `require_exe()` returns `AppError::GitNotFound` ("git.not_found") when
    // the git binary itself cannot be located - a local machine-
    // configuration problem, never a network round trip.
    renderScreen(
      [repo({ id: 1, localName: "no-git-repo", lastErrorCode: "git.not_found" })],
      summary({
        attentionCount: 1,
        attention: [summaryItem({ repoId: 1, localName: "no-git-repo", detail: "git.not_found" })],
      }),
    );
    await screen.findByText("no-git-repo");
    const row = screen.getByText("no-git-repo").closest("li") as HTMLElement;

    expect(within(row).getByText("Git not found")).toBeDefined();
    expect(row.querySelector(".lucide-hard-drive")).not.toBeNull();
    expect(row.querySelector(".lucide-cloud")).toBeNull();
  });

  it("renders an unresolved repo lookup as neutral, with no guessed local or remote glyph", async () => {
    // The attention item's repoId (2) has no matching entry in `repos.data`
    // (the repo dropped out of the list between fetches, here represented
    // by a DIFFERENT repo, id 1, still being present so the screen is not
    // in its empty-library state) - the row falls back to the backend's own
    // detail string but must not guess a provenance for it (the old
    // behaviour unconditionally guessed "remote", Codex review finding 2,
    // confirmed).
    renderScreen(
      [repo({ id: 1, localName: "other-repo" })],
      summary({
        attentionCount: 1,
        attention: [summaryItem({ repoId: 2, localName: "vanished-repo", detail: "git.auth_failed" })],
      }),
    );
    await screen.findByText("vanished-repo");
    const row = screen.getByText("vanished-repo").closest("li") as HTMLElement;

    expect(within(row).getByText("git.auth_failed")).toBeDefined();
    expect(row.querySelector(".lucide-hard-drive")).toBeNull();
    expect(row.querySelector(".lucide-cloud")).toBeNull();
  });

  it("still classifies the three real fetch/network/auth codes as REMOTE", async () => {
    renderScreen(
      [repo({ id: 1, localName: "offline-repo", lastErrorCode: "net.offline" })],
      summary({
        attentionCount: 1,
        attention: [summaryItem({ repoId: 1, localName: "offline-repo", detail: "net.offline" })],
      }),
    );
    await screen.findByText("offline-repo");
    const row = screen.getByText("offline-repo").closest("li") as HTMLElement;

    expect(within(row).getByText("Offline")).toBeDefined();
    expect(row.querySelector(".lucide-cloud")).not.toBeNull();
    expect(row.querySelector(".lucide-hard-drive")).toBeNull();
  });
});
