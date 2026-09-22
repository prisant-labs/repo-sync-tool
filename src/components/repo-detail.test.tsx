// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type { ActivityRecord, GroupSummary, RepoDetail, Settings } from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { ToastContext } from "@/hooks/use-toast";
import { Drawer } from "@/components/ui/drawer";
import { RepoDetailPanel, REPO_DETAIL_TITLE_ID } from "@/components/repo-detail";

/**
 * The removal states these tests exist to keep distinguishable (BL-NI-85, plus
 * the four confirmed findings of the 2026-08-28 adversarial review):
 *
 * 1. ARMED is not REMOVED. The first click on "Remove from RepoSync" must never
 *    reach the backend; only the explicit confirm may. Removal deletes the
 *    repo's RepoSync data irrecoverably, so collapsing the two clicks into one
 *    is the regression that matters most.
 * 2. FAILED is not DONE. A genuinely failed removal must leave the drawer open
 *    and say so, remediation included; closing the drawer is the success
 *    signal, so closing on failure would read as "removed" about a repo that
 *    is still tracked.
 * 3. ALREADY GONE is not FAILED. `db.not_found` means the requested end state
 *    is already true (another instance sharing the database may have removed
 *    it first, BL-NI-73), so it converges like a success instead of stranding
 *    a drawer on a dead id.
 * 4. A STALE resolve is not a live one. The backend holds the per-repo lock
 *    across the delete, so a removal can resolve after this panel is gone; it
 *    must refresh the list without closing whatever drawer is open by then.
 * 5. Arming swaps the focused trigger out of the DOM; keyboard focus must
 *    follow the swap in both directions rather than fall out of the drawer's
 *    focus trap.
 *
 * Assertions are about rendered meaning, IPC traffic, and focus, not markup,
 * so a restyle of the section should leave them untouched.
 */

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

const DETAIL: RepoDetail = {
  id: 7,
  localName: "example",
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
  localPath: "C:\\repos\\example",
  remoteOriginUrl: "https://github.com/example/example.git",
  defaultBranch: "main",
  headState: "branch",
  updateMode: "fetch_only",
  checkFrequencyMin: 0,
  createdAt: 1_690_000_000,
  notes: null,
  activeBranch: "main",
  headSha: "abcdef1234567890",
  upstreamBranch: "origin/main",
  upstreamState: null,
  lastLocalCommitAt: null,
  lastUpdatedAt: null,
  lastAttemptedAt: null,
  nextCheckAt: null,
  consecutiveFailures: 0,
  description: null,
  topicsJson: null,
  latestReleaseAt: null,
  latestReleaseUrl: null,
  isArchived: false,
  lastRemoteSha: null,
  lastFetchedAt: null,
  openPrCount: null,
  defaultBranchPrCount: null,
  prLastCheckedAt: null,
  stars: null,
  forks: null,
  license: null,
  size: null,
  visibility: null,
  homepage: null,
};

const SETTINGS: Settings = {
  globalCheckMinutes: 360,
  quietHoursStart: null,
  quietHoursEnd: null,
  notifyOnRelease: true,
  notifyOnFailure: true,
  gitExecutablePath: null,
  editorCommand: "code",
  terminalCommand: "wt",
  autostart: false,
  activityRetentionD: 90,
  githubTokenPresent: false,
  autoUpdateCheck: true,
  closeMinimizesToTray: true,
};

beforeEach(() => {
  mockCommand(commands, "repoGet", async () => ok(DETAIL));
  mockCommand(commands, "groupList", async () => ok([]));
  mockCommand(commands, "groupsForRepo", async () => ok([]));
  mockCommand(commands, "settingsGet", async () => ok(SETTINGS));
  // N4: the panel now fetches this repo's own activity for the Activity tab
  // on mount, regardless of which tab is showing (see `RepoDetailPanel`'s
  // `useActivity` call). Without this stub the real binding is hit and every
  // test in this file would fail on an unmocked IPC call, not just the ones
  // that visit that tab.
  mockCommand(commands, "activityList", async () => ok([]));
  // The drawer subscribes to per-repo backend events on mount; without a Tauri
  // runtime the real `listen` cannot resolve, so stub it to a no-op unlistener.
  for (const ev of [events.repoCheckCompleted, events.repoUpdateCompleted, events.repoStateChanged]) {
    vi.spyOn(ev, "listen").mockResolvedValue(() => {});
  }
});

function renderPanel() {
  const onChanged = vi.fn();
  const onClose = vi.fn();
  const toast = vi.fn();
  const view = render(
    <ToastContext.Provider value={toast}>
      <RepoDetailPanel id={7} onChanged={onChanged} onClose={onClose} />
    </ToastContext.Provider>,
  );
  return { onChanged, onClose, toast, unmount: view.unmount };
}

