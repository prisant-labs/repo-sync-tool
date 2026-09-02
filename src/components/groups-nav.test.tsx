// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { commands } from "@/lib/bindings";
import type { GroupSummary } from "@/lib/bindings";
import { err, mockCommand, ok } from "@/test/mock-ipc";
import { ToastContext } from "@/hooks/use-toast";
import { GroupsNav } from "@/components/groups-nav";

/**
 * N5 (sidebar restructure and toolbar consolidation; ui-delivery-plan.md
 * ledger B1): the sidebar Groups section moved to
 * nest one level beneath Repos in `app-shell.tsx`, but every shipped
 * behaviour (coverage-matrix.md section 2) must survive that move unchanged.
 * These tests pin the INTERACTION contract (header, empty state, the "All
 * repositories" row, hover/focus-revealed rename+delete, the inline "Delete?"
 * confirm, and rename/recolor reaching `GroupDialog`).
 *
 * The rename/delete icons are always present in the DOM - only their opacity
 * is conditional on `:hover`/`:focus-within` (group-hover/group-focus-within
 * classes in groups-nav.tsx). That visual swap is pure CSS jsdom cannot
 * verify (and asserting on the opacity classes would be asserting on DOM
 * shape, not meaning); it is covered by the real-browser Playwright pass
 * instead. What jsdom CAN and does pin here is that the affordances exist and
 * that clicking them (regardless of hover) fires the right IPC calls.
 */

const GROUPS: GroupSummary[] = [
  { id: 1, name: "Work", color: "#4477ff", repoCount: 3 },
  { id: 2, name: "Forks", color: null, repoCount: 0 },
];

