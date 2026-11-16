import type {
  ActivityRecord,
  GroupSummary,
  RepoGroupMembership,
  HeadState,
  RepoSummary,
  SummaryItem,
} from "@/lib/bindings";
import { GROUPS, NOW, REPO_TEMPLATE, type Scenario } from "@/gallery/fixtures";

/**
 * Stress fixtures: the libraries the real screens must survive, not just show.
 *
 * Everything is generated from the index, with no `Math.random`, so a screenshot
 * taken today matches one taken next week. Names, paths and URLs are invented.
 * Each fixture is typed against the generated bindings, like the small ones.
 */

export const STRESS_REPO_COUNT = 200;

const MIN = 60;
const DAY = 86400;

const WORDS = [
  "orchestration", "telemetry", "ingestion", "reconciliation", "scheduler", "gateway", "pipeline",
  "renderer", "indexer", "migration", "dashboard", "connector", "analytics", "registry", "workbench",
];
const ORGS = [
  "example-organisation-with-an-unreasonably-long-name",
  "acme-platform-engineering-and-reliability-group",
  "tiny",
  "example-org",
];
const HOSTS = ["github", "gitlab", "bitbucket", "unknown"] as const;
const ROOTS = [
  "C:\\Code",
  "D:\\Projects\\clients\\some-very-long-client-name-limited\\engagements\\2026-annual-platform-review",
  "C:\\Users\\example-person\\Documents\\Work in Progress\\Repositories checked out for later",
];

/** A short invented name for index `i`, so most rows read like ordinary repos. */
function shortName(i: number): string {
  return `${WORDS[i % WORDS.length]}-${WORDS[(i * 7 + 3) % WORDS.length]}-${i}`;
}

/** A name long enough to defeat any column: 90 to 130 characters. */
function longName(i: number): string {
  const parts = [shortName(i), WORDS[(i * 3) % WORDS.length], WORDS[(i * 5 + 1) % WORDS.length], "service", "monorepo"];
  return `${parts.join("-")}-${"extended-".repeat(1 + (i % 3))}variant-${i}`;
}

const LONG_BRANCHES = [
  "feature/PLAT-48213-rework-the-scheduler-to-batch-fetches-and-respect-quiet-hours-for-metered-connections",
  "release/2026.11.0-rc.3+build.20261113.sha.abcdef1234567890",
  "dependabot/npm_and_yarn/example-org/very-long-package-name-with-scope-and-more-than-fifty-characters-in-it-4.12.9",
  "main",
  "develop",
  "hotfix/2026-11-13-urgent-fix-for-the-thing-that-broke-everything-in-production",
];

const ERROR_CODES = [
  "git.auth_failed",
  "net.offline",
  "git.fetch_failed",
  "git.not_a_repo",
  "git.not_found",
  "git.ff_not_possible",
  "git.command_failed",
  // Codes the frontend has not been taught: they render as their own text.
  "git.remote_rejected_the_push_because_the_branch_protection_policy_requires_a_signed_commit",
  "net.tls_handshake_failed_certificate_chain_could_not_be_verified_against_the_system_trust_store",
  "fs.permission_denied_while_reading_the_packed_refs_file_in_the_dot_git_directory",
];

function remoteUrl(i: number): string | null {
  const host = HOSTS[i % HOSTS.length];
  if (host === "unknown") return i % 8 === 3 ? null : `ssh://deploy@git.internal.example.test:2222/srv/git/${longName(i)}.git`;
  const org = ORGS[i % ORGS.length];
  const base = host === "github" ? "github.com" : host === "gitlab" ? "gitlab.com" : "bitbucket.org";
  const name = i % 3 === 0 ? longName(i) : shortName(i);
  return `https://${base}/${org}/${name}.git?ref=refs/heads/${LONG_BRANCHES[i % LONG_BRANCHES.length].replace(/\//g, "%2F")}`;
}

