import type {
  ActivityRecord,
  DailySummary,
  Diagnostics,
  GroupSummary,
  RepoDetail,
  RepoGroupMembership,
  RepoSummary,
  ScanResult,
  Settings,
  UpdateAvailability,
  DbRecoveryNotice,
} from "@/lib/bindings";

/**
 * Fixture data for the component gallery, typed against the generated IPC
 * bindings. Every object is annotated with its wire type, so a field added,
 * removed or renamed in `bindings.ts` fails `pnpm typecheck` here instead of
 * silently rendering a half-empty gallery.
 *
 * Paths and names are invented. Nothing here comes from a real machine.
 */

export type Scenario = "populated" | "empty" | "loading";

const NOW = Math.floor(Date.now() / 1000);
const MIN = 60;
const HOUR = 3600;
const DAY = 86400;

/** The group colours the app stores: oklch strings, never colour names. */
export const GROUP_COLOR_BLUE = "oklch(0.55 0.16 264)";
export const GROUP_COLOR_GREEN = "oklch(0.55 0.13 150)";

const base: RepoSummary = {
  id: 1,
  localName: "example-repo",
  localPath: "C:\\Code\\example-repo",
  remoteOriginUrl: "https://github.com/example-org/example-repo.git",
  hostType: "github",
  aheadCount: 0,
  behindCount: 0,
  isDirty: false,
  isDetached: false,
  enabled: true,
  autoPaused: false,
  lastCheckedAt: NOW - 10 * MIN,
  lastErrorCode: null,
  latestReleaseTag: "v1.4.0",
  openPrCount: 2,
  lastLocalCommitAt: NOW - 2 * DAY,
  activeBranch: "main",
  headState: "branch",
  updateMode: "pull_ff_only",
  upstreamState: "tracking",
  stars: 120,
  forks: 8,
  license: "MIT",
  size: 2048,
  visibility: "public",
  homepage: "https://example.org/example-repo",
};

/** One repository per status the app can derive, plus a long-name edge case. */
export const REPOS: RepoSummary[] = [
  { ...base, id: 1, localName: "example-repo" },
  {
    ...base,
    id: 2,
    localName: "widget-forge",
    localPath: "C:\\Code\\widget-forge",
    remoteOriginUrl: "https://github.com/example-org/widget-forge.git",
    aheadCount: 3,
    latestReleaseTag: null,
    openPrCount: null,
    stars: 4,
  },
  {
    ...base,
    id: 3,
    localName: "sample-service",
    localPath: "C:\\Code\\sample-service",
    remoteOriginUrl: "https://github.com/example-org/sample-service.git",
    behindCount: 14,
    lastCheckedAt: NOW - 3 * HOUR,
    openPrCount: 5,
  },
  {
    ...base,
    id: 4,
    localName: "scratch-notes",
    localPath: "C:\\Code\\scratch-notes",
    remoteOriginUrl: null,
    hostType: "unknown",
    isDirty: true,
    latestReleaseTag: null,
    openPrCount: null,
    stars: null,
    forks: null,
    license: null,
    visibility: null,
    homepage: null,
  },
  {
    ...base,
    id: 5,
    localName: "legacy-tooling",
    localPath: "C:\\Code\\legacy-tooling",
    remoteOriginUrl: "https://github.com/example-org/legacy-tooling.git",
    lastErrorCode: "fetch_failed",
    lastCheckedAt: NOW - DAY,
    aheadCount: null,
    behindCount: null,
  },
  {
    ...base,
    id: 6,
    localName: "archived-demo",
    localPath: "C:\\Code\\archived-demo",
    remoteOriginUrl: "https://github.com/example-org/archived-demo.git",
    enabled: false,
    lastCheckedAt: NOW - 30 * DAY,
  },
  {
    ...base,
    id: 7,
    localName: "local-only-experiment",
    localPath: "C:\\Code\\local-only-experiment",
    remoteOriginUrl: null,
    hostType: "unknown",
    upstreamState: "none",
    aheadCount: null,
    behindCount: null,
    latestReleaseTag: null,
    openPrCount: null,
    stars: null,
    forks: null,
  },
  {
    ...base,
    id: 8,
    localName: "a-repository-with-a-very-long-name-for-truncation",
    localPath:
      "C:\\Code\\some-organisation\\deeply\\nested\\working\\trees\\a-repository-with-a-very-long-name-for-truncation",
    remoteOriginUrl: null,
    hostType: "unknown",
    headState: "unborn",
    activeBranch: null,
    upstreamState: null,
    aheadCount: null,
    behindCount: null,
    latestReleaseTag: null,
    openPrCount: null,
  },
];

