import { describe, expect, it } from "vitest";
import type { ActivityRecord } from "@/lib/bindings";
import {
  ACTIVITY_FETCH_LIMIT,
  ACTIVITY_PAGE_LIMIT,
  formatDuration,
  formatReceipt,
  paginate,
  toActivityFilter,
} from "@/lib/activity";

/**
 * The receipt string is what a user pastes into a bug report, so a field
 * silently dropping out of it is a defect nobody notices until an issue arrives
 * missing the one detail that would have explained the failure.
 *
 * `absoluteTime` is deliberately NOT tested here: it delegates to
 * `toLocaleString` with the runtime's own locale and timezone, so any assertion
 * would either pin the CI runner's environment or re-implement Intl.
 */

function record(overrides: Partial<ActivityRecord> = {}): ActivityRecord {
  return {
    id: 1,
    repoId: 7,
    timestamp: 1_754_300_000,
    actionType: "check",
    status: "ok",
    reasonCode: null,
    summary: null,
    commitRange: null,
    rawCommand: null,
    rawStdout: null,
    rawStderr: null,
    exitCode: null,
    durationMs: null,
    ...overrides,
  };
}

describe("formatReceipt", () => {
  it("leads with the action, status, and repo name", () => {
    const text = formatReceipt(record({ actionType: "update", status: "failed" }), "my-repo");
    expect(text.split("\n")[0]).toBe("update failed - my-repo");
  });

  /**
   * A repo removed after its activity rows were written has no name to look up.
   * The receipt has to stay readable rather than rendering "null" or an empty
   * gap where the name belongs - the audit trail outlives the repo on purpose.
   */
  it("says so plainly when the repo name is unknown", () => {
    expect(formatReceipt(record(), null)).toContain("- unknown repo");
  });

  /**
   * THE distinction this formatter exists to preserve. `null` means RepoSync
   * never captured that stream (a policy decision that skipped without running
   * git); `""` means git ran and printed nothing. Collapsing them would erase
   * exactly what someone reads a receipt to find out.
   */
  it("distinguishes a stream that was never captured from one that was empty", () => {
    const notCaptured = formatReceipt(record({ rawStdout: null }), "r");
    expect(notCaptured).toContain("stdout: (not captured)");

    const capturedEmpty = formatReceipt(record({ rawStdout: "" }), "r");
    expect(capturedEmpty).toContain("stdout:\n");
    expect(capturedEmpty).not.toContain("stdout: (not captured)");
  });

  it("puts each captured stream on its own line under a label", () => {
    const text = formatReceipt(
      record({
        rawCommand: "git fetch --all --prune",
        rawStdout: "",
        rawStderr: "fatal: could not read from remote repository",
      }),
      "r",
    );
    expect(text).toContain("command:\ngit fetch --all --prune");
    expect(text).toContain("stderr:\nfatal: could not read from remote repository");
  });

  /**
   * Absent optional fields are OMITTED rather than printed as "reason: null".
   * A receipt padded with nulls is harder to scan than a short one, and every
   * one of these is genuinely absent for some legitimate action type.
   */
  it("omits optional fields that are absent instead of printing null", () => {
    const text = formatReceipt(record(), "r");
    for (const label of ["summary:", "reason:", "commits:", "exit:", "duration:"]) {
      expect(text).not.toContain(label);
    }
  });

  it("includes optional fields that are present", () => {
    const text = formatReceipt(
      record({
        summary: "fast-forwarded 3 commits",
        reasonCode: "fetch_failed",
        commitRange: "aaa..bbb",
        exitCode: 128,
        durationMs: 412,
      }),
      "r",
    );
    expect(text).toContain("summary: fast-forwarded 3 commits");
    expect(text).toContain("reason: fetch_failed");
    expect(text).toContain("commits: aaa..bbb");
    expect(text).toContain("exit: 128");
    expect(text).toContain("duration: 412 ms");
  });

  /**
   * Exit code 0 is a real, meaningful value. A truthiness check would drop it,
   * turning "git ran and succeeded" into "we have no idea whether git ran".
   */
  it("keeps a zero exit code rather than treating it as absent", () => {
    expect(formatReceipt(record({ exitCode: 0 }), "r")).toContain("exit: 0");
    expect(formatReceipt(record({ durationMs: 0 }), "r")).toContain("duration: 0 ms");
  });
});