function stressRepo(i: number, forceFailed: boolean): RepoSummary {
  const id = i + 1;
  const long = i % 4 === 0;
  const localName = long ? longName(i) : shortName(i);
  const root = ROOTS[i % ROOTS.length];
  const depth = i % 5 === 0 ? "\\team-workspaces\\shared\\vendored\\third-party\\forks\\experimental" : "";
  const hostType = HOSTS[i % HOSTS.length];
  const failed = forceFailed || i % 9 === 4;
  const behindHeavy = i % 11 === 2;
  const aheadHeavy = i % 13 === 5;
  // Choose HEAD first, then derive every field that depends on it, so no repo
  // contradicts itself. Per bindings.ts: a detached or unborn HEAD has no
  // branch name, `unborn` and `detached` are exclusive, and with no branch
  // there is no upstream to be ahead of or behind. `null` is "not observed".
  const head: HeadState | null = i % 19 === 0 ? "unborn" : i % 17 === 0 ? "detached" : i % 43 === 5 ? null : "branch";
  const onBranch = head === "branch";
  return {
    ...REPO_TEMPLATE,
    id,
    localName,
    localPath: `${root}${depth}\\${localName}`,
    remoteOriginUrl: remoteUrl(i),
    hostType,
    aheadCount: failed || !onBranch ? null : aheadHeavy ? 1200 + i * 37 : i % 6 === 1 ? i % 40 : 0,
    behindCount: failed || !onBranch ? null : behindHeavy ? 98765 + i : i % 7 === 3 ? 1 + (i % 300) : 0,
    isDirty: i % 10 === 6,
    isDetached: head === "detached",
    enabled: forceFailed || i % 23 !== 0,
    autoPaused: !forceFailed && i % 29 === 0,
    lastCheckedAt: i % 31 === 0 && !forceFailed ? null : NOW - ((i * 17) % 4000) * MIN,
    lastErrorCode: failed ? ERROR_CODES[(forceFailed ? i : Math.floor(i / 9)) % ERROR_CODES.length] : null,
    latestReleaseTag: i % 3 === 0 ? null : `v${1 + (i % 12)}.${i % 50}.${i % 9}-release-candidate.${i}+build.2026.11.13`,
    openPrCount: i % 4 === 1 ? null : (i * 13) % 1500,
    lastLocalCommitAt: NOW - ((i * 29) % 900) * DAY,
    activeBranch: onBranch ? LONG_BRANCHES[i % LONG_BRANCHES.length] : null,
    headState: head,
    upstreamState: !onBranch ? null : i % 15 === 7 ? "none" : "tracking",
    stars: i % 5 === 2 ? null : i * 1337,
    forks: i % 5 === 2 ? null : i * 211,
    license: i % 6 === 0 ? null : "LicenseRef-Example-Proprietary-Source-Available-1.0",
    size: 1024 * (1 + i * 913),
    visibility: i % 7 === 0 ? null : i % 3 === 0 ? "private" : "public",
    homepage: i % 4 === 0 ? null : `https://docs.example.test/${localName}`,
  };
}

/** 200 repositories that exercise every column and chip at its worst. */
export const STRESS_REPOS: RepoSummary[] = Array.from({ length: STRESS_REPO_COUNT }, (_, i) => stressRepo(i, false));

/** The same 200, every one of them failed, with the full range of error text. */
export const ALL_FAILED_REPOS: RepoSummary[] = Array.from({ length: STRESS_REPO_COUNT }, (_, i) => stressRepo(i, true));

const GROUP_NAMES = [
  "work", "personal", "clients",
  "archive-of-things-i-might-want-again-someday-but-probably-will-not",
  "q4-planning-and-budget-review-working-copies", "oss", "x", "experiments-and-spikes",
  "vendor-forks-pending-upstream-merge", "docs", "infra", "mobile", "data-platform-and-analytics-engineering",
  "security-review-queue", "design-system", "legacy", "onboarding-material-for-new-hires-2026",
  "release-engineering", "research", "tooling", "sandbox", "contract-work-for-a-client-with-a-long-name",
  "reading-list", "reference-implementations", "ml",
];
const GROUP_COLORS = [
  "oklch(0.55 0.16 264)", "oklch(0.55 0.13 150)", "oklch(0.58 0.17 25)", "oklch(0.6 0.14 80)",
  "oklch(0.5 0.15 310)", null,
];

