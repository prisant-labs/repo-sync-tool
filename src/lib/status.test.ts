import { afterEach, describe, expect, it, vi } from "vitest";
import {
  checkFailureMessage,
  deriveStatus,
  lagLabel,
  lagMagnitude,
  relativeTime,
} from "@/lib/status";

/**
 * The status taxonomy is a FRONTEND policy decision: the wire type carries only
 * raw facts and no `status` field, so `deriveStatus` is where "what is wrong with
 * this repo" is actually decided. Nothing in the Rust suite can cover it.
 *
 * It also has form. The 2026-07-04 audit found the dashboard collapsing every
 * attention row to a failure icon (BL-NI-27), which is exactly a
 * status-derivation bug, and it shipped because this logic had no tests.
 */

/** Build the fact set `deriveStatus` reads, defaulting to a healthy repo. */
function facts(overrides: Partial<Parameters<typeof deriveStatus>[0]> = {}) {
  return {
    isDirty: false,
    enabled: true,
    autoPaused: false,
    lastErrorCode: null,
    aheadCount: 0,
    behindCount: 0,
    // A healthy repo tracks something. Defaulting this to `null` would make
    // every existing test in this file exercise the not-yet-observed path
    // instead of the healthy one, which is a different assertion than the one
    // they were written to make.
    upstreamState: "tracking" as const,
    ...overrides,
  };
}

