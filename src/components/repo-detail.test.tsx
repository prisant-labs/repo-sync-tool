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

    // The last focusable control inside Overview's own content: the
    // "Where it lives" > Remote row renders as a link-styled button when
    // `remoteOriginUrl` is set. Selected by its visible URL text since an
    // earlier "Open in > Remote" button shares the same accessible name.
    const remoteLink = screen.getByText(DETAIL.remoteOriginUrl!);
    remoteLink.focus();
    expect(document.activeElement).toBe(remoteLink);

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

describe("RepoDetailPanel homepage glyph (N4)", () => {
  it("is hidden when homepage is null", async () => {
    renderPanel();
    await screen.findByText("Up to date with origin");
    expect(screen.queryByRole("button", { name: "Open homepage" })).toBeNull();
  });

  it("opens the homepage via repoOpenHomepage when set", async () => {
    mockCommand(commands, "repoGet", async () => ok({ ...DETAIL, homepage: "https://example.com" }));
    const openHomepage = mockCommand(commands, "repoOpenHomepage", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

    const button = await screen.findByRole("button", { name: "Open homepage" });
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

    const button = await screen.findByRole("button", { name: "Open homepage" });
    await user.click(button);

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith(
        "error",
        "Action failed",
        "the homepage URL is not a web address RepoSync will open",
      ),
    );
  });
});
