// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import type { ActivityRecord } from "@/lib/bindings";
import { ActivityReceipt } from "@/components/activity-receipt";

/**
 * The distinction these tests exist to protect: a `null` stream and an empty
 * stream mean different things, and the receipt is the one surface where the
 * difference is the answer someone came for.
 *
 * `null` means RepoSync never captured that stream for this action - a policy
 * decision that skipped without ever running git. `""` means git ran and printed
 * nothing. "Did it run and stay quiet, or did it never run" is precisely the
 * question an audit trail is opened to settle, and a single "no output" string
 * erases it.
 *
 * These assertions are about rendered MEANING, not markup, so restyling the
 * drawer should leave them untouched.
 */

afterEach(cleanup);

const RECORD: ActivityRecord = {
  id: 1,
  repoId: 7,
  timestamp: 1_700_000_000,
  actionType: "update",
  status: "success",
  reasonCode: null,
  summary: null,
  commitRange: null,
  rawCommand: "git -C C:\\repos\\example fetch --prune",
  rawStdout: "",
  rawStderr: "",
  exitCode: 0,
  durationMs: 412,
};

function renderWith(overrides: Partial<ActivityRecord>, repoName: string | null = "example") {
  return render(
    <ActivityReceipt
      record={{ ...RECORD, ...overrides }}
      repoName={repoName}
      onClose={vi.fn()}
    />,
  );
}

describe("ActivityReceipt stream rendering", () => {
  it("says a null stream was never captured", () => {
    renderWith({ rawStdout: null });

    expect(screen.getAllByText(/not captured for this action/i).length).toBeGreaterThan(0);
  });

  it("says an empty stream ran and printed nothing", () => {
    renderWith({ rawStdout: "" });

    expect(screen.getAllByText(/empty \(git printed nothing\)/i).length).toBeGreaterThan(0);
  });

  it("never uses the same words for a null stream and an empty one", () => {
    renderWith({ rawCommand: null, rawStdout: "", rawStderr: null });

    const neverCaptured = screen.getAllByText(/not captured for this action/i);
    const ranAndQuiet = screen.getAllByText(/empty \(git printed nothing\)/i);

    // Two nulls (command, errors) and one empty (output). If the two states were
    // ever collapsed, one of these lists would be empty and the other would hold
    // all three.
    expect(neverCaptured).toHaveLength(2);
    expect(ranAndQuiet).toHaveLength(1);
    expect(neverCaptured[0].textContent).not.toEqual(ranAndQuiet[0].textContent);
  });

  it("renders a non-empty stream verbatim", () => {
    const stderr = "fatal: couldn't find remote ref refs/heads/gone";
    renderWith({ rawStderr: stderr });

    // Verbatim matters: the streams are already redacted at capture in `run_git`,
    // so re-cleaning or reformatting here would make this view disagree with the
    // log file and the error toast, which read the same bytes.
    expect(screen.getByText(stderr)).toBeDefined();
  });

  it("keeps whitespace-only output distinct from no output", () => {
    // All three streams are pinned so the assertion can be about absence: the
    // base fixture leaves stderr as `""`, which would otherwise supply the very
    // message this test claims should not appear.
    renderWith({ rawCommand: "git -C C:\\repos\\example status", rawStdout: "   \n", rawStderr: null });

    // Whitespace is something git printed. Trimming it into the empty case would
    // report "git printed nothing" about a run that printed something.
    expect(screen.queryByText(/empty \(git printed nothing\)/i)).toBeNull();
  });
});

describe("ActivityReceipt facts", () => {
  it("shows a zero exit code rather than treating it as absent", () => {
    renderWith({ exitCode: 0 });

    // `0` is falsy, so the naive render drops the single most common exit code
    // and shows the null placeholder for a perfectly successful run.
    const exit = screen.getByText("Exit code").parentElement;
    expect(exit?.textContent).toContain("0");
  });

  it("shows a missing exit code as a placeholder, not as zero", () => {
    renderWith({ exitCode: null });

    const exit = screen.getByText("Exit code").parentElement;
    expect(exit?.textContent).not.toContain("0");
  });

  it("names an unknown repo instead of hiding the row", () => {
    renderWith({}, null);

    // The audit trail outlives the repo on purpose: a repo deleted after its
    // rows were written still has history, and dropping the row would quietly
    // shorten the record.
    expect(screen.getByText("Unknown repo")).toBeDefined();
  });

  it("renders the repo name when it is known", () => {
    renderWith({}, "example");

    expect(screen.getByText("example")).toBeDefined();
  });
});

describe("ActivityReceipt sharing warning", () => {
  it("qualifies what redaction does rather than promising it is clean", () => {
    renderWith({});

    // This wording was deliberately weakened after an adversarial review found
    // the original "Credentials are stripped from captured git output" claimed
    // more than the redactor delivers: URL credentials go structurally, the
    // token-prefix list is short and best-effort, and `raw_command` always
    // carries the repository's full local path. An unqualified reassurance next
    // to a Copy button is the worst place in the app to overclaim, because the
    // user acts on it immediately and irreversibly.
    const warning = screen.getByText(/before sharing/i);
    expect(warning.textContent).toMatch(/full local path/i);
    expect(warning.textContent).toMatch(/where recognized|unfamiliar format/i);
  });

  it("does not claim captured output is unconditionally safe to share", () => {
    renderWith({});

    const warning = screen.getByText(/before sharing/i).textContent ?? "";
    // The specific regression: any phrasing that ends the promise at "removed"
    // or "stripped" without the qualifier that follows it.
    expect(warning).not.toMatch(/credentials are (stripped|removed)\.?$/i);
  });
});