describe("formatDuration", () => {
  it("keeps sub-second work in milliseconds, the resolution a git operation is judged at", () => {
    expect(formatDuration(0)).toBe("0 ms");
    expect(formatDuration(412)).toBe("412 ms");
    expect(formatDuration(999)).toBe("999 ms");
  });

  it("switches to one decimal of seconds once the extra digits are noise", () => {
    expect(formatDuration(1000)).toBe("1.0 s");
    expect(formatDuration(1240)).toBe("1.2 s");
    expect(formatDuration(59_940)).toBe("59.9 s");
  });

  it("switches to minutes and zero-padded seconds past a minute", () => {
    expect(formatDuration(60_000)).toBe("1m 00s");
    expect(formatDuration(63_000)).toBe("1m 03s");
    expect(formatDuration(3_600_000)).toBe("60m 00s");
  });

  it("carries rather than printing 60 seconds", () => {
    // 119_600 ms rounds to 60 seconds inside minute 1, which would render
    // "1m 60s" - a value no clock shows and that reads as a bug.
    expect(formatDuration(119_600)).toBe("2m 00s");
  });

  it("returns null for an absent duration, never a zero", () => {
    // A row written before durations were recorded, or an operation that
    // never completed, has no duration. "0 ms" is a different claim: it says
    // the work took no time. The table renders null as its muted dash.
    expect(formatDuration(null)).toBeNull();
  });
});

/**
 * `toActivityFilter` maps the two chip selections plus the shell's engaged
 * group onto the wire filter, and the one rule that matters is that "all"
 * becomes `null` rather than the string "all".
 *
 * The backend treats a null field as "no constraint" and applies a literal
 * equality comparison otherwise. Sending "all" would therefore ask for rows
 * whose `action_type` is the string "all", of which there are none, and the
 * screen would render the empty state. That failure is nasty precisely because
 * it is not loud: an empty Activity screen is indistinguishable from a fresh
 * install, so the bug reads as "the filter works, I just have no activity".
 */