/**
 * Like `renderPanel`, but inside the real `Drawer` primitive so its focus
 * trap (`use-modal-a11y.ts`) is actually attached and exercised - `renderPanel`
 * alone renders no modal wrapper at all, so a test asserting the OUTER trap's
 * wrap-around behaviour (Tab from the last control back to the first) needs
 * this instead.
 */
function renderPanelInDrawer() {
  const onChanged = vi.fn();
  const onClose = vi.fn();
  const toast = vi.fn();
  render(
    <ToastContext.Provider value={toast}>
      <Drawer open onClose={onClose} size="wide" aria-labelledby={REPO_DETAIL_TITLE_ID}>
        <RepoDetailPanel id={7} onChanged={onChanged} onClose={onClose} />
      </Drawer>
    </ToastContext.Provider>,
  );
  return { onChanged, onClose, toast };
}

/** The armed-state confirm button, distinct from the "Remove from RepoSync" trigger. */
function confirmButton() {
  return screen.getByRole("button", { name: "Remove" });
}

/**
 * Remove moved to the Settings tab in N4 (D3, the ratified tab mapping).
 * `TabPanel` keeps every panel mounted but toggles the `hidden` attribute
 * (see `ui/tabs.tsx`'s doc comment), so "Remove from RepoSync" is present in
 * the DOM but excluded from the accessibility tree - and from `getByRole`/
 * `findByRole`, which respect that by default - until the Settings tab is
 * the active one. Every Remove test needs this step first. Mechanical only:
 * none of the six tests' own assertions (arm/cancel/confirm/not-found/failure/
 * stale-resolve) changed.
 */
async function gotoSettingsTab(user: ReturnType<typeof userEvent.setup>) {
  await user.click(await screen.findByRole("tab", { name: "Settings" }));
}

describe("RepoDetailPanel remove", () => {
  it("discloses the full consequence up front, and the first click only arms the confirm", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    const arm = await screen.findByRole("button", { name: "Remove from RepoSync" });
    // Everything the cascade deletes is named before any click, alongside what
    // is spared: the folder on disk.
    expect(screen.getByText(/group assignments/i)).toBeDefined();
    expect(screen.getByText(/policy and cadence/i)).toBeDefined();
    expect(screen.getByText(/not touched/i)).toBeDefined();

    await user.click(arm);

    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByText(/cannot be undone/i)).toBeDefined();
    // Focus follows the trigger it replaced, staying inside the drawer's trap.
    expect(document.activeElement).toBe(confirmButton());
  });

  it("cancel disarms without any IPC call and returns focus to the trigger", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(remove).not.toHaveBeenCalled();
    expect(screen.queryByText(/cannot be undone/i)).toBeNull();
    const arm = screen.getByRole("button", { name: "Remove from RepoSync" });
    expect(document.activeElement).toBe(arm);
  });

  it("confirm removes the right repo, then closes the drawer and refreshes the list", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    const { onChanged, onClose } = renderPanel();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(confirmButton());

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(remove).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledWith(7);
    expect(onChanged).toHaveBeenCalled();
  });

  it("treats an already-removed repo as the requested end state, not a failure", async () => {
    // db.not_found is the store's rows_affected == 0 answer: the repo is
    // already gone (removed by another instance sharing the database).
    mockCommand(commands, "repoRemove", async () => err("db.not_found", "repo 7 was not found"));
    const { onChanged, onClose, toast } = renderPanel();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(confirmButton());

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(onChanged).toHaveBeenCalled();
    expect(toast).toHaveBeenCalledWith("ok", "Removed example", expect.stringMatching(/already gone/i));
    expect(toast).not.toHaveBeenCalledWith("error", expect.anything(), expect.anything());
  });

  it("a genuine failure reports message plus remediation and keeps the drawer open", async () => {
    mockCommand(commands, "repoRemove", async () =>
      err("db.query_failed", "the database rejected the write", "Close other RepoSync instances and retry."),
    );
    const { onClose, toast } = renderPanel();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(confirmButton());

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith(
        "error",
        "Could not remove",
        "the database rejected the write Close other RepoSync instances and retry.",
      ),
    );
    expect(onClose).not.toHaveBeenCalled();
    // The repo is still tracked, so the drawer still shows it.
    expect(screen.getByText(/not touched/i)).toBeDefined();
  });

  it("a removal resolving after the panel is gone refreshes the list without closing the current drawer", async () => {
    type RemoveResult = Awaited<ReturnType<(typeof commands)["repoRemove"]>>;
    let resolveRemove: (result: RemoveResult) => void = () => {};
    mockCommand(
      commands,
      "repoRemove",
      () =>
        new Promise<RemoveResult>((resolve) => {
          resolveRemove = resolve;
        }),
    );
    const { onChanged, onClose, unmount } = renderPanel();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(confirmButton());
    // The user closes the drawer (or opens another repo) while the backend
    // still holds the per-repo lock; this panel instance is gone by the time
    // the removal resolves.
    unmount();
    resolveRemove(ok(null));

    await waitFor(() => expect(onChanged).toHaveBeenCalled());
    expect(onClose).not.toHaveBeenCalled();
  });
});

