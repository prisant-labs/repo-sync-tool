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
 * The one-time database-recovery notice (E-02 AC7 / BL-NI-33). Read once at
 * launch; `data.recovered` is true only when the startup migration failed and the
 * previous database was moved aside, in which case `data.backupPath` names where
 * it was preserved. The app shell surfaces this as a dismissible banner.
 */
export function useDbRecoveryNotice() {
  return useAsync(() => unwrap(commands.dbRecoveryNotice()), []);
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
 * Group memberships for every repo, as a `Map<repoId, groupId[]>`, in ONE IPC
 * call (`repo_group_memberships`) instead of fanning `groups_for_repo` out per
 * repo (BL-NI-22, was O(N) round-trips).
 *
 * The bulk read returns one entry per repo that belongs to at least one group, so
 * a repo with no memberships is simply ABSENT from the map. Every consumer reads
 * through `?.get(id)` / `?? []` (see `screens/repos.tsx`), so an absent repo reads
 * as "no groups", identical to the old per-repo empty array. `data` is still
 * `Map | null` where `null` means loading-or-error, preserving the Repos screen's
 * AsyncPanel loading/error presentation.
 */
export function useRepoGroupMemberships() {
  return useAsync(async () => {
    const rows = await unwrap(commands.repoGroupMemberships());
    return new Map<number, number[]>(rows.map((r) => [r.repoId, r.groupIds]));
  }, []);
}

/**
 * Call `onChange` when the backend broadcasts a state-affecting event, for the
 * AGGREGATE screens (dashboard, repos list) that refetch a whole-library view.
 * This is how those screens stay live without polling. Pass a stable callback
 * (e.g. a `refetch` from `useAsync`).
 *
 * Coalescing (finding 3): a scheduled cycle emits ONE `scheduler:tick` after all
 * its per-repo jobs have joined, PLUS one `repo:state-changed` per completed repo.
 * Refetching on both fanned an N-repo cycle into N+1 aggregate refetches. So the
 * aggregate refetch uses `scheduler:tick` as the single per-cycle batch trigger and
 * deliberately does NOT subscribe to `repo:state-changed` (that per-repo event is
 * only for the focused repo-detail drawer, `useRepoBackendEvents`, correctly scoped
 * to one repo id - finding 11). A zero-work tick (`checked === 0`: nothing was due)
 * carries no state change and is ignored. `repo:check-completed` / `-update-completed`
 * stay subscribed because those fire only on MANUAL, user-initiated single actions,
 * so refetching immediately keeps the screen responsive without any per-cycle storm.
 *
 * Background GitHub metadata (E-17 finding 3): the background PR/release refresh pass
 * is NOT a git check, so it does not ride `scheduler:tick`. Instead it emits ONE
 * `repo:metadata-refreshed` per pass that changed at least one repo, subscribed to
 * here so the list picks up fresher PR/release badges - still exactly one refetch per
 * pass, never a per-repo storm (the pass's per-repo `repo:state-changed` events go only
 * to the scoped drawer hook, as above).
 */
export function useBackendEvents(onChange: () => void) {
  useEffect(() => {
    const subscriptions = [
      events.repoCheckCompleted.listen(() => onChange()),
      events.repoUpdateCompleted.listen(() => onChange()),
      events.schedulerTick.listen((e) => {
        if (e.payload.checked > 0) onChange();
      }),
      events.repoMetadataRefreshed.listen(() => onChange()),
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

/**
 * Like `useBackendEvents`, but scoped to one repo: only calls `onChange` when
 * a check/update/state event's payload concerns this repo id. Used by the
 * open repo-detail drawer so it stays live when a background scheduled check
 * completes for its repo (finding 11 / BL-NI-28), without refetching on every
 * OTHER repo's event too (no refetch storm across a whole scheduler pass, and
 * no need to also refetch group membership here: nothing about a check or
 * update changes it, so that stays scoped to the drawer's own toggle action).
 */
export function useRepoBackendEvents(repoId: number, onChange: () => void) {
  useEffect(() => {
    const subscriptions = [
      events.repoCheckCompleted.listen((e) => {
        if (e.payload.repoId === repoId) onChange();
      }),
      events.repoUpdateCompleted.listen((e) => {
        if (e.payload.repoId === repoId) onChange();
      }),
      events.repoStateChanged.listen((e) => {
        if (e.payload.repoId === repoId) onChange();
      }),
    ];
    return () => {
      void Promise.all(subscriptions).then((unlisteners) => {
        for (const off of unlisteners) off();
      });
    };
    // onChange is expected to be referentially stable; see useBackendEvents.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [repoId]);
}
