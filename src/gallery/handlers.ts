import type { commands } from "@/lib/bindings";
import type {
  ActivityRecord,
  AppErrorPayload,
  GroupSummary,
  RepoDetail,
  RepoSummary,
  Settings,
} from "@/lib/bindings";
import {
  ALL_FAILED_ACTIVITY,
  ALL_FAILED_REPOS,
  ALL_FAILED_SUMMARY,
  STRESS_ACTIVITY,
  STRESS_GROUPS,
  STRESS_MEMBERSHIPS,
  STRESS_REPOS,
  STRESS_SUMMARY,
} from "@/gallery/fixtures-stress";
import {
  ACTIVITY,
  DB_RECOVERY,
  DIAGNOSTICS,
  GROUPS,
  MEMBERSHIPS,
  NOW,
  REPOS,
  REPO_TEMPLATE,
  SCAN_CANDIDATES,
  SETTINGS,
  SUMMARY_ATTENTION,
  SUMMARY_RELEASES,
  SUMMARY_UPDATED,
  UPDATE_AVAILABILITY,
  toDetail,
  type Scenario,
} from "@/gallery/fixtures";

/**
 * The stubbed IPC commands, typed against the generated bindings.
 *
 * Pure module: it touches no `window`, so a test can call it directly.
 * `stub-ipc.ts` is the thin installer that wires it to the Tauri globals.
 *
 * Three guarantees, each enforced by the compiler or by `handlers.test.ts`:
 *
 * 1. Every handler has the parameter list of the real command and returns the
 *    real success payload. `Handlers` is derived from `typeof commands`, so a
 *    wrong payload shape, or a command whose signature changed, fails typecheck.
 * 2. Every command a gallery surface can call has a handler. `CALLED_COMMANDS`
 *    is the list, and the test greps `src/` for `commands.<name>` to prove it
 *    has not gone stale.
 * 3. A command with no handler REJECTS, naming the command. It never resolves
 *    to a made-up value, because a fake success hides a missing stub as a
 *    plausible-looking screen.
 */

type Commands = typeof commands;

/** The success payload of a generated command, whichever shape it wraps it in. */
type Payload<K extends keyof Commands> =
  Awaited<ReturnType<Commands[K]>> extends infer R
    ? R extends { status: "ok"; data: infer D }
      ? D
      : R extends { status: "error" }
        ? never
        : R
    : never;

/** Every command a rendered gallery surface can call. */
export const CALLED_COMMANDS = [
  "activityList",
  "appCheckForUpdate",
  "appInstallUpdate",
  "dbRecoveryNotice",
  "diagnosticsGet",
  "diagnosticsOpenLogDir",
  "groupAssign",
  "groupCreate",
  "groupDelete",
  "groupList",
  "groupUnassign",
  "groupUpdate",
  "groupsForRepo",
  "repoAddPath",
  "repoCheckAll",
  "repoCheckNow",
  "repoGet",
  "repoGroupMemberships",
  "repoList",
  "repoOpenEditor",
  "repoOpenFolder",
  "repoOpenHomepage",
  "repoOpenRemote",
  "repoOpenTerminal",
  "repoRefreshMetadata",
  "repoRemove",
  "repoScanParent",
  "repoSetCadence",
  "repoSetEnabled",
  "repoSetPolicy",
  "repoUpdateNow",
  "settingsGet",
  "settingsSet",
  "summaryToday",
] as const satisfies readonly (keyof Commands)[];

type Called = (typeof CALLED_COMMANDS)[number];

export type Handlers = {
  [K in Called]: (...args: Parameters<Commands[K]>) => Payload<K>;
};

/**
 * The wire argument names of each command, in positional order. The generated
 * bindings send `{ id, mode }`, not `[id, mode]`, so the dispatcher needs the
 * names to turn the wire object back into the positional call. `handlers.test.ts`
 * checks this table against the real `__TAURI_INVOKE` calls in `bindings.ts`.
 */
