// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands } from "@/lib/bindings";
import type { ActivityRecord, RepoSummary } from "@/lib/bindings";
import { ACTIVITY_FETCH_LIMIT, ACTIVITY_PAGE_LIMIT } from "@/lib/activity";
import { mockCommand, ok } from "@/test/mock-ipc";
import { ActivityScreen } from "@/screens/activity";

/**
 * N3 (ui-delivery-plan.md ledger B6): the Activity screen rebuilt on the
 * shared `DataTable`, closing BL-NI-89 (the Activity row renders a serialized
 * debug string). These tests are about MEANING, not the grid: real Time /
 * Repository / Action / Outcome / Summary columns exist, the sentinel-keyed
 * truncation notice survives, both empty-state wordings survive, the filter
 * chips still carry no count (an honesty constraint, not an omission - see
 * `filter-chip.tsx`), and PR #73's row accessibility pattern (mouse-only row
 * click, a labeled focusable chevron as THE keyboard path) is preserved.
 */

function record(overrides: Partial<ActivityRecord> = {}): ActivityRecord {
  return {
    id: 1,
    repoId: 7,
    timestamp: 1_700_000_000,
    actionType: "update",
    status: "success",
    reasonCode: null,
    summary: "Fast-forwarded 3 commits",
    commitRange: null,
    rawCommand: null,
    rawStdout: null,
    rawStderr: null,
    exitCode: 0,
    durationMs: 412,
    ...overrides,
  };
}

function repo(overrides: Partial<RepoSummary> = {}): RepoSummary {
  return {
    id: 7,
    localName: "repo-a",
    localPath: "E:\\Projects\\repo-a",
    hostType: "github",
    aheadCount: 0,
    behindCount: 0,
    isDirty: false,
    isDetached: false,
    enabled: true,
    autoPaused: false,
    lastCheckedAt: 1_700_000_000,
    lastErrorCode: null,
    latestReleaseTag: null,
    openPrCount: null,
    lastLocalCommitAt: null,
    activeBranch: "main",
    upstreamState: "tracking",
    stars: null,
    forks: null,
    license: null,
    size: null,
    visibility: null,
    homepage: null,
    ...overrides,
  };
}

function manyRecords(n: number): ActivityRecord[] {
  // Newest first, like the real `activity_list` ORDER BY, one second apart.
  return Array.from({ length: n }, (_, i) => record({ id: i + 1, timestamp: 1_700_000_000 - i }));
}

