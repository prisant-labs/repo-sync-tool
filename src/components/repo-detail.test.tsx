// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands, events } from "@/lib/bindings";
import type { RepoDetail, Settings } from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { ToastContext } from "@/hooks/use-toast";
import { RepoDetailPanel } from "@/components/repo-detail";

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

/** The armed-state confirm button, distinct from the "Remove from RepoSync" trigger. */
function confirmButton() {
  return screen.getByRole("button", { name: "Remove" });
}

describe("RepoDetailPanel remove", () => {
  it("discloses the full consequence up front, and the first click only arms the confirm", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

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