/** Twenty-five groups, several with names that cannot fit a sidebar. */
export const STRESS_GROUPS: GroupSummary[] = GROUP_NAMES.map((name, i) => ({
  id: i + 1,
  name,
  color: GROUP_COLORS[i % GROUP_COLORS.length],
  repoCount: STRESS_REPOS.filter((_, r) => membershipIds(r).includes(i + 1)).length,
}));

/** The group list a screen should be handed for a data state. */
export function groupsForScenario(scenario: Scenario): GroupSummary[] {
  if (scenario === "stress" || scenario === "allFailed") return STRESS_GROUPS;
  if (scenario === "empty") return [];
  return GROUPS;
}

/** Every repo in one to three groups, spread across the 25. */
function membershipIds(index: number): number[] {
  const n = GROUP_NAMES.length;
  const ids = new Set<number>([(index % n) + 1]);
  if (index % 3 === 0) ids.add(((index * 5 + 2) % n) + 1);
  if (index % 7 === 0) ids.add(((index * 11 + 4) % n) + 1);
  return [...ids];
}

export const STRESS_MEMBERSHIPS: RepoGroupMembership[] = STRESS_REPOS.map((r, i) => ({
  repoId: r.id,
  groupIds: membershipIds(i),
}));

const LONG_SUMMARY =
  "fatal: unable to access 'https://github.com/example-organisation-with-an-unreasonably-long-name/repository.git/': SSL certificate problem: unable to get local issuer certificate while negotiating a connection through the corporate proxy";

function activityFor(repos: RepoSummary[]): ActivityRecord[] {
  return repos.slice(0, 60).flatMap((r, i): ActivityRecord[] => {
    const failed = r.lastErrorCode !== null;
    return [
      {
        id: i * 2 + 1,
        repoId: r.id,
        timestamp: NOW - i * 13 * MIN,
        actionType: i % 3 === 0 ? "update" : "check",
        status: failed ? "failed" : i % 5 === 0 ? "skipped" : "success",
        reasonCode: failed ? r.lastErrorCode : null,
        summary: failed ? LONG_SUMMARY : `Fast-forwarded ${(i * 311) % 5000} commits on ${r.activeBranch ?? "an unborn branch"}`,
        commitRange: i % 3 === 0 ? "a1b2c3d4e5f6a7b8c9d0..e4f5a6b7c8d9e0f1a2b3" : null,
        rawCommand: "git fetch --prune --tags --force --recurse-submodules=on-demand origin",
        rawStdout: null,
        rawStderr: failed ? `${LONG_SUMMARY}\nfatal: the remote end hung up unexpectedly` : null,
        exitCode: failed ? 128 : 0,
        durationMs: 400 + i * 97,
      },
    ];
  });
}

export const STRESS_ACTIVITY: ActivityRecord[] = activityFor(STRESS_REPOS);
export const ALL_FAILED_ACTIVITY: ActivityRecord[] = activityFor(ALL_FAILED_REPOS);

function summaryFor(repos: RepoSummary[]) {
  const attention: SummaryItem[] = repos
    .filter((r) => r.lastErrorCode !== null || r.isDirty)
    .map((r) => ({ repoId: r.id, localName: r.localName, detail: r.lastErrorCode ?? "Uncommitted changes" }));
  const updated: SummaryItem[] = repos
    .filter((r) => r.lastErrorCode === null && (r.behindCount ?? 0) > 0)
    .slice(0, 30)
    .map((r) => ({ repoId: r.id, localName: r.localName, detail: `Fast-forwarded ${r.behindCount} commits` }));
  const newReleases: SummaryItem[] = repos
    .filter((r) => r.latestReleaseTag !== null)
    .slice(0, 30)
    .map((r) => ({ repoId: r.id, localName: r.localName, detail: r.latestReleaseTag ?? "" }));
  return { updated, newReleases, attention };
}

export const STRESS_SUMMARY = summaryFor(STRESS_REPOS);
export const ALL_FAILED_SUMMARY = summaryFor(ALL_FAILED_REPOS);