function toDetail(r: RepoSummary): RepoDetail {
  return {
    id: r.id,
    localName: r.localName,
    hostType: r.hostType,
    aheadCount: r.aheadCount,
    behindCount: r.behindCount,
    isDirty: r.isDirty,
    isDetached: r.isDetached,
    enabled: r.enabled,
    autoPaused: r.autoPaused,
    lastCheckedAt: r.lastCheckedAt,
    lastErrorCode: r.lastErrorCode,
    latestReleaseTag: r.latestReleaseTag,
    localPath: r.localPath,
    remoteOriginUrl: r.remoteOriginUrl,
    defaultBranch: "main",
    updateMode: r.updateMode,
    checkFrequencyMin: 0,
    createdAt: NOW - 30 * DAY,
    notes: null,
    activeBranch: r.activeBranch,
    headSha: "abcdef1234567890abcdef1234567890abcdef12",
    upstreamBranch: r.upstreamState === "tracking" ? "origin/main" : null,
    headState: r.headState,
    upstreamState: r.upstreamState,
    lastLocalCommitAt: r.lastLocalCommitAt,
    lastUpdatedAt: NOW - DAY,
    lastAttemptedAt: r.lastCheckedAt,
    nextCheckAt: NOW + 6 * HOUR,
    consecutiveFailures: r.lastErrorCode ? 3 : 0,
    description: r.remoteOriginUrl ? "An invented repository used to exercise the gallery." : null,
    topicsJson: null,
    latestReleaseAt: r.latestReleaseTag ? NOW - 5 * DAY : null,
    latestReleaseUrl: r.latestReleaseTag
      ? `https://github.com/example-org/${r.localName}/releases/tag/${r.latestReleaseTag}`
      : null,
    isArchived: false,
    lastRemoteSha: null,
    lastFetchedAt: r.lastCheckedAt,
    openPrCount: r.openPrCount,
    defaultBranchPrCount: r.openPrCount === null ? null : 1,
    prLastCheckedAt: r.openPrCount === null ? null : NOW - 10 * MIN,
    stars: r.stars,
    forks: r.forks,
    license: r.license,
    size: r.size,
    visibility: r.visibility,
    homepage: r.homepage,
  };
}

export const GROUPS: GroupSummary[] = [
  { id: 1, name: "work", color: GROUP_COLOR_BLUE, repoCount: 3 },
  { id: 2, name: "personal", color: GROUP_COLOR_GREEN, repoCount: 2 },
];

const MEMBERSHIPS: RepoGroupMembership[] = [
  { repoId: 1, groupIds: [1] },
  { repoId: 2, groupIds: [1] },
  { repoId: 3, groupIds: [1] },
  { repoId: 4, groupIds: [2] },
  { repoId: 5, groupIds: [2] },
];

const ACTIVITY: ActivityRecord[] = [
  {
    id: 1,
    repoId: 1,
    timestamp: NOW - HOUR,
    actionType: "check",
    status: "success",
    reasonCode: null,
    summary: "Up to date with origin",
    commitRange: null,
    rawCommand: "git fetch --prune origin",
    rawStdout: null,
    rawStderr: null,
    exitCode: 0,
    durationMs: 420,
  },
  {
    id: 2,
    repoId: 3,
    timestamp: NOW - 2 * HOUR,
    actionType: "update",
    status: "success",
    reasonCode: null,
    summary: "Fast-forwarded 14 commits",
    commitRange: "a1b2c3d..e4f5a6b",
    rawCommand: "git pull --ff-only",
    rawStdout: "Updating a1b2c3d..e4f5a6b\nFast-forward",
    rawStderr: null,
    exitCode: 0,
    durationMs: 1830,
  },
  {
    id: 3,
    repoId: 5,
    timestamp: NOW - 3 * HOUR,
    actionType: "check",
    status: "failed",
    reasonCode: "fetch_failed",
    summary: "Could not reach the remote",
    commitRange: null,
    rawCommand: "git fetch --prune origin",
    rawStdout: null,
    rawStderr: "fatal: unable to access the remote: Could not resolve host",
    exitCode: 128,
    durationMs: 5010,
  },
  {
    id: 4,
    repoId: 4,
    timestamp: NOW - 5 * HOUR,
    actionType: "update",
    status: "skipped",
    reasonCode: "dirty_worktree",
    summary: "Skipped: uncommitted changes",
    commitRange: null,
    rawCommand: null,
    rawStdout: null,
    rawStderr: null,
    exitCode: null,
    durationMs: null,
  },
];

