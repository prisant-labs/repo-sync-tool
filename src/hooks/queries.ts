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