describe("toActivityFilter", () => {
  it("maps the unfiltered selection to all-null, not to the string 'all'", () => {
    expect(toActivityFilter("all", "all", null)).toEqual({
      repoId: null,
      groupId: null,
      actionType: null,
      status: null,
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("passes a concrete action type through and leaves status unconstrained", () => {
    expect(toActivityFilter("update", "all", null)).toEqual({
      repoId: null,
      groupId: null,
      actionType: "update",
      status: null,
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("passes a concrete status through and leaves action type unconstrained", () => {
    expect(toActivityFilter("all", "failed", null)).toEqual({
      repoId: null,
      groupId: null,
      actionType: null,
      status: "failed",
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("combines both axes independently", () => {
    expect(toActivityFilter("check", "success", null)).toEqual({
      repoId: null,
      groupId: null,
      actionType: "check",
      status: "success",
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  // A2: the sidebar marks the engaged group on the Activity screen too, so
  // the screen has to actually honour it - a mark over an unfiltered list is
  // the same class of lie as a count derived from a capped page. The backend
  // resolves membership server-side, before its own LIMIT (BL-NI-93, closed),
  // so the scope goes on the wire rather than narrowing the fetched page.
  it("carries the engaged group onto the wire, alongside the two chip axes", () => {
    expect(toActivityFilter("all", "all", 7)).toEqual({
      repoId: null,
      groupId: 7,
      actionType: null,
      status: null,
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("leaves the group unconstrained when no group is engaged", () => {
    expect(toActivityFilter("check", "failed", null).groupId).toBeNull();
  });

  it("requests one MORE row than it displays, so truncation is knowable", () => {
    // The core's own default is 200 and its ceiling is 1000, so an explicit limit
    // is always sent rather than letting the backend default apply silently. The
    // +1 is the sentinel: a response capped at N cannot distinguish "exactly N
    // matches" from "far more than N", so the screen asks for N+1 and treats the
    // extra row's arrival as the evidence that older entries exist.
    expect(toActivityFilter("all", "all", null).limit).toBe(ACTIVITY_FETCH_LIMIT);
    expect(ACTIVITY_FETCH_LIMIT).toBe(ACTIVITY_PAGE_LIMIT + 1);
    expect(ACTIVITY_PAGE_LIMIT).toBeGreaterThan(0);
  });

  it("never invents a scope: repoId stays null, and groupId is only ever what the caller passed", () => {
    // The original guard here said "never scopes to a repo OR a group, since
    // no control sets either yet". Half of that expired: A2 gave the group a
    // control (the sidebar's engaged group, marked on this screen too) and a
    // label (the empty state names it), so a group scope is now visible
    // rather than silent, which is the property that guard existed to
    // protect. The repo half has neither and still holds.
    //
    // What replaces it is the same protection restated: this function may
    // pass a scope through, never originate one. A silently scoped audit
    // trail is worse than an unscoped one - the bug class BL-NI-93's backend
    // fix exists to make honest, not to reintroduce on the frontend side.
    for (const a of ["all", "check", "update"] as const) {
      for (const s of ["all", "success", "failed"] as const) {
        expect(toActivityFilter(a, s, null).repoId).toBeNull();
        expect(toActivityFilter(a, s, 4).repoId).toBeNull();
        expect(toActivityFilter(a, s, null).groupId).toBeNull();
        expect(toActivityFilter(a, s, 4).groupId).toBe(4);
      }
    }
  });
});

/**
 * `paginate` decides two things a rendered list makes awkward to check: which
 * rows to show, and whether to claim older entries exist.
 *
 * The boundary is the whole point. An earlier version of the screen asked for 60
 * rows and showed the truncation notice when it got 60 back. That test can never
 * be right: the request is capped, so a response cannot exceed the limit, and
 * "we received exactly 60" is equally consistent with "there are exactly 60" and
 * "there are ten thousand". At exactly 60 the notice asserted the existence of
 * older entries on no evidence, which is the same unfounded-confidence problem it
 * was added to fix. Asking for 61 and testing for the extra row makes the claim
 * something the code actually knows.
 */
describe("paginate", () => {
  const rows = (n: number) => Array.from({ length: n }, (_, i) => i);

  it("shows everything and claims no more when under the display limit", () => {
    const { visible, hasMore } = paginate(rows(ACTIVITY_PAGE_LIMIT - 1));
    expect(visible).toHaveLength(ACTIVITY_PAGE_LIMIT - 1);
    expect(hasMore).toBe(false);
  });

  it("shows everything and claims no more at EXACTLY the display limit", () => {
    // The case the old length-based check got wrong.
    const { visible, hasMore } = paginate(rows(ACTIVITY_PAGE_LIMIT));
    expect(visible).toHaveLength(ACTIVITY_PAGE_LIMIT);
    expect(hasMore).toBe(false);
  });

  it("drops the sentinel row and claims more when it arrives", () => {
    const { visible, hasMore } = paginate(rows(ACTIVITY_FETCH_LIMIT));
    expect(visible).toHaveLength(ACTIVITY_PAGE_LIMIT);
    expect(hasMore).toBe(true);
  });

  it("never renders the sentinel row itself", () => {
    // The extra row was requested to answer a question, not to be read. Showing
    // it would make the list one longer than the notice says it is.
    const { visible } = paginate(rows(ACTIVITY_FETCH_LIMIT));
    expect(visible.at(-1)).toBe(ACTIVITY_PAGE_LIMIT - 1);
  });

  it("handles an empty page without claiming more", () => {
    expect(paginate([])).toEqual({ visible: [], hasMore: false });
  });
});