/** Whether the named tab is the one currently marked selected. */
function tabSelected(name: string): boolean {
  return screen.getByRole("tab", { name }).getAttribute("aria-selected") === "true";
}

describe("RepoDetailPanel tabs (N4)", () => {
  it("switches tabs by mouse click; the previous tab's panel is hidden, not unmounted", async () => {
    renderPanel();
    const user = userEvent.setup();

    await screen.findByText("Up to date with origin");
    expect(tabSelected("Overview")).toBe(true);

    await user.click(screen.getByRole("tab", { name: "Activity" }));

    expect(tabSelected("Activity")).toBe(true);
    expect(tabSelected("Overview")).toBe(false);
    // `queryByText` does not filter hidden content, so it cannot tell
    // "unmounted" from "hidden" - assert via the panel's own `hidden`
    // attribute instead (TabPanel keeps every panel mounted; see
    // `ui/tabs.tsx`'s file doc comment).
    const overviewHeading = screen.getByText("Up to date with origin");
    const overviewPanel = overviewHeading.closest('[role="tabpanel"]');
    expect(overviewPanel).not.toBeNull();
    expect(overviewPanel?.hasAttribute("hidden")).toBe(true);
    expect(await screen.findByText("No activity yet for this repository.")).toBeDefined();
  });

  it("switches tabs with Left/Right/Home/End, and moving focus also activates (automatic activation)", async () => {
    renderPanel();
    const user = userEvent.setup();
    await screen.findByText("Up to date with origin");

    const overviewTab = screen.getByRole("tab", { name: "Overview" });
    overviewTab.focus();

    await user.keyboard("{ArrowRight}");
    expect(tabSelected("Activity")).toBe(true);
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Activity" }));

    await user.keyboard("{ArrowRight}");
    expect(tabSelected("Settings")).toBe(true);

    // Wraps forward past the last tab.
    await user.keyboard("{ArrowRight}");
    expect(tabSelected("Overview")).toBe(true);

    await user.keyboard("{End}");
    expect(tabSelected("Settings")).toBe(true);

    await user.keyboard("{Home}");
    expect(tabSelected("Overview")).toBe(true);

    // Wraps backward past the first tab.
    await user.keyboard("{ArrowLeft}");
    expect(tabSelected("Settings")).toBe(true);
  });

  it("roving tabindex: only the active tab is a Tab stop", async () => {
    renderPanel();
    await screen.findByText("Up to date with origin");

    expect(screen.getByRole("tab", { name: "Overview" }).getAttribute("tabindex")).toBe("0");
    expect(screen.getByRole("tab", { name: "Activity" }).getAttribute("tabindex")).toBe("-1");
    expect(screen.getByRole("tab", { name: "Settings" }).getAttribute("tabindex")).toBe("-1");
  });

  it("keeps the focus trap intact across a tab switch: Tab from the last control wraps to the first", async () => {
    renderPanelInDrawer();
    const user = userEvent.setup();
    await gotoSettingsTab(user);

    const removeTrigger = await screen.findByRole("button", { name: "Remove from RepoSync" });
    removeTrigger.focus();
    expect(document.activeElement).toBe(removeTrigger);

    await user.keyboard("{Tab}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close" }));

    await user.keyboard("{Shift>}{Tab}{/Shift}");
    expect(document.activeElement).toBe(removeTrigger);
  });

  it("excludes hidden panels from the trap's boundary: with Overview active (DOM-first), Tab from its own last control wraps straight to Close, never landing on a hidden Activity/Settings control", async () => {
    // The test above proves the trap still works when the ACTIVE tab
    // (Settings) happens to be LAST in DOM order - its own controls are
    // naturally the trap's last element regardless of whether hidden
    // content is excluded, so that case alone cannot distinguish a correct
    // exclusion from a filter that does nothing (Codex adversarial review,
    // finding 2, the `focusableIn` fix in `use-modal-a11y.ts`). Overview is
    // FIRST in DOM order (`repo-detail.tsx`'s TabPanel order is
    // overview/activity/settings) with the other two - Activity's empty
    // state and Settings' Cadence/Update-policy/Remove controls - mounted
    // but hidden AFTER it. Without the exclusion filter, the trap's
    // computed "last" element would be one of those hidden controls, and
    // Tab from Overview's real last VISIBLE control would never satisfy the
    // wrap condition.
    renderPanelInDrawer();
    await screen.findByText("Up to date with origin");
    expect(tabSelected("Overview")).toBe(true);

    // The trap's last element when Overview is active is the Overview PANEL
    // ITSELF. Until 2026-09-14 this test focused a control inside the panel -
    // the "Where it lives" > Remote row, which rendered as a link-styled
    // button - but decision P3 moved every repo-level action into the header
    // chrome above the tab list, and P2 moved the Remote row up with them, so
    // Overview's content has no focusable control of its own in the clean
    // state. `TabPanel` carries `tabIndex={0}` (see `ui/tabs.tsx`) precisely
    // so a panel is still reachable and scrollable by keyboard when nothing
    // inside it can take focus, which makes the active panel the last
    // focusable element in the drawer.
    //
    // This still discriminates a working exclusion filter from a broken one.
    // With the filter, the trap's "last" is this panel, so Tab is intercepted
    // and wraps to Close. Without it, "last" would be one of the mounted-but-
    // hidden Settings controls, the handler would not fire, and native
    // tabbing - which skips `hidden` content - would walk straight out of the
    // drawer rather than landing on Close.
    const overviewPanel = screen.getByRole("tabpanel", { name: "Overview" });
    overviewPanel.focus();
    expect(document.activeElement).toBe(overviewPanel);

    const user = userEvent.setup();
    await user.keyboard("{Tab}");
    expect(document.activeElement).toBe(screen.getByRole("button", { name: "Close" }));
  });
});