// `railActive` defaults to true (the "we are on Repos" case) so every
// existing interaction test below exercises the same visual path it always
// did; the two tests specifically about the destination/filter split pass
// `railActive: false` explicitly.
function renderNav(
  groups: GroupSummary[],
  activeGroupId: number | null = null,
  options: { railActive?: boolean } = {},
) {
  const toast = vi.fn();
  const onSelectGroup = vi.fn();
  const onClearActiveGroup = vi.fn();
  const refetchGroups = vi.fn();
  const view = render(
    <ToastContext.Provider value={toast}>
      <GroupsNav
        groups={groups}
        activeGroupId={activeGroupId}
        railActive={options.railActive ?? true}
        onSelectGroup={onSelectGroup}
        onClearActiveGroup={onClearActiveGroup}
        refetchGroups={refetchGroups}
      />
    </ToastContext.Provider>,
  );
  return { toast, onSelectGroup, onClearActiveGroup, refetchGroups, ...view };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("GroupsNav", () => {
  it("renders the Groups header with a New group button", () => {
    renderNav(GROUPS);
    expect(screen.getByText("Groups")).toBeDefined();
    expect(screen.getByRole("button", { name: "New group" })).toBeDefined();
  });

  it("New group opens GroupDialog in create mode", async () => {
    renderNav(GROUPS);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "New group" }));

    expect(await screen.findByRole("heading", { name: "New group" })).toBeDefined();
    // BL-NI-95: the dialog names itself from that same visible heading via
    // aria-labelledby, mirroring the Drawer fix from PR #77.
    expect(screen.getByRole("dialog", { name: "New group" })).toBeDefined();
  });

  it("shows the empty state and no rows when there are no groups", () => {
    renderNav([]);
    expect(screen.getByText("No groups yet. Create one to organize your repos.")).toBeDefined();
    expect(screen.queryByText("All repositories")).toBeNull();
  });

  it("the 'All repositories' clear row selects the null group (clearing, not a group id)", async () => {
    const { onSelectGroup } = renderNav(GROUPS);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "All repositories" }));

    expect(onSelectGroup).toHaveBeenCalledWith(null);
  });

  it("renders each group's colour dot, name and member count, honouring a null colour with the muted fallback", () => {
    renderNav(GROUPS);

    const workRow = screen.getByText("Work").closest("div") as HTMLElement;
    expect(within(workRow).getByText("3")).toBeDefined();
    const dot = workRow.querySelector("span[style]") as HTMLElement;
    expect(dot.style.backgroundColor).toBe("rgb(68, 119, 255)");

    const forksRow = screen.getByText("Forks").closest("div") as HTMLElement;
    expect(within(forksRow).getByText("0")).toBeDefined();
    // Null colour: no inline style, falls back to the muted dot class.
    const forksDot = forksRow.querySelector("span:not([style])") as HTMLElement;
    expect(forksDot.className).toContain("bg-muted-foreground/50");
  });

  it("selecting a group row calls onSelectGroup with its id", async () => {
    const { onSelectGroup } = renderNav(GROUPS);
    const user = userEvent.setup();

    // The row's own select button is the accessible name "Work" (the count
    // and the hover-revealed icons are separate buttons alongside it).
    await user.click(screen.getByRole("button", { name: "Work" }));

    expect(onSelectGroup).toHaveBeenCalledWith(1);
  });

  it("rename and delete affordances are always in the DOM (opacity-swapped on hover/focus, not conditionally mounted)", () => {
    renderNav(GROUPS);
    // No hover simulated at all - both icons must already be queryable.
    expect(screen.getByRole("button", { name: "Rename Work" })).toBeDefined();
    expect(screen.getByRole("button", { name: "Delete Work" })).toBeDefined();
  });

  it("Rename opens GroupDialog in edit mode, pre-seeded with the group's own name", async () => {
    renderNav(GROUPS);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Rename Work" }));

    expect(await screen.findByRole("heading", { name: "Edit group" })).toBeDefined();
    expect(screen.getByDisplayValue("Work")).toBeDefined();
    // BL-NI-95: same aria-labelledby wiring in the dialog's other mode.
    expect(screen.getByRole("dialog", { name: "Edit group" })).toBeDefined();
  });

  it("Delete shows an inline 'Delete?' confirm, not a modal; Cancel dismisses it without calling groupDelete", async () => {
    const groupDelete = mockCommand(commands, "groupDelete", async () => ok(null));
    renderNav(GROUPS);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Delete Work" }));
    expect(screen.getByText("Delete?")).toBeDefined();
    // Still an inline row, not a dialog: no role="dialog" appeared for this.
    expect(screen.queryByRole("dialog", { name: /delete/i })).toBeNull();

    await user.click(screen.getByRole("button", { name: "Cancel delete" }));

    expect(screen.queryByText("Delete?")).toBeNull();
    expect(groupDelete).not.toHaveBeenCalled();
  });

  it("Confirming delete calls groupDelete and refetches the group list", async () => {
    const groupDelete = mockCommand(commands, "groupDelete", async () => ok(null));
    const { refetchGroups, onClearActiveGroup } = renderNav(GROUPS, null);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Delete Work" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));

    await waitFor(() => expect(groupDelete).toHaveBeenCalledWith(1));
    expect(refetchGroups).toHaveBeenCalled();
    // Work (id 1) was not the active filter in this render, so the
    // navigation-free clear helper must NOT fire for an unrelated delete.
    expect(onClearActiveGroup).not.toHaveBeenCalled();
    expect(screen.queryByText("Delete?")).toBeNull();
  });

  it("a FAILED delete of the active group's filter reports the failure and does not clear it (E-16 (groups and tags) known defect 6 governs only a successful delete)", async () => {
    // db.locked (AppError::DbLocked) is a real, plausible failure for this
    // write - the taxonomy's `group_delete` is otherwise idempotent (a
    // missing id is not an error; see crates/reposync-core/src/store.rs),
    // so a genuine failure here is a transient DB condition, not a 404.
    mockCommand(commands, "groupDelete", async () => err("db.locked", "the database is locked"));
    const { onClearActiveGroup, toast } = renderNav(GROUPS, 1);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Delete Work" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));

    await waitFor(() =>
      expect(toast).toHaveBeenCalledWith("error", "Could not delete group", "the database is locked"),
    );
    // The delete failed: clearing the filter for a group that still exists
    // would be wrong, so the navigation-free clear must not have fired.
    expect(onClearActiveGroup).not.toHaveBeenCalled();
  });

  it("a successful delete of the active group clears the filter without navigating (no view/navigation prop exists on this component to misuse)", async () => {
    mockCommand(commands, "groupDelete", async () => ok(null));
    const { onClearActiveGroup, onSelectGroup } = renderNav(GROUPS, 1);
    const user = userEvent.setup();

    await user.click(screen.getByRole("button", { name: "Delete Work" }));
    await user.click(screen.getByRole("button", { name: "Confirm delete" }));

    await waitFor(() => expect(onClearActiveGroup).toHaveBeenCalledTimes(1));
    // The navigating callback is never touched by delete - only the
    // navigation-free one is, which is the whole point of the distinction.
    expect(onSelectGroup).not.toHaveBeenCalled();
  });

  // Codex adversarial review finding 2: destination state (which screen is
  // current) and filter state (which group is applied) were conflated - the
  // group row's accent fill painted on every screen, not only Repos, so a
  // sighted user saw two "current" things in one rail at once. The fix
  // separates them: `aria-pressed` (the filter's own, real, persistent
  // state) is unaffected by `railActive`; only the visual fill is gated.
  it("aria-pressed reflects the true filter state regardless of railActive - the filter persists even where its accent fill does not paint", () => {
    renderNav(GROUPS, 1, { railActive: false });

    // Work (id 1) IS the active filter even though this render represents a
    // screen other than Repos (railActive: false) - the semantic state must
    // say so for assistive tech, independent of whether the accent fill painted.
    expect(screen.getByRole("button", { name: "Work" }).getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByRole("button", { name: "Forks" }).getAttribute("aria-pressed")).toBe("false");
    expect(screen.getByRole("button", { name: "All repositories" }).getAttribute("aria-pressed")).toBe(
      "false",
    );
  });

  it("aria-pressed on 'All repositories' is true exactly when no group is the active filter, on every screen", () => {
    renderNav(GROUPS, null, { railActive: false });

    expect(screen.getByRole("button", { name: "All repositories" }).getAttribute("aria-pressed")).toBe(
      "true",
    );
    expect(screen.getByRole("button", { name: "Work" }).getAttribute("aria-pressed")).toBe("false");
  });
});