describe("deriveStatus", () => {
  it("reports a clean, current, enabled repo as in sync", () => {
    expect(deriveStatus(facts())).toBe("sync");
  });

  it("maps each single condition to its own state", () => {
    expect(deriveStatus(facts({ enabled: false }))).toBe("paused");
    expect(deriveStatus(facts({ autoPaused: true }))).toBe("paused");
    expect(deriveStatus(facts({ lastErrorCode: "fetch_failed" }))).toBe("failed");
    expect(deriveStatus(facts({ isDirty: true }))).toBe("dirty");
    expect(deriveStatus(facts({ behindCount: 3 }))).toBe("behind");
    expect(deriveStatus(facts({ aheadCount: 2 }))).toBe("ahead");
  });

  /**
   * The part worth pinning. Real repos are usually in several states at once, and
   * the whole point of the ranking is deciding which one the user sees. Each case
   * asserts the LOWER-priority condition is present and still loses.
   */
  it("applies the priority order paused > failed > dirty > behind > ahead", () => {
    expect(
      deriveStatus(facts({ autoPaused: true, lastErrorCode: "fetch_failed", isDirty: true, behindCount: 9 })),
    ).toBe("paused");

    expect(deriveStatus(facts({ lastErrorCode: "fetch_failed", isDirty: true, behindCount: 9 }))).toBe(
      "failed",
    );

    expect(deriveStatus(facts({ isDirty: true, behindCount: 9, aheadCount: 4 }))).toBe("dirty");

    expect(deriveStatus(facts({ behindCount: 9, aheadCount: 4 }))).toBe("behind");
  });

  /**
   * The BL-NI-27 shape, stated as a rule rather than a UI assertion: a repo that
   * merely needs attention must NOT read as failed. "Behind" is a normal, expected
   * state; "failed" means something went wrong. Collapsing the two is what made
   * every attention row look like an error.
   */
  it("does not report a merely-behind or dirty repo as failed", () => {
    expect(deriveStatus(facts({ behindCount: 42 }))).not.toBe("failed");
    expect(deriveStatus(facts({ isDirty: true }))).not.toBe("failed");
    expect(deriveStatus(facts({ isDirty: true, behindCount: 42 }))).not.toBe("failed");
  });

  /**
   * The counts are nullable on the wire (a repo with no upstream has never had
   * them computed). Null must read as "no lag", not as truthy.
   */
  it("treats null ahead and behind counts as zero, not as lag", () => {
    expect(deriveStatus(facts({ aheadCount: null, behindCount: null }))).toBe("sync");
  });

  it("treats a disabled repo as paused even when it is also behind and dirty", () => {
    expect(deriveStatus(facts({ enabled: false, isDirty: true, behindCount: 5 }))).toBe("paused");
  });

  /**
   * BL-NI-77. A repo whose upstream branch was deleted reported "In sync",
   * because `deriveStatus` had no state for it and fell through to the last
   * branch. Observed live: one repo recorded that outcome 39 times over ten
   * days, every one of them green, while its badge said it was fine.
   *
   * The engine had always known (`policy::UpstreamState` is a three-state enum);
   * the wire type carried nothing, so the UI defaulted to the reassuring answer.
   */
  it("does not call a repo in sync when its upstream was deleted", () => {
    expect(deriveStatus(facts({ upstreamState: "deleted" }))).toBe("noUpstream");
  });

  it("treats a branch that never had an upstream the same way", () => {
    // Different cause, same consequence: there is nothing to sync against. The
    // two are kept distinct in the DATA (the engine and the column both carry
    // three states) and deliberately collapsed only here, at the badge, where
    // the user's question is "can this sync" rather than "why not".
    expect(deriveStatus(facts({ upstreamState: "none" }))).toBe("noUpstream");
  });

  it("leaves a tracking repo alone", () => {
    expect(deriveStatus(facts({ upstreamState: "tracking" }))).toBe("sync");
  });

  it("ranks a dirty tree above a missing upstream", () => {
    // Both are true and only one can be shown. Dirty wins because it is the one
    // the user can act on immediately.
    expect(deriveStatus(facts({ upstreamState: "deleted", isDirty: true }))).toBe("dirty");
  });

  it("ranks a missing upstream above behind and ahead", () => {
    // Contrived: with no upstream the backend reports null counts, so these
    // cannot really co-occur. Pinned anyway, because the ranking is what stops a
    // stale count from out-voting the fact that there is nothing to count against.
    expect(deriveStatus(facts({ upstreamState: "deleted", behindCount: 9 }))).toBe("noUpstream");
    expect(deriveStatus(facts({ upstreamState: "deleted", aheadCount: 9 }))).toBe("noUpstream");
  });

  /**
   * `null` is NOT YET OBSERVED, not a state. It falls through to the
   * pre-BL-NI-77 derivation, which on a clean repo lands on "sync".
   *
   * That is the reassuring answer, and it is chosen knowingly rather than by
   * accident: the alternative badges every repo in the library as broken the
   * moment migration 0008 lands and before anything has been re-checked. The
   * window is one check per repo; a wrong badge on the whole library is not.
   */
  it("does not invent a broken upstream from an unobserved one", () => {
    expect(deriveStatus(facts({ upstreamState: null }))).toBe("sync");
    expect(deriveStatus(facts({ upstreamState: null, behindCount: 3 }))).toBe("behind");
  });
});

describe("lagLabel", () => {
  it("names the actual count for behind and ahead", () => {
    expect(lagLabel(facts({ behindCount: 7 }))).toBe("7 behind");
    expect(lagLabel(facts({ aheadCount: 2 }))).toBe("2 ahead, clean");
  });

  it("explains WHY a repo is not being updated, not just that it is not", () => {
    // These strings are the user's only explanation for inaction, so they are
    // part of the contract, not decoration.
    expect(lagLabel(facts({ isDirty: true }))).toBe("uncommitted, skipped");
    expect(lagLabel(facts({ enabled: false }))).toBe("watching paused");
    expect(lagLabel(facts({ lastErrorCode: "fetch_failed" }))).toBe("check failed");
  });

  it("says current for a healthy repo", () => {
    expect(lagLabel(facts())).toBe("current");
  });

  it("renders a null behind count as 0 rather than 'null behind'", () => {
    // Reachable only if the ranking changes; asserted so a future re-rank cannot
    // leak the word "null" into the UI.
    expect(lagLabel(facts({ behindCount: 5 }))).not.toContain("null");
  });
});

