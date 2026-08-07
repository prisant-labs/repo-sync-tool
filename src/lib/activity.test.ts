import { describe, expect, it } from "vitest";
import type { ActivityRecord } from "@/lib/bindings";
import {
  ACTIVITY_FETCH_LIMIT,
  ACTIVITY_PAGE_LIMIT,
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

/**
 * `toActivityFilter` maps two chip selections onto the wire filter, and the one
 * rule that matters is that "all" becomes `null` rather than the string "all".
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
    expect(toActivityFilter("all", "all")).toEqual({
      repoId: null,
      actionType: null,
      status: null,
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("passes a concrete action type through and leaves status unconstrained", () => {
    expect(toActivityFilter("update", "all")).toEqual({
      repoId: null,
      actionType: "update",
      status: null,
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("passes a concrete status through and leaves action type unconstrained", () => {
    expect(toActivityFilter("all", "failed")).toEqual({
      repoId: null,
      actionType: null,
      status: "failed",
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("combines both axes independently", () => {
    expect(toActivityFilter("check", "success")).toEqual({
      repoId: null,
      actionType: "check",
      status: "success",
      limit: ACTIVITY_FETCH_LIMIT,
    });
  });

  it("requests one MORE row than it displays, so truncation is knowable", () => {
    // The core's own default is 200 and its ceiling is 1000, so an explicit limit
    // is always sent rather than letting the backend default apply silently. The
    // +1 is the sentinel: a response capped at N cannot distinguish "exactly N
    // matches" from "far more than N", so the screen asks for N+1 and treats the
    // extra row's arrival as the evidence that older entries exist.
    expect(toActivityFilter("all", "all").limit).toBe(ACTIVITY_FETCH_LIMIT);
    expect(ACTIVITY_FETCH_LIMIT).toBe(ACTIVITY_PAGE_LIMIT + 1);
    expect(ACTIVITY_PAGE_LIMIT).toBeGreaterThan(0);
  });

  it("never scopes to a repo, since no control sets one yet", () => {
    // Guards against a future edit wiring repoId here without also adding the
    // control and the label that say the view is scoped. A silently repo-scoped
    // audit trail is worse than an unscoped one.
    for (const a of ["all", "check", "update"] as const) {
      for (const s of ["all", "success", "failed"] as const) {
        expect(toActivityFilter(a, s).repoId).toBeNull();
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
