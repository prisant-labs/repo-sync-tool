import { describe, expect, it } from "vitest";
import { groupScope, type MembershipMap } from "@/lib/group-scope";

/**
 * The reason this is one function with its own tests rather than a helper
 * copied into two components: SB3's attention dot sits in the sidebar and
 * points at the Dashboard, which computes its own scoped attention count.
 * If the two ever disagree, the dot says "something needs you" over a screen
 * that says "All clear" - and the user cannot tell which one is lying.
 */

const MEMBERSHIPS: MembershipMap = new Map([
  [1, [10]], // in group 10
  [2, [11]], // in group 11
  [3, [10, 11]], // in both
  [4, []], // in no group
]);

const REPOS = [{ id: 1 }, { id: 2 }, { id: 3 }, { id: 4 }];

describe("groupScope", () => {
  it("includes every repo and counts everything when no group is engaged", () => {
    const s = groupScope(null, MEMBERSHIPS);

    expect(s.pending).toBe(false);
    expect(REPOS.every((r) => s.includes(r.id))).toBe(true);
    expect(s.countRepos(REPOS)).toBe(4);
  });

  it("counts only the engaged group's members", () => {
    const s = groupScope(10, MEMBERSHIPS);

    expect(s.includes(1)).toBe(true);
    expect(s.includes(2)).toBe(false);
    expect(s.includes(3)).toBe(true);
    expect(s.countRepos(REPOS)).toBe(2);
  });

  it("treats a repo with no membership row at all as outside every group", () => {
    // A repo added since the bulk read, or one belonging to nothing. Absent
    // must mean "not in the group" rather than throwing or defaulting to in -
    // the latter would inflate every scoped count on a stale map.
    const s = groupScope(10, MEMBERSHIPS);

    expect(s.includes(4)).toBe(false);
    expect(s.includes(999)).toBe(false);
  });

  it("returns null, never zero, while a group is engaged and membership is unknown", () => {
    // The defect this guards: a fabricated 0 that becomes a 7 a moment later.
    // A caller rendering `null` shows nothing, which claims nothing.
    const s = groupScope(10, null);

    expect(s.pending).toBe(true);
    expect(s.countRepos(REPOS)).toBeNull();
  });

  it("is NOT pending when no group is engaged, even with no membership map", () => {
    // Nothing needs resolving to count an unscoped list, so the sidebar must
    // not sit blank on a fresh load just because memberships have not arrived.
    const s = groupScope(null, null);

    expect(s.pending).toBe(false);
    expect(s.countRepos(REPOS)).toBe(4);
  });

  it("returns null for data that has not loaded, scoped or not", () => {
    expect(groupScope(null, MEMBERSHIPS).countRepos(null)).toBeNull();
  });
});