describe("RepoDetailPanel activity tab (N4)", () => {
  function activityRow(id: number, overrides: Partial<ActivityRecord> = {}): ActivityRecord {
    return {
      id,
      repoId: 7,
      timestamp: 1_700_000_000 - id,
      actionType: "check",
      status: "success",
      reasonCode: null,
      summary: `entry ${id}`,
      commitRange: null,
      rawCommand: null,
      rawStdout: null,
      rawStderr: null,
      exitCode: null,
      durationMs: null,
      ...overrides,
    };
  }

  it("fetches this repo's activity scoped by repoId, with no filter controls in the panel", async () => {
    const list = mockCommand(commands, "activityList", async () => ok([activityRow(1)]));
    renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("tab", { name: "Activity" }));

    await waitFor(() =>
      expect(list).toHaveBeenCalledWith(
        expect.objectContaining({ repoId: 7, groupId: null, actionType: null, status: null }),
      ),
    );
    expect(screen.getByText("entry 1")).toBeDefined();
    // No filter chips or controls: the panel scopes to one repo already.
    expect(screen.queryByRole("group")).toBeNull();
  });

  it("shows the truncation notice only when the sentinel row (N+1) comes back, never from a length guess", async () => {
    const sixty = Array.from({ length: 60 }, (_, i) => activityRow(i + 1));
    mockCommand(commands, "activityList", async () => ok(sixty));
    renderPanel();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: "Activity" }));

    await screen.findByText("entry 1");
    expect(screen.queryByText(/most recent entries/i)).toBeNull();
  });

  it("shows the truncation notice when a 61st (sentinel) row arrives", async () => {
    const sixtyOne = Array.from({ length: 61 }, (_, i) => activityRow(i + 1));
    mockCommand(commands, "activityList", async () => ok(sixtyOne));
    renderPanel();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: "Activity" }));

    expect(await screen.findByText(/showing the 60 most recent entries/i)).toBeDefined();
    // The sentinel itself (the 61st row) is never rendered.
    expect(screen.queryByText("entry 61")).toBeNull();
  });

  it("opens the activity receipt drawer on a row click, and Escape closes only the receipt", async () => {
    mockCommand(commands, "activityList", async () => ok([activityRow(1, { summary: "fetched 3 commits" })]));
    renderPanel();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: "Activity" }));

    await user.click(await screen.findByText("fetched 3 commits"));

    expect(await screen.findByRole("button", { name: "Close receipt" })).toBeDefined();
    // The receipt shows the record's own repo name (this repo), not "Unknown repo".
    expect(screen.getByRole("heading", { name: "example" })).toBeDefined();

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Close receipt" })).toBeNull());
    // The outer drawer's own Close button is still there: Escape closed only
    // the nested receipt, not the whole panel.
    expect(screen.getByRole("button", { name: "Close" })).toBeDefined();
  });

  it("names both modal layers from their own visible heading, and restores focus to the opener row on close (Codex adversarial review, finding 3)", async () => {
    // `renderPanel` alone (used above) renders no OUTER Drawer, so it can
    // only exercise the nested receipt in isolation. Naming the outer layer
    // needs the real thing, hence `renderPanelInDrawer`.
    mockCommand(commands, "activityList", async () => ok([activityRow(1, { summary: "fetched 3 commits" })]));
    renderPanelInDrawer();
    const user = userEvent.setup();
    await user.click(await screen.findByRole("tab", { name: "Activity" }));

    const rowText = await screen.findByText("fetched 3 commits");
    const rowButton = rowText.closest("button");
    expect(rowButton).not.toBeNull();
    await user.click(rowButton!);
    expect(await screen.findByRole("button", { name: "Close receipt" })).toBeDefined();

    // Before this fix neither modal boundary had an accessible name at all
    // (both were bare "dialog"); now each is named from its own visible
    // heading (`REPO_DETAIL_TITLE_ID` for the outer panel,
    // `ACTIVITY_RECEIPT_TITLE_ID` for the receipt). They happen to show the
    // same text here ("example", this fixture's repo name) - which is
    // exactly the scenario a bare, unnamed "dialog" role could never
    // distinguish: two same-named dialogs open at once, each still
    // individually resolvable via `getByRole("dialog", { name })`.
    expect(screen.getAllByRole("dialog", { name: "example" })).toHaveLength(2);

    await user.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("button", { name: "Close receipt" })).toBeNull());
    // Escape closed only the inner layer: the outer dialog survives, still
    // named from the same heading.
    expect(screen.getByRole("dialog", { name: "example" })).toBeDefined();

    // Focus returns to the specific row that opened the receipt, not merely
    // "somewhere in the panel" - `useModalA11y` restores focus to whatever
    // `document.activeElement` was at open time, and clicking a `<button>`
    // natively focuses it first.
    expect(document.activeElement).toBe(rowButton);
  });
});