function renderScreen(rows: ActivityRecord[], repos: RepoSummary[] = [repo()]) {
  const activityList = mockCommand(commands, "activityList", async () => ok(rows));
  mockCommand(commands, "repoList", async () => ok(repos));
  const view = render(<ActivityScreen />);
  return { activityList, ...view };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("ActivityScreen table", () => {
  it("renders the five ratified columns, closing BL-NI-89's serialized-row structure", async () => {
    renderScreen([record()]);

    await screen.findByRole("columnheader", { name: "Time" });
    for (const name of ["Time", "Repository", "Action", "Outcome", "Summary"]) {
      expect(screen.getByRole("columnheader", { name })).toBeDefined();
    }
  });

  it("renders the action type, outcome chip and human summary as separate cells, not one serialized string", async () => {
    renderScreen([record({ actionType: "check", status: "failed", summary: "Auth failed" })]);
    await screen.findByText("Auth failed");

    // Column order (activity.tsx): time, repo, action, outcome, summary,
    // actions - action is cell index 2, outcome index 3.
    const row = screen.getByText("Auth failed").closest('[role="row"]') as HTMLElement;
    const cells = within(row).getAllByRole("cell");
    expect(cells[2].textContent).toBe("check");
    expect(cells[3].textContent).toBe("failed");
    expect(cells[4].textContent).toBe("Auth failed");
    // Never the old serialized form.
    expect(screen.queryByText(/mode=|outcome=/)).toBeNull();
  });

  it("a null summary renders the muted dash, not a blank cell", async () => {
    renderScreen([record({ summary: null })]);
    await screen.findByRole("columnheader", { name: "Summary" });

    // There is exactly one data row; find it via its outcome chip text.
    const dataRow = screen.getByText("success").closest('[role="row"]') as HTMLElement;
    expect(within(dataRow).getAllByRole("cell")[4].textContent).toBe("-");
  });

  it("resolves the repo name for the row, and dashes when the repo has no known name (e.g. removed)", async () => {
    renderScreen(
      [record({ id: 1, repoId: 7 }), record({ id: 2, repoId: 999, summary: "Orphaned row" })],
      [repo({ id: 7, localName: "repo-a" })],
    );
    await screen.findByText("repo-a");

    const orphanRow = screen.getByText("Orphaned row").closest('[role="row"]') as HTMLElement;
    expect(within(orphanRow).getAllByRole("cell")[1].textContent).toBe("-");
  });

  it("filter chips carry no count: the accessible name is the bare label", async () => {
    renderScreen([record()]);
    await screen.findByRole("columnheader", { name: "Time" });

    // If a count badge were ever added (the regression `filter-chip.tsx`'s
    // optional count exists to prevent), the accessible name would carry the
    // digits too and this exact-name lookup would stop matching.
    expect(screen.getByRole("button", { name: "All actions" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Checks" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Any outcome" })).toBeDefined();
  });

  it("clicking a filter chip re-fetches with the real wire filter shape, not a client-side narrow", async () => {
    const { activityList } = renderScreen([record()]);
    await screen.findByRole("columnheader", { name: "Time" });
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Checks" }));

    await waitFor(() =>
      expect(activityList).toHaveBeenLastCalledWith({
        repoId: null,
        actionType: "check",
        status: null,
        limit: ACTIVITY_FETCH_LIMIT,
      }),
    );
  });

  it("shows the unfiltered empty state when there is no activity at all", async () => {
    renderScreen([]);
    expect(await screen.findByText("No activity yet")).toBeDefined();
  });

  it("shows the filtered empty state, worded differently, once a filter is active", async () => {
    renderScreen([]);
    await screen.findByText("No activity yet");
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Failed" }));

    expect(await screen.findByText("Nothing matches this filter")).toBeDefined();
    expect(screen.queryByText("No activity yet")).toBeNull();
  });

  it("exactly ACTIVITY_PAGE_LIMIT rows: no truncation notice", async () => {
    renderScreen(manyRecords(ACTIVITY_PAGE_LIMIT));
    await screen.findByRole("columnheader", { name: "Time" });

    expect(screen.queryByText(/most recent matching entries/)).toBeNull();
  });

  it("one row over the limit (the sentinel): the truncation notice appears, keyed on the sentinel row, not a length guess", async () => {
    renderScreen(manyRecords(ACTIVITY_PAGE_LIMIT + 1));
    await screen.findByRole("columnheader", { name: "Time" });

    expect(
      await screen.findByText(new RegExp(`Showing the ${ACTIVITY_PAGE_LIMIT} most recent matching entries`)),
    ).toBeDefined();
    // The sentinel row itself is never rendered.
    const table = screen.getByRole("table", { name: "Activity log" });
    expect(within(table).getAllByRole("row")).toHaveLength(ACTIVITY_PAGE_LIMIT + 1); // +1 for the header row
  });

  it("the table has a valid accessible tree: a row carries no button role or tabIndex", async () => {
    renderScreen([record()]);
    await screen.findByText("success");

    const dataRow = screen.getByText("success").closest('[role="row"]') as HTMLElement;
    expect(dataRow.getAttribute("role")).toBe("row");
    expect(dataRow.hasAttribute("tabindex")).toBe(false);
  });

  it("Enter on the chevron is THE keyboard path into the receipt drawer", async () => {
    renderScreen([record({ summary: "Fast-forwarded 3 commits" })]);
    await screen.findByText("Fast-forwarded 3 commits");
    const user = userEvent.setup();

    screen.getByRole("button", { name: "Open receipt" }).focus();
    await user.keyboard("{Enter}");

    expect(await screen.findByRole("button", { name: /copy/i })).toBeDefined();
  });

  it("clicking anywhere else on the row also opens the receipt, as a mouse convenience", async () => {
    renderScreen([record({ summary: "Fast-forwarded 3 commits" })]);
    await screen.findByText("Fast-forwarded 3 commits");
    const user = userEvent.setup();

    await user.click(screen.getByText("Fast-forwarded 3 commits"));

    expect(await screen.findByRole("button", { name: /copy/i })).toBeDefined();
  });
});