export const WIRE_ARGS: { [K in Called]: readonly string[] } = {
  activityList: ["filter"],
  appCheckForUpdate: [],
  appInstallUpdate: [],
  dbRecoveryNotice: [],
  diagnosticsGet: [],
  diagnosticsOpenLogDir: [],
  groupAssign: ["repoId", "groupId"],
  groupCreate: ["name", "color"],
  groupDelete: ["id"],
  groupList: [],
  groupUnassign: ["repoId", "groupId"],
  groupUpdate: ["id", "name", "color"],
  groupsForRepo: ["repoId"],
  repoAddPath: ["path"],
  repoCheckAll: [],
  repoCheckNow: ["id"],
  repoGet: ["id"],
  repoGroupMemberships: [],
  repoList: ["filter"],
  repoOpenEditor: ["id"],
  repoOpenFolder: ["id"],
  repoOpenHomepage: ["id"],
  repoOpenRemote: ["id"],
  repoOpenTerminal: ["id"],
  repoRefreshMetadata: ["id"],
  repoRemove: ["id"],
  repoScanParent: ["path"],
  repoSetCadence: ["id", "checkFrequencyMin"],
  repoSetEnabled: ["id", "enabled"],
  repoSetPolicy: ["id", "policy"],
  repoUpdateNow: ["id", "mode"],
  settingsGet: [],
  settingsSet: ["settings"],
  summaryToday: ["groupId"],
};

