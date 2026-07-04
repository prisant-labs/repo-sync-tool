import { useEffect } from "react";
import { commands, events } from "@/lib/bindings";
import type { ActivityFilter, RepoFilter } from "@/lib/bindings";
import { unwrap } from "@/lib/ipc";
import { useAsync } from "@/hooks/use-async";

/** Live list of tracked repos (summary view), re-run when the filter changes. */
export function useRepoList(filter: RepoFilter) {
  return useAsync(
    () => unwrap(commands.repoList(filter)),
    [filter.enabledOnly, filter.hostType, filter.query],
  );
}

/** Full detail for one repo. Resolves to null while no repo is selected. */
export function useRepoDetail(id: number | null) {
  return useAsync(() => (id === null ? Promise.resolve(null) : unwrap(commands.repoGet(id))), [id]);
}

/** Today's daily summary (local-day roll-up). */
export function useSummaryToday() {
  return useAsync(() => unwrap(commands.summaryToday()), []);
}

/** Activity-log rows (newest first), re-run when the filter changes. */
export function useActivity(filter: ActivityFilter) {
  return useAsync(
    () => unwrap(commands.activityList(filter)),
    [filter.repoId, filter.actionType, filter.status, filter.limit],
  );
}

/** The settings singleton. */
export function useSettings() {
  return useAsync(() => unwrap(commands.settingsGet()), []);
}

/** Live list of repo groups (tags) with member counts, for the sidebar + management. */
export function useGroups() {
  return useAsync(() => unwrap(commands.groupList()), []);
}

/** The ids of the groups one repo belongs to. A null repo id resolves to an empty list. */
export function useGroupsForRepo(repoId: number | null) {
  return useAsync(
    () => (repoId === null ? Promise.resolve<number[]>([]) : unwrap(commands.groupsForRepo(repoId))),
    [repoId],
  );
}

/**
 * Group memberships for many repos at once, as a `Map<repoId, groupId[]>`.
 *
 * This fans out one `groups_for_repo` call per repo (O(N) IPC round-trips),
 * which is fine for V1 repo counts. A dedicated `repos_in_group` query would
 * collapse the fan-out into a single call and is the natural future
 * optimization once repo counts grow.
 */
export function useRepoGroupMemberships(repoIds: number[]) {
  const key = repoIds.join(",");
  return useAsync(async () => {
    const lists = await Promise.all(repoIds.map((id) => unwrap(commands.groupsForRepo(id))));
    const map = new Map<number, number[]>();
    repoIds.forEach((id, i) => map.set(id, lists[i]));
    return map;
    // Re-run only when the set of repo ids changes (keyed by their join above).
  }, [key]);
}

/**
 * Call `onChange` whenever the backend broadcasts a state-affecting event
 * (a check or update finished, a repo's cached state changed, or the scheduler
 * ticked). This is how screens stay live without polling. Pass a stable
 * callback (e.g. a `refetch` from `useAsync`).
 */
export function useBackendEvents(onChange: () => void) {
  useEffect(() => {
    const subscriptions = [
      events.repoCheckCompleted.listen(() => onChange()),
      events.repoUpdateCompleted.listen(() => onChange()),
      events.repoStateChanged.listen(() => onChange()),
      events.schedulerTick.listen(() => onChange()),
    ];
    return () => {
      void Promise.all(subscriptions).then((unlisteners) => {
        for (const off of unlisteners) off();
      });
    };
    // onChange is expected to be referentially stable; see doc comment.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);
}
