import { describe, expect, it } from "vitest";
import type { ActivityRecord } from "@/lib/bindings";
import { formatReceipt } from "@/lib/activity";

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