/** `repoCheckNow` to `repo_check_now`, the wire name the bindings invoke. */
export function toWireName(name: string): string {
  return name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`);
}

function appError(code: string, message: string, remediation = "", context: unknown = null): AppErrorPayload {
  return { code, message, remediation, context };
}

type MutableGroup = { id: number; name: string; color: string | null };

/**
 * The gallery's in-memory library for one scenario. Mutating commands change it,
 * so an action in the gallery (add a repo, assign a group, pause a repo) shows
 * its effect on the next read, the way the app does. `empty` starts with no
 * repositories and no groups, so the real empty states render.
 */
export function createWorld(scenario: Scenario) {
  const empty = scenario === "empty";
  const stress = scenario === "stress" || scenario === "allFailed";
  const failedAll = scenario === "allFailed";
  const sourceRepos = empty ? [] : failedAll ? ALL_FAILED_REPOS : stress ? STRESS_REPOS : REPOS;
  const sourceGroups = empty ? [] : stress ? STRESS_GROUPS : GROUPS;
  const sourceMembers = empty ? [] : stress ? STRESS_MEMBERSHIPS : MEMBERSHIPS;
  const sourceActivity = empty ? [] : failedAll ? ALL_FAILED_ACTIVITY : stress ? STRESS_ACTIVITY : ACTIVITY;
  const summary = failedAll
    ? ALL_FAILED_SUMMARY
    : stress
      ? STRESS_SUMMARY
      : { updated: SUMMARY_UPDATED, newReleases: SUMMARY_RELEASES, attention: SUMMARY_ATTENTION };
  const repos: RepoSummary[] = sourceRepos.map((r) => ({ ...r }));
  const groups: MutableGroup[] = sourceGroups.map((g) => ({ id: g.id, name: g.name, color: g.color }));
  const members = new Map<number, number[]>(sourceMembers.map((m) => [m.repoId, [...m.groupIds]]));
  const cadence = new Map<number, number>();
  let settings: Settings = { ...SETTINGS };
  const activity: ActivityRecord[] = sourceActivity.map((a) => ({ ...a }));
  return { repos, groups, members, cadence, activity, summary, getSettings: () => settings, setSettings: (s: Settings) => (settings = s) };
}

export type World = ReturnType<typeof createWorld>;

export function createHandlers(world: World): Handlers {
  const { repos, groups, members, cadence, activity } = world;
  const now = () => Math.floor(Date.now() / 1000);

  const findRepo = (id: number): RepoSummary => {
    const repo = repos.find((r) => r.id === id);
    if (!repo) throw appError("repo.not_found", `No repository with id ${id}.`, "Refresh the list.");
    return repo;
  };
  const detail = (id: number): RepoDetail => toDetail(findRepo(id), cadence.get(id) ?? 0);
  const inGroup = (repoId: number, groupId: number) => (members.get(repoId) ?? []).includes(groupId);
  const noop = (): null => null;

  function groupSummary(g: MutableGroup): GroupSummary {
    return {
      id: g.id,
      name: g.name,
      color: g.color,
      repoCount: repos.filter((r) => inGroup(r.id, g.id)).length,
    };
  }

  function checkOne(repo: RepoSummary) {
    repo.lastCheckedAt = now();
    const failed = repo.lastErrorCode !== null;
    return {
      repoId: repo.id,
      decision: failed ? "failed" : repo.isDirty ? "skipped_dirty" : "fetched",
      reason: failed ? "net.offline" : null,
      ahead: repo.aheadCount,
      behind: repo.behindCount,
      isDirty: repo.isDirty,
      isDetached: repo.isDetached,
      checkedAt: repo.lastCheckedAt,
      failed,
    };
  }

  return {
    repoList: (filter) => {
      const q = filter.query?.trim().toLowerCase() ?? "";
      return repos
        .filter((r) => filter.enabledOnly !== true || r.enabled)
        .filter((r) => filter.hostType === null || r.hostType === filter.hostType)
        .filter((r) => q === "" || r.localName.toLowerCase().includes(q) || r.localPath.toLowerCase().includes(q))
        .map((r) => ({ ...r }));
    },
    repoGet: (id) => detail(id),
    repoAddPath: (path) => {
      if (repos.some((r) => r.localPath === path)) {
        throw appError("repo.already_tracked", "That folder is already tracked.", "Choose a different folder.");
      }
      const id = repos.reduce((max, r) => Math.max(max, r.id), 0) + 1;
      const localName = path.split(/[\\/]/).filter(Boolean).pop() ?? path;
      repos.push({
        ...REPO_TEMPLATE,
        id,
        localName,
        localPath: path,
        remoteOriginUrl: null,
        hostType: "unknown",
        latestReleaseTag: null,
        openPrCount: null,
        stars: null,
        forks: null,
        license: null,
        visibility: null,
        homepage: null,
        lastCheckedAt: null,
      });
      return id;
    },
    repoScanParent: (path) => ({
      parentPath: path,
      discovered: SCAN_CANDIDATES.map((c) => ({ ...c, alreadyTracked: repos.some((r) => r.localPath === c.localPath) })),
    }),
    repoRemove: (id) => {
      findRepo(id);
      repos.splice(repos.findIndex((r) => r.id === id), 1);
      members.delete(id);
      return null;
    },
    repoSetEnabled: (id, enabled) => {
      findRepo(id).enabled = enabled;
      return null;
    },
    repoSetPolicy: (id, policy) => {
      findRepo(id).updateMode = policy.mode;
      return null;
    },
    repoSetCadence: (id, checkFrequencyMin) => {
      findRepo(id);
      cadence.set(id, checkFrequencyMin);
      return null;
    },
    repoCheckNow: (id) => checkOne(findRepo(id)),
    repoCheckAll: () => {
      const targets = repos.filter((r) => r.enabled);
      const results = targets.map(checkOne);
      const failedCheck = results.filter((r) => r.failed).length;
      return {
        targeted: targets.length,
        completed: results.length,
        succeeded: results.length - failedCheck,
        noResult: 0,
        failedCheck,
      };
    },
    repoUpdateNow: (id, mode) => {
      const repo = findRepo(id);
      const pulled = mode !== "check_only" && mode !== "fetch_only" && (repo.behindCount ?? 0) > 0;
      const result = {
        repoId: id,
        mode,
        outcome: pulled ? "updated" : "up_to_date",
        commitRange: null,
        ahead: repo.aheadCount,
        behind: pulled ? 0 : repo.behindCount,
        updatedAt: now(),
      };
      if (pulled) repo.behindCount = 0;
      return result;
    },
    repoRefreshMetadata: (id) => detail(id),
    repoOpenFolder: noop,
    repoOpenTerminal: noop,
    repoOpenEditor: noop,
    repoOpenRemote: noop,
    repoOpenHomepage: noop,

    activityList: (filter) =>
      activity
        .filter((a) => filter.repoId === null || a.repoId === filter.repoId)
        .filter((a) => filter.groupId === null || inGroup(a.repoId, filter.groupId))
        .filter((a) => filter.actionType === null || a.actionType === filter.actionType)
        .filter((a) => filter.status === null || a.status === filter.status)
        .sort((a, b) => b.timestamp - a.timestamp || b.id - a.id)
        .slice(0, filter.limit ?? undefined)
        .map((a) => ({ ...a })),

    summaryToday: (groupId) => {
      const scoped = (item: { repoId: number }) =>
        repos.some((r) => r.id === item.repoId) && (groupId === null || inGroup(item.repoId, groupId));
      const updated = world.summary.updated.filter(scoped);
      const newReleases = world.summary.newReleases.filter(scoped);
      const attention = world.summary.attention.filter(scoped);
      const touched = new Set([...updated, ...attention].map((i) => i.repoId));
      return {
        date: new Date(NOW * 1000).toISOString().slice(0, 10),
        updatedCount: updated.length,
        releasesCount: newReleases.length,
        attentionCount: attention.length,
        noChangeCount: repos.filter((r) => r.enabled && (groupId === null || inGroup(r.id, groupId)) && !touched.has(r.id)).length,
        updated,
        newReleases,
        attention,
      };
    },

    settingsGet: () => ({ ...world.getSettings() }),
    settingsSet: (settings) => {
      world.setSettings({ ...settings });
      return null;
    },
    dbRecoveryNotice: () => ({ ...DB_RECOVERY }),
    diagnosticsGet: () => ({ ...DIAGNOSTICS }),
    diagnosticsOpenLogDir: noop,
    appCheckForUpdate: () => ({ ...UPDATE_AVAILABILITY }),
    appInstallUpdate: noop,

    groupList: () => groups.map(groupSummary),
    groupCreate: (name, color) => {
      const trimmed = name.trim();
      if (groups.some((g) => g.name.toLowerCase() === trimmed.toLowerCase())) {
        throw appError("config.invalid_setting", "A group with that name already exists.", "Choose a different name.", { field: "name" });
      }
      const group = { id: groups.reduce((max, g) => Math.max(max, g.id), 0) + 1, name: trimmed, color };
      groups.push(group);
      return groupSummary(group);
    },
    groupUpdate: (id, name, color) => {
      const group = groups.find((g) => g.id === id);
      if (!group) throw appError("group.not_found", `No group with id ${id}.`);
      const trimmed = name.trim();
      if (groups.some((g) => g.id !== id && g.name.toLowerCase() === trimmed.toLowerCase())) {
        throw appError("config.invalid_setting", "A group with that name already exists.", "Choose a different name.", { field: "name" });
      }
      group.name = trimmed;
      group.color = color;
      return null;
    },
    groupDelete: (id) => {
      const index = groups.findIndex((g) => g.id === id);
      if (index === -1) throw appError("group.not_found", `No group with id ${id}.`);
      groups.splice(index, 1);
      for (const [repoId, ids] of members) members.set(repoId, ids.filter((g) => g !== id));
      return null;
    },
    groupAssign: (repoId, groupId) => {
      findRepo(repoId);
      const ids = members.get(repoId) ?? [];
      if (!ids.includes(groupId)) members.set(repoId, [...ids, groupId]);
      return null;
    },
    groupUnassign: (repoId, groupId) => {
      members.set(repoId, (members.get(repoId) ?? []).filter((g) => g !== groupId));
      return null;
    },
    groupsForRepo: (repoId) => [...(members.get(repoId) ?? [])],
    repoGroupMemberships: () =>
      repos.map((r) => ({ repoId: r.id, groupIds: [...(members.get(r.id) ?? [])] })).filter((m) => m.groupIds.length > 0),
  };
}

/**
 * Answer one wire-level IPC call. Rejects, naming the command, when there is no
 * handler: see guarantee 3 at the top of this file.
 */
export function dispatch(handlers: Handlers, wireName: string, args: Record<string, unknown> | undefined): unknown {
  const name = (CALLED_COMMANDS as readonly string[]).find((c) => toWireName(c) === wireName) as Called | undefined;
  if (name === undefined) {
    throw new Error(
      `[gallery] no stub for IPC command "${wireName}". Add it to CALLED_COMMANDS and WIRE_ARGS in src/gallery/handlers.ts.`,
    );
  }
  const handler = handlers[name] as (...positional: unknown[]) => unknown;
  return handler(...WIRE_ARGS[name].map((arg) => args?.[arg]));
}