describe("lagMagnitude", () => {
  it("stays within 0..1 so the bar cannot overflow its track", () => {
    for (const behindCount of [0, 1, 25, 50, 500, 100_000]) {
      const m = lagMagnitude(facts({ behindCount }));
      expect(m).toBeGreaterThanOrEqual(0);
      expect(m).toBeLessThanOrEqual(1);
    }
  });

  it("saturates at 50 commits behind instead of growing without bound", () => {
    expect(lagMagnitude(facts({ behindCount: 50 }))).toBe(1);
    expect(lagMagnitude(facts({ behindCount: 5_000 }))).toBe(1);
  });

  it("grows with the behind count below saturation", () => {
    expect(lagMagnitude(facts({ behindCount: 5 }))).toBeLessThan(
      lagMagnitude(facts({ behindCount: 25 })),
    );
  });
});

describe("relativeTime", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  /** Pin the clock; these assertions are otherwise time-dependent and flaky. */
  function atFixedNow(epochSeconds: number) {
    vi.useFakeTimers();
    vi.setSystemTime(epochSeconds * 1000);
  }

  it("reports never for an absent timestamp", () => {
    expect(relativeTime(null)).toBe("never");
  });

  it("labels each bucket", () => {
    const now = 1_700_000_000;
    atFixedNow(now);
    expect(relativeTime(now - 10)).toBe("just now");
    expect(relativeTime(now - 60 * 5)).toBe("5m ago");
    expect(relativeTime(now - 3600 * 3)).toBe("3h ago");
    expect(relativeTime(now - 86_400 * 4)).toBe("4d ago");
  });

  /**
   * Clock skew is real: a machine whose clock steps backwards, or a backend
   * timestamp a second ahead, would otherwise produce a negative delta and a
   * label like "-1m ago". The implementation clamps at zero.
   */
  it("does not render a future timestamp as negative time", () => {
    const now = 1_700_000_000;
    atFixedNow(now);
    expect(relativeTime(now + 600)).toBe("just now");
  });
});

/**
 * A failed check now RESOLVES rather than rejecting (BL-NI-04), which moves the
 * decision of what to tell the user out of the error path and into the screen.
 * `checkFailureMessage` is that decision, and it is tested here for the same
 * reason `deriveStatus` is: it is a policy call the backend deliberately does not
 * make.
 */
describe("checkFailureMessage", () => {
  it("names the credential problem for an auth failure", () => {
    const m = checkFailureMessage("git.auth_failed");
    expect(m).toContain("Authentication failed");
    expect(m).toContain("Activity");
  });

  it("names the network for an offline failure", () => {
    const m = checkFailureMessage("net.offline");
    expect(m).toContain("Could not reach the remote");
    expect(m).toContain("Activity");
  });

  it("falls back for the generic fetch failure", () => {
    expect(checkFailureMessage("git.fetch_failed")).toContain("The fetch failed");
  });

  it("treats a null reason and an unknown code the same, and does not invent a cause", () => {
    // A code the frontend has not been taught is still a failure. Guessing at a
    // specific explanation would be worse than the generic one, because the
    // receipt has the truth either way and a wrong guess sends the user after the
    // wrong problem.
    const unknown = checkFailureMessage("git.something_new");
    expect(unknown).toBe(checkFailureMessage(null));
    expect(unknown).not.toContain("Authentication");
    expect(unknown).not.toContain("network");
  });

  it("always points at Activity, whatever the reason", () => {
    // Every branch has to end somewhere the user can actually go. "Fetch failed"
    // is not a next step; "the output is in Activity" is, and the receipt drawer
    // that makes it true already exists.
    for (const reason of ["git.auth_failed", "net.offline", "git.fetch_failed", null]) {
      expect(checkFailureMessage(reason)).toContain("Activity");
    }
  });
});
