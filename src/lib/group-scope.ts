/**
 * One authority for "does this repo fall inside the engaged group", shared by
 * every surface that reports a scoped number.
 *
 * This used to live inline in `dashboard.tsx` as a `useCallback` plus two
 * `useMemo`s. SB3 and SB4 put a second consumer in the sidebar - an attention
 * dot on Dashboard and a repo count on Repos - and a second copy of this rule
 * is how the dot ends up claiming attention that the Dashboard it points at
 * reports as All clear. Two numbers derived from one fact must be derived from
 * one function.
 *
 * `null` is the load-bearing return value. A group can be engaged while the
 * bulk membership read is still in flight, and every scoped number must WAIT
 * for it rather than render a fabricated zero - a `0` that later becomes a `7`
 * is worse than a blank, because a blank does not claim anything.
 */

/** The membership shape `useRepoGroupMemberships` resolves to. */
export type MembershipMap = Map<number, number[]>;

export type GroupScope = {
  /**
   * A group is engaged but its membership is not known yet. Callers render
   * nothing at all while this is true; the counters below already return
   * `null`, this flag is for the non-numeric case (an attention DOT, which is
   * a boolean, not a count).
   */
  pending: boolean;
  /** Whether this repo is in scope. Always true when no group is engaged. */
  includes: (repoId: number) => boolean;
  /**
   * How many of these repos are in scope, or `null` if that cannot be known
   * yet. Pass the raw `useRepoList` data, `null` included.
   */
  countRepos: (repos: readonly { id: number }[] | null) => number | null;
  /**
   * How many of these summary items are in scope, or `null` if that cannot be
   * known yet. Pass the raw `DailySummary` list, `null` included.
   */
  countItems: (items: readonly { repoId: number }[] | null) => number | null;
};

export function groupScope(
  activeGroupId: number | null,
  membershipMap: MembershipMap | null,
): GroupScope {
  const pending = activeGroupId !== null && membershipMap === null;
  const includes = (repoId: number) =>
    activeGroupId === null || (membershipMap?.get(repoId)?.includes(activeGroupId) ?? false);

  return {
    pending,
    includes,
    countRepos: (repos) => {
      if (repos === null || pending) return null;
      return activeGroupId === null ? repos.length : repos.filter((r) => includes(r.id)).length;
    },
    countItems: (items) => {
      if (items === null || pending) return null;
      return activeGroupId === null
        ? items.length
        : items.filter((it) => includes(it.repoId)).length;
    },
  };
}
