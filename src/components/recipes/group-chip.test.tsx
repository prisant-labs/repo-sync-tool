// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { GroupChip } from "@/components/recipes/group-chip";
import type { GroupSummary } from "@/lib/bindings";

afterEach(cleanup);

const work: GroupSummary = { id: 1, name: "work", color: "#3b82f6", repoCount: 3 };
const plain: GroupSummary = { id: 2, name: "misc", color: null, repoCount: 0 };

describe("GroupChip", () => {
  it("shows the group name in the compact variant, with no button to press", () => {
    render(<GroupChip variant="compact" group={work} />);
    expect(screen.getByText("work")).toBeTruthy();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("shows the group name and a named remove button in the removable variant", () => {
    render(<GroupChip variant="removable" group={work} busy={false} onRemove={() => {}} />);
    expect(screen.getByText("work")).toBeTruthy();
    expect(screen.getByRole("button", { name: "Remove from work" })).toBeTruthy();
  });

  it("calls onRemove when the remove button is clicked", () => {
    const onRemove = vi.fn();
    render(<GroupChip variant="removable" group={work} busy={false} onRemove={onRemove} />);
    fireEvent.click(screen.getByRole("button", { name: "Remove from work" }));
    expect(onRemove).toHaveBeenCalledTimes(1);
  });

  it("disables the remove button while busy", () => {
    const onRemove = vi.fn();
    render(<GroupChip variant="removable" group={work} busy onRemove={onRemove} />);
    const button = screen.getByRole("button", { name: "Remove from work" }) as HTMLButtonElement;
    expect(button.disabled).toBe(true);
    fireEvent.click(button);
    expect(onRemove).not.toHaveBeenCalled();
  });

  it("renders a group with no colour in both variants", () => {
    render(
      <>
        <GroupChip variant="compact" group={plain} />
        <GroupChip variant="removable" group={plain} busy={false} onRemove={() => {}} />
      </>,
    );
    expect(screen.getAllByText("misc")).toHaveLength(2);
  });
});
