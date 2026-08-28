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
 * The two states these tests exist to keep distinguishable (BL-NI-85):
 *
 * 1. ARMED is not REMOVED. The first click on "Remove from RepoSync" must never
 *    reach the backend; only the explicit confirm may. Removal clears the repo's
 *    check history irrecoverably, so collapsing the two clicks into one is the
 *    regression that matters most here.
 * 2. FAILED is not DONE. A removal that errors must leave the drawer open and
 *    say so; closing the drawer is the success signal, so closing on failure
 *    would read as "removed" about a repo that is still tracked.
 *
 * Assertions are about rendered meaning and IPC traffic, not markup, so a
 * restyle of the section should leave them untouched.
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
  render(
    <ToastContext.Provider value={toast}>
      <RepoDetailPanel id={7} onChanged={onChanged} onClose={onClose} />
    </ToastContext.Provider>,
  );
  return { onChanged, onClose, toast };
}

describe("RepoDetailPanel remove", () => {
  it("states the consequence before any click, and the first click only arms the confirm", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

    const arm = await screen.findByRole("button", { name: "Remove from RepoSync" });
    // The consequence copy is visible up front, not revealed by the confirm.
    expect(screen.getByText(/not touched/i)).toBeDefined();

    await user.click(arm);

    expect(remove).not.toHaveBeenCalled();
    expect(screen.getByText(/cannot be recovered/i)).toBeDefined();
    expect(screen.getByRole("button", { name: "Remove" })).toBeDefined();
  });

  it("cancel disarms without ever calling the backend", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(screen.getByRole("button", { name: "Cancel" }));

    expect(remove).not.toHaveBeenCalled();
    expect(screen.queryByText(/cannot be recovered/i)).toBeNull();
    expect(screen.getByRole("button", { name: "Remove from RepoSync" })).toBeDefined();
  });

  it("confirm removes the right repo, then closes the drawer and refreshes the list", async () => {
    const remove = mockCommand(commands, "repoRemove", async () => ok(null));
    const { onChanged, onClose } = renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(screen.getByRole("button", { name: "Remove" }));

    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(remove).toHaveBeenCalledTimes(1);
    expect(remove).toHaveBeenCalledWith(7);
    expect(onChanged).toHaveBeenCalled();
  });

  it("a failed removal reports the error and keeps the drawer open", async () => {
    mockCommand(commands, "repoRemove", async () => err("not_found", "repo 7 was not found"));
    const { onClose, toast } = renderPanel();
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Remove from RepoSync" }));
    await user.click(screen.getByRole("button", { name: "Remove" }));

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith("error", "Could not remove", "repo 7 was not found"),
    );
    expect(onClose).not.toHaveBeenCalled();
    // The repo is still tracked, so the drawer still shows it.
    expect(screen.getByText(/not touched/i)).toBeDefined();
  });
});