describe("RepoDetailPanel group pills (N4)", () => {
  const GROUPS: GroupSummary[] = [
    { id: 1, name: "Client work", color: "oklch(0.55 0.16 264)", repoCount: 2 },
    { id: 2, name: "Archived clients", color: null, repoCount: 1 },
  ];

  it("renders only member groups as pills, with an Add affordance reaching every non-member group", async () => {
    mockCommand(commands, "groupList", async () => ok(GROUPS));
    mockCommand(commands, "groupsForRepo", async () => ok([1]));
    renderPanel();

    await screen.findByText("Client work");
    // Membership-only: the non-member group is not rendered as a standing pill.
    expect(screen.queryByText("Archived clients")).toBeNull();
    expect(screen.getByRole("button", { name: "Add this repo to a group" })).toBeDefined();
  });

  it("removing a member pill calls groupUnassign for that group", async () => {
    mockCommand(commands, "groupList", async () => ok(GROUPS));
    mockCommand(commands, "groupsForRepo", async () => ok([1]));
    const unassign = mockCommand(commands, "groupUnassign", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

    await screen.findByText("Client work");
    await user.click(screen.getByRole("button", { name: "Remove from Client work" }));

    await waitFor(() => expect(unassign).toHaveBeenCalledWith(7, 1));
  });

  it("the Add disclosure lists every non-member group and assigns on click", async () => {
    mockCommand(commands, "groupList", async () => ok(GROUPS));
    mockCommand(commands, "groupsForRepo", async () => ok([1]));
    const assign = mockCommand(commands, "groupAssign", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

    await screen.findByText("Client work");
    await user.click(screen.getByRole("button", { name: "Add this repo to a group" }));

    const candidate = await screen.findByRole("button", { name: /Archived clients/ });
    await user.click(candidate);

    await waitFor(() => expect(assign).toHaveBeenCalledWith(7, 2));
  });
});

describe("RepoDetailPanel Website button (N4)", () => {
  it("is hidden when homepage is null", async () => {
    renderPanel();
    await screen.findByText("Up to date with origin");
    expect(screen.queryByRole("button", { name: "Website" })).toBeNull();
  });

  it("opens the homepage via repoOpenHomepage when set", async () => {
    mockCommand(commands, "repoGet", async () => ok({ ...DETAIL, homepage: "https://example.com" }));
    const openHomepage = mockCommand(commands, "repoOpenHomepage", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

    const button = await screen.findByRole("button", { name: "Website" });
    await user.click(button);

    await waitFor(() => expect(openHomepage).toHaveBeenCalledWith(7));
  });

  it("a rejected homepage URL routes through run's generic failure toast with the backend's own message", async () => {
    // github.invalid_external_url (BL-NI-94): the dedicated wire code for a
    // homepage value that fails the same http(s)-only scheme validation
    // `repo_open_remote` already applies. The glyph itself has no special
    // handling for this code - it goes through the drawer's shared `run`
    // helper like every other Open-in action, so this also pins that `run`'s
    // error arm covers the new action.
    mockCommand(commands, "repoGet", async () => ok({ ...DETAIL, homepage: "https://example.com" }));
    mockCommand(commands, "repoOpenHomepage", async () =>
      err("github.invalid_external_url", "the homepage URL is not a web address RepoSync will open"),
    );
    const { toast } = renderPanel();
    const user = userEvent.setup();

    const button = await screen.findByRole("button", { name: "Website" });
    await user.click(button);

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith(
        "error",
        "Action failed",
        "the homepage URL is not a web address RepoSync will open",
      ),
    );
  });

  /**
   * Walk item P5. The "Open in" row used to carry THREE outward buttons:
   * "Remote", a globe labelled "Open repository website", and a link glyph
   * labelled "Open homepage". The globe called `repoOpenRemote`, identical to
   * the "Remote" button beside it, while wearing the label that describes
   * what the link glyph actually does - so a screen reader announced "Open
   * repository website" and landed the user on the git remote. Removed
   * 2026-09-14. This test is the guard: the accessible name must not come
   * back attached to the remote command.
   */
  it("P5: no globe duplicating Remote under a website label", async () => {
    mockCommand(commands, "repoGet", async () =>
      ok({ ...DETAIL, remoteOriginUrl: "git@github.com:o/r.git", homepage: "https://example.com" }),
    );
    renderPanel();
    await screen.findByRole("button", { name: "Remote" });

    expect(screen.queryByRole("button", { name: "Open repository website" })).toBeNull();
    // The two that remain are distinct destinations, not two names for one.
    expect(screen.getByRole("button", { name: "Website" })).toBeDefined();
  });

  /**
   * Codex adversarial review, finding 2 (2026-09-14). Removing the duplicate
   * globe left the homepage action as an icon-only button whose destination
   * lived only in `aria-label` and `title`, sitting beside a visibly labelled
   * "Remote" - so a sighted keyboard user got no wording at all and a pointer
   * user had to hover to find out. `getByRole({ name })` alone CANNOT catch a
   * regression back to that state, because an `aria-label` satisfies it just
   * as well as visible text. This test reads `textContent`, which only visible
   * wording can satisfy.
   */
  it("the Website button's label is visible text, not only an accessible name", async () => {
    mockCommand(commands, "repoGet", async () =>
      ok({ ...DETAIL, remoteOriginUrl: "git@github.com:o/r.git", homepage: "https://example.com" }),
    );
    renderPanel();

    const website = await screen.findByRole("button", { name: "Website" });
    expect(website.textContent).toContain("Website");
    // Its labelled sibling, held to the same standard so the pair stays consistent.
    expect(screen.getByRole("button", { name: "Remote" }).textContent).toContain("Remote");
  });
});

/**
 * P3 (the 2026-09-14 drawer walk-through): the eight repo-level actions used
 * to live INSIDE the Overview tab, split across two rows - a bare button row
 * and an "Open in" section. They now sit in a single row in the header chrome,
 * above the tab list.
 *
 * The point of the move is that these actions stop being a property of one
 * tab, so the first test asserts exactly that: they are reachable while a
 * DIFFERENT tab is active. The rest pin the four conditional controls, because
 * moving a button between containers is precisely where a condition gets
 * dropped - the button still renders, the command still compiles, every other
 * test still passes, and the only evidence is a control that appears when it
 * should not or is clickable when it cannot succeed.
 *
 * Two of the four are HIDDEN on their condition and two are DISABLED WITH A
 * REASON, which is the distinction most at risk of being flattened into "just
 * hide it". Neither Terminal nor Editor had any test before this move; their
 * behaviour was documented only in a comment.
 */
describe("RepoDetailPanel header action row (P3)", () => {
  /** Both optional destinations present, so all eight controls render. */
  const WIRED = {
    ...DETAIL,
    remoteOriginUrl: "https://github.com/example/example.git",
    homepage: "https://example.com",
  };

  const ALL_ACTIONS = [
    "Folder",
    "Terminal",
    "Editor",
    "Remote",
    "Website",
    "Check now",
    "Refresh metadata",
    "Pause",
  ];

  it("keeps every action reachable while the Activity tab is the active one", async () => {
    mockCommand(commands, "repoGet", async () => ok(WIRED));
    renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("tab", { name: "Activity" }));
    expect(tabSelected("Activity")).toBe(true);

    for (const name of ALL_ACTIONS) {
      expect(screen.getByRole("button", { name })).toBeDefined();
    }
  });

  it("hides Remote when the repo has no origin", async () => {
    mockCommand(commands, "repoGet", async () => ok({ ...WIRED, remoteOriginUrl: null }));
    renderPanel();
    await screen.findByRole("button", { name: "Folder" });

    expect(screen.queryByRole("button", { name: "Remote" })).toBeNull();
    // The other four opening actions are unaffected by a missing origin.
    expect(screen.getByRole("button", { name: "Website" })).toBeDefined();
  });

  it("hides Pause once the repo is paused by hand, leaving Resume in the Focal card", async () => {
    mockCommand(commands, "repoGet", async () => ok({ ...WIRED, enabled: false }));
    renderPanel();
    await screen.findByRole("button", { name: "Folder" });

    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
    // Resume deliberately did NOT move into the header row: it belongs beside
    // the Focal card's explanation of why the repo is not being checked.
    expect(await screen.findByRole("button", { name: "Resume watching" })).toBeDefined();
  });

  it("hides Pause once the scheduler has auto-paused the repo", async () => {
    mockCommand(commands, "repoGet", async () => ok({ ...WIRED, autoPaused: true }));
    renderPanel();
    await screen.findByRole("button", { name: "Folder" });

    expect(screen.queryByRole("button", { name: "Pause" })).toBeNull();
  });

  /*
   * Terminal and Editor are gated on their Settings command being set at all.
   * Both backend commands return `InvalidSetting` when the column is NULL, so
   * before migration 0009 backfilled them these buttons looked live and failed
   * on click - with a Settings field whose placeholder ("code", "default")
   * read like a configured value.
   *
   * Rendered-and-disabled, never hidden: a hidden button tells the user
   * nothing, while a disabled one with a `title` says what to go and set. That
   * is the behaviour these two tests exist to stop a future redesign from
   * quietly converting into a conditional render.
   */
  it("disables Terminal and Editor with the reason on them when their commands are unset", async () => {
    mockCommand(commands, "repoGet", async () => ok(WIRED));
    // An empty value can no longer ARRIVE through the Settings screen, and
    // that is worth stating plainly rather than letting the next reader assume
    // this is the routine path. PR #86 rejects a cleared editor or terminal
    // command at save time ("the settings UI sends an emptied text box as
    // null", `commands/mod.rs:856-868`), and migration 0009 backfilled the
    // NULLs that existed before it. So neither "" nor null is reachable by
    // using the app normally.
    //
    // What this pins is the FRONTEND's derivation of "unset" (`repo-detail
    // .tsx:146-147`), which is the last thing standing if an empty value ever
    // does arrive - a hand-edited database, a restored pre-0009 file, or a
    // future backend change that relaxes the save rule. It is exactly the kind
    // of defence a layout change deletes without noticing, because nothing a
    // user can do makes its absence visible.
    //
    // Note `?? "code"`: that fallback treats NULL as available on purpose,
    // because null is also what the hook reports while settings are loading,
    // and a disabled button that enables a moment later is worse than a
    // briefly optimistic one. So "" is the representation to test with, not
    // null - the two are deliberately not equivalent here.
    mockCommand(commands, "settingsGet", async () =>
      ok({ ...SETTINGS, terminalCommand: "", editorCommand: "  " }),
    );
    renderPanel();

    const terminal = await screen.findByRole("button", { name: "Terminal" });
    const editor = screen.getByRole("button", { name: "Editor" });

    expect(terminal.hasAttribute("disabled")).toBe(true);
    expect(terminal.getAttribute("title")).toBe("Set a terminal command in Settings");
    expect(editor.hasAttribute("disabled")).toBe(true);
    expect(editor.getAttribute("title")).toBe("Set an editor command in Settings");
  });

  it("leaves Terminal and Editor enabled and unexplained when their commands are set", async () => {
    mockCommand(commands, "repoGet", async () => ok(WIRED));
    renderPanel();

    const terminal = await screen.findByRole("button", { name: "Terminal" });
    const editor = screen.getByRole("button", { name: "Editor" });

    expect(terminal.hasAttribute("disabled")).toBe(false);
    // No `title` when there is nothing to explain - a tooltip that always
    // shows would train the user to ignore the one that matters.
    expect(terminal.getAttribute("title")).toBeNull();
    expect(editor.hasAttribute("disabled")).toBe(false);
    expect(editor.getAttribute("title")).toBeNull();
  });

  /*
   * P2 moved the path and the remote URL into the header as text. The remote
   * used to render as a link-styled button that opened the remote, duplicating
   * the Remote button beside it; P3 collapses duplicate affordances onto one
   * control, so the line reports and the button acts.
   */
  it("shows the path and remote as header text, with the opening click on the buttons", async () => {
    mockCommand(commands, "repoGet", async () => ok(WIRED));
    renderPanel();

    const path = await screen.findByText(WIRED.localPath);
    expect(path.tagName).toBe("SPAN");

    const remote = screen.getByText(WIRED.remoteOriginUrl);
    expect(remote.tagName).toBe("SPAN");
    expect(screen.getByRole("button", { name: "Remote" })).toBeDefined();
  });

  it("omits the remote line entirely when there is no origin, rather than printing a placeholder", async () => {
    mockCommand(commands, "repoGet", async () => ok({ ...WIRED, remoteOriginUrl: null }));
    renderPanel();
    await screen.findByText(WIRED.localPath);

    // The old "Where it lives" row printed the string "none" here. A header
    // line has no label to hang that off, so the line is simply absent.
    expect(screen.queryByText("none")).toBeNull();
  });
});

/**
 * Composite pin 34. The Focal panel's apply button used to call
 * `repoUpdateNow(r.id, "pull_ff_only")` with the mode written into the call, so
 * a repository the user had deliberately set to Check only or Fetch only still
 * offered a button that pulled - the one thing those two modes exist to prevent.
 *
 * These tests pin the correctness half only. The full J.4 sync model (every
 * row's button carrying its own mode) is still unratified - bench decision R2,
 * marked Unsure on 2026-09-22 - and is deliberately NOT implemented here.
 */
describe("the apply action obeys the repository's own update mode (pin 34)", () => {
  /** What `repoUpdateNow` resolves to; its contents do not matter to these tests. */
  const UPDATE_OK = {
    repoId: 7,
    mode: "pull_ff_only",
    outcome: "updated",
    commitRange: null,
    ahead: null,
    behind: null,
    updatedAt: 0,
  };

  /** A repo that is behind origin, so the Focal panel offers its apply branch. */
  function behind(updateMode: string): RepoDetail {
    return { ...DETAIL, behindCount: 3, aheadCount: 0, isDirty: false, updateMode };
  }

  it("sends the repository's OWN mode, never a hardcoded fast-forward", async () => {
    mockCommand(commands, "repoGet", async () => ok(behind("pull_ff_only")));
    const updateNow = mockCommand(commands, "repoUpdateNow", async () => ok(UPDATE_OK));
    renderPanel();

    const button = await screen.findByRole("button", { name: /Fast-forward now/ });
    await userEvent.setup().click(button);

    await waitFor(() => expect(updateNow).toHaveBeenCalled());
    // The second argument is the mode. It must come from the repo, which is why
    // the fixture and the assertion name the same value in two places on purpose:
    // a hardcoded literal in the component would pass a test that only checked
    // "pull_ff_only" against a pull_ff_only repo.
    expect(updateNow.mock.calls[0][1]).toBe("pull_ff_only");
  });

  it.each(["check_only", "fetch_only"])(
    "offers NO apply button for a %s repository, and says why",
    async (mode) => {
      mockCommand(commands, "repoGet", async () => ok(behind(mode)));
      const updateNow = mockCommand(commands, "repoUpdateNow", async () => ok(UPDATE_OK));
      renderPanel();

      // The behind state still reports itself - the repo IS behind, and hiding
      // that would be a different lie than the one being fixed.
      await screen.findByText(/3 commits behind origin/);

      // Precise on purpose: "Check now" is a different, always-present button,
      // so a loose /now$/ matcher would catch it and prove nothing.
      expect(
        screen.queryByRole("button", { name: /^(Fast-forward|Fetch only|Check only) now$/ }),
      ).toBeNull();
      expect(
        screen.getByText(/RepoSync will not pull these commits for you/),
      ).toBeDefined();
      expect(updateNow).not.toHaveBeenCalled();
    },
  );

  it("offers no apply button for a mode it does not recognise, rather than guessing", async () => {
    // `RepoDetail.updateMode` is a plain `String` on the wire (E-06), so an
    // unrecognised value is reachable - a newer backend, or a hand-edited
    // database. Failing closed is the only safe direction: the alternative is
    // pulling against a configuration this build cannot interpret.
    mockCommand(commands, "repoGet", async () => ok(behind("some_future_mode")));
    renderPanel();

    await screen.findByText(/3 commits behind origin/);
    // Assert the EXPLANATION renders, not merely that a button is missing. With the
    // pre-fix hardcode a button WAS present, labelled from the unknown mode, so a
    // name-based absence check passed while the bug was live - found by probing
    // this very test against the old code.
    expect(screen.getByText(/RepoSync will not pull these commits for you/)).toBeDefined();
  });
});
