import type {
  ActivityRecord,
  SummaryItem,
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

export const NOW = Math.floor(Date.now() / 1000);
const MIN = 60;
const HOUR = 3600;
const DAY = 86400;

/** The group colours the app stores: oklch strings, never colour names. */
export const GROUP_COLOR_BLUE = "oklch(0.55 0.16 264)";
export const GROUP_COLOR_GREEN = "oklch(0.55 0.13 150)";

/** A healthy, in-sync repository; every other fixture repo is this plus a change. */
export const REPO_TEMPLATE: RepoSummary = {
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
  { ...REPO_TEMPLATE, id: 1, localName: "example-repo" },
  {
    ...REPO_TEMPLATE,
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
    ...REPO_TEMPLATE,
    id: 3,
    localName: "sample-service",
    localPath: "C:\\Code\\sample-service",
    remoteOriginUrl: "https://github.com/example-org/sample-service.git",
    behindCount: 14,
    lastCheckedAt: NOW - 3 * HOUR,
    openPrCount: 5,
  },
  {
    ...REPO_TEMPLATE,
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
    ...REPO_TEMPLATE,
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
    ...REPO_TEMPLATE,
    id: 6,
    localName: "archived-demo",
    localPath: "C:\\Code\\archived-demo",
    remoteOriginUrl: "https://github.com/example-org/archived-demo.git",
    enabled: false,
    lastCheckedAt: NOW - 30 * DAY,
  },
  {
    ...REPO_TEMPLATE,
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
    ...REPO_TEMPLATE,
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

export function toDetail(r: RepoSummary, checkFrequencyMin = 0): RepoDetail {
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
    checkFrequencyMin,
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

export const MEMBERSHIPS: RepoGroupMembership[] = [
  { repoId: 1, groupIds: [1] },
  { repoId: 2, groupIds: [1] },
  { repoId: 3, groupIds: [1] },
  { repoId: 4, groupIds: [2] },
  { repoId: 5, groupIds: [2] },
];

export const ACTIVITY: ActivityRecord[] = [
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
  {
    id: 5,
    repoId: 1,
    timestamp: NOW - 6 * HOUR,
    actionType: "check",
    status: "success",
    reasonCode: null,
    summary: "Up to date with origin",
    commitRange: null,
    rawCommand: "git fetch --prune origin",
    rawStdout: null,
    rawStderr: null,
    exitCode: 0,
    durationMs: 390,
  },
  {
    id: 6,
    repoId: 2,
    timestamp: NOW - 8 * HOUR,
    actionType: "update",
    status: "failed",
    reasonCode: "git.not_fast_forward",
    summary: "Could not fast-forward",
    commitRange: null,
    rawCommand: "git pull --ff-only",
    rawStdout: null,
    rawStderr: "fatal: Not possible to fast-forward, aborting.",
    exitCode: 128,
    durationMs: 960,
  },
  {
    id: 7,
    repoId: 3,
    timestamp: NOW - DAY,
    actionType: "check",
    status: "success",
    reasonCode: null,
    summary: "14 commits behind origin",
    commitRange: null,
    rawCommand: "git fetch --prune origin",
    rawStdout: null,
    rawStderr: null,
    exitCode: 0,
    durationMs: 510,
  },
  {
    id: 8,
    repoId: 5,
    timestamp: NOW - 2 * DAY,
    actionType: "check",
    status: "failed",
    reasonCode: "net.offline",
    summary: "Could not reach the remote",
    commitRange: null,
    rawCommand: "git fetch --prune origin",
    rawStdout: null,
    rawStderr: "fatal: unable to access the remote",
    exitCode: 128,
    durationMs: 5002,
  },
];

export const SETTINGS: Settings = {
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

export const DIAGNOSTICS: Diagnostics = {
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

export const UPDATE_AVAILABILITY: UpdateAvailability = {
  currentVersion: "0.9.0",
  available: false,
  newVersion: null,
  notes: null,
  error: null,
};

export const DB_RECOVERY: DbRecoveryNotice = { recovered: false, backupPath: null };

/** The day's notable events. Counts and scope are derived from these in handlers.ts. */
export const SUMMARY_UPDATED: SummaryItem[] = [
  { repoId: 3, localName: "sample-service", detail: "Fast-forwarded 14 commits" },
];
export const SUMMARY_RELEASES: SummaryItem[] = [
  { repoId: 1, localName: "example-repo", detail: "v1.4.0" },
];
export const SUMMARY_ATTENTION: SummaryItem[] = [
  { repoId: 4, localName: "scratch-notes", detail: "Uncommitted changes" },
  { repoId: 5, localName: "legacy-tooling", detail: "Could not reach the remote" },
];

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