const SETTINGS: Settings = {
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

const DIAGNOSTICS: Diagnostics = {
  appVersion: "0.9.0",
  dataDir: "C:\\Users\\example\\AppData\\Local\\RepoSync",
  dbPath: "C:\\Users\\example\\AppData\\Local\\RepoSync\\reposync.sqlite3",
  logDir: "C:\\Users\\example\\AppData\\Local\\RepoSync\\logs",
  loggingActive: true,
  logLevel: "INFO",
  logMaxFiles: 14,
  logMaxBytes: 10485760,
  logDirReadable: true,
  logFileCount: 6,
  logBytes: 842112,
  logWriteFailures: 0,
  logLastWriteFailureAt: null,
  logBytesWritten: 842112,
  logDroppedLines: 0,
  onedriveRooted: false,
  gitPath: "C:\\Program Files\\Git\\cmd\\git.exe",
  gitVersion: "2.47.1.windows.1",
  gitResolved: true,
  gitExplicitPath: null,
  gitExplicitPathHonored: null,
  gitMeetsFloor: true,
  schedulerCycles: 214,
  schedulerReposChecked: 428,
  schedulerOutcomePersistFailures: 0,
  dbRecovered: false,
};

const UPDATE_AVAILABILITY: UpdateAvailability = {
  currentVersion: "0.9.0",
  available: false,
  newVersion: null,
  notes: null,
  error: null,
};

const DB_RECOVERY: DbRecoveryNotice = { recovered: false, backupPath: null };

const TODAY = new Date().toISOString().slice(0, 10);

const SUMMARY_POPULATED: DailySummary = {
  date: TODAY,
  updatedCount: 1,
  releasesCount: 1,
  attentionCount: 2,
  noChangeCount: 4,
  updated: [{ repoId: 3, localName: "sample-service", detail: "Fast-forwarded 14 commits" }],
  newReleases: [{ repoId: 1, localName: "example-repo", detail: "v1.4.0" }],
  attention: [
    { repoId: 4, localName: "scratch-notes", detail: "Uncommitted changes" },
    { repoId: 5, localName: "legacy-tooling", detail: "Could not reach the remote" },
  ],
};

const SUMMARY_EMPTY: DailySummary = {
  date: TODAY,
  updatedCount: 0,
  releasesCount: 0,
  attentionCount: 0,
  noChangeCount: 0,
  updated: [],
  newReleases: [],
  attention: [],
};

/** Candidates for the Add-repositories dialog's scan: one already tracked. */
export const SCAN_CANDIDATES: ScanResult["discovered"] = [
  {
    localPath: "C:\\Code\\example-repo",
    localName: "example-repo",
    alreadyTracked: true,
    remoteOriginUrl: "https://github.com/example-org/example-repo.git",
  },
  {
    localPath: "C:\\Code\\new-project",
    localName: "new-project",
    alreadyTracked: false,
    remoteOriginUrl: "https://github.com/example-org/new-project.git",
  },
  {
    localPath: "C:\\Code\\scratch-pad",
    localName: "scratch-pad",
    alreadyTracked: false,
    remoteOriginUrl: null,
  },
];

/**
 * Everything the stubbed commands answer with, for one scenario. `empty` gives
 * the zero form of every list so the real empty states render; `loading` uses
 * the populated data (the stub never resolves it, so it is never shown).
 */
export function fixturesFor(scenario: Scenario) {
  const empty = scenario === "empty";
  return {
    repos: empty ? [] : REPOS,
    groups: empty ? [] : GROUPS,
    memberships: empty ? [] : MEMBERSHIPS,
    activity: empty ? [] : ACTIVITY,
    settings: SETTINGS,
    diagnostics: DIAGNOSTICS,
    updateAvailability: UPDATE_AVAILABILITY,
    dbRecovery: DB_RECOVERY,
    summary: empty ? SUMMARY_EMPTY : SUMMARY_POPULATED,
    detailFor: (id: number): RepoDetail => toDetail(REPOS.find((r) => r.id === id) ?? REPOS[0]),
    groupsForRepo: (repoId: number): number[] =>
      empty ? [] : (MEMBERSHIPS.find((m) => m.repoId === repoId)?.groupIds ?? []),
    scan: (parentPath: string | null): ScanResult => ({
      parentPath: parentPath ?? "C:\\Code",
      discovered: SCAN_CANDIDATES,
    }),
  };
}
