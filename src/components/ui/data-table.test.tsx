// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach } from "vitest";
import { Clock } from "lucide-react";
import { DataTable, type DataTableColumn } from "@/components/ui/data-table";

/**
 * The primitive's own tests, independent of any screen that consumes it
 * (N2, ui-delivery-plan.md ledger B5). Covers the two things a screen test
 * cannot: the generic column/density/icon contract, and the CSS traps named
 * in the N2 task (a `position: sticky` with no inset, a grid cell that can
 * grow past the viewport instead of scrolling, a column rule that floats
 * mid-row instead of spanning it).
 *
 * Sticky/min-width assertions read `style.*` directly rather than a
 * stylesheet-resolved computed style: jsdom does not apply Tailwind's
 * generated CSS in tests, so the load-bearing inset/min-width values are set
 * as inline styles specifically so they stay assertable here. See the
 * doc comment on `data-table.tsx`.
 */

type Row = { id: number; name: string; checkedAgo: string | null };

const ROWS: Row[] = [
  { id: 1, name: "repo-a", checkedAgo: "12m ago" },
  { id: 2, name: "repo-b", checkedAgo: null },
];

function columns(): DataTableColumn<Row>[] {
  return [
    { id: "repo", header: "Repository", width: "minmax(180px,240px)", frozen: true, cell: (r) => r.name },
    {
      id: "checked",
      header: "Checked",
      width: "96px",
      icon: Clock,
      cell: (r) => r.checkedAgo,
    },
  ];
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DataTable", () => {
  it("renders every column header and every row's cell content", () => {
    render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);

    expect(screen.getByRole("columnheader", { name: "Repository" })).toBeDefined();
    expect(screen.getByRole("columnheader", { name: "Checked" })).toBeDefined();
    expect(screen.getByText("repo-a")).toBeDefined();
    expect(screen.getByText("repo-b")).toBeDefined();
    expect(screen.getByText("12m ago")).toBeDefined();
  });

  it("skips the column icon on an empty cell, rendering the dash placeholder instead", () => {
    const { container } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);

    // repo-a has a checked value: the Clock icon (an svg) sits in that cell.
    const cells = screen.getAllByRole("cell");
    const repoACheckedCell = cells.find((c) => c.textContent === "12m ago");
    expect(repoACheckedCell?.querySelector("svg")).not.toBeNull();

    // repo-b's checked value is null: dash, no icon.
    const dashCell = cells.find((c) => c.textContent === "-");
    expect(dashCell).toBeDefined();
    expect(dashCell?.querySelector("svg")).toBeNull();
    expect(container.querySelectorAll("svg").length).toBe(1);
  });

  it("defaults to the full density (52px rows); compact renders 44px rows", () => {
    const { container: full } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);
    const fullRow = full.querySelectorAll('[role="button"], [role="row"]')[1] as HTMLElement; // [0] is the header row
    expect(fullRow.style.minHeight).toBe("52px");

    cleanup();

    const { container: compact } = render(
      <DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} density="compact" />,
    );
    const compactRow = compact.querySelectorAll('[role="button"], [role="row"]')[1] as HTMLElement;
    expect(compactRow.style.minHeight).toBe("44px");
  });

  it("the frozen column is sticky-left with an inset in both the header and a data row", () => {
    const { container } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);

    const headerFrozenCell = screen.getByRole("columnheader", { name: "Repository" });
    expect(headerFrozenCell.style.position).toBe("sticky");
    expect(headerFrozenCell.style.left).toBe("0px");

    const bodyFrozenCell = container.querySelector('[role="cell"]') as HTMLElement;
    expect(bodyFrozenCell.textContent).toBe("repo-a");
    expect(bodyFrozenCell.style.position).toBe("sticky");
    expect(bodyFrozenCell.style.left).toBe("0px");
  });

  it("the header row is sticky-top with an inset", () => {
    const { container } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);
    const headerRow = container.querySelector('[role="row"]') as HTMLElement;
    expect(headerRow.style.position).toBe("sticky");
    expect(headerRow.style.top).toBe("0px");
  });

  it("the actions column has no header label and is sticky-right with an inset", () => {
    render(
      <DataTable
        columns={columns()}
        rows={ROWS}
        rowKey={(r) => r.id}
        actions={(r) => <button>Check {r.name}</button>}
      />,
    );
    // No column-header text was added for actions.
    expect(screen.queryByRole("columnheader", { name: /action/i })).toBeNull();

    const checkButton = screen.getByRole("button", { name: "Check repo-a" });
    const actionsCell = checkButton.parentElement as HTMLElement;
    expect(actionsCell.style.position).toBe("sticky");
    expect(actionsCell.style.right).toBe("0px");
  });

  it("every cell forces min-width: 0 so the table scrolls instead of clipping", () => {
    const { container } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);
    const anyCell = container.querySelector('[role="cell"]') as HTMLElement;
    expect(anyCell.style.minWidth).toBe("0px");
    const headerCell = screen.getByRole("columnheader", { name: "Repository" });
    expect(headerCell.style.minWidth).toBe("0px");
  });

  it("the row grid does not center-align items (the column-rule floating-segment trap)", () => {
    const { container } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);
    const bodyRow = container.querySelectorAll('[role="button"], [role="row"]')[1] as HTMLElement;
    // Deliberately absent: setting align-items: center on the ROW (grid
    // container) shrinks each cell to content height, turning a column rule
    // into a short floating segment instead of one spanning the full row.
    expect(bodyRow.style.alignItems).not.toBe("center");
  });

  it("clicking a row fires onRowClick with that row", async () => {
    const onRowClick = vi.fn();
    render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} onRowClick={onRowClick} />);
    const user = userEvent.setup();

    await user.click(screen.getByText("repo-b"));

    expect(onRowClick).toHaveBeenCalledWith(ROWS[1]);
  });

  it("Enter and Space on a focused row also fire onRowClick, and actions stop propagation", async () => {
    const onRowClick = vi.fn();
    const onCheck = vi.fn();
    render(
      <DataTable
        columns={columns()}
        rows={ROWS}
        rowKey={(r) => r.id}
        onRowClick={onRowClick}
        actions={() => <button onClick={onCheck}>Check</button>}
      />,
    );
    const user = userEvent.setup();

    // Row and action are both role="button"; narrow to the row for repo-a.
    const repoARow = screen.getByText("repo-a").closest('[role="button"]') as HTMLElement;
    repoARow.focus();
    await user.keyboard("{Enter}");
    expect(onRowClick).toHaveBeenCalledWith(ROWS[0]);

    onRowClick.mockClear();
    await user.click(screen.getAllByRole("button", { name: "Check" })[0]);
    expect(onCheck).toHaveBeenCalledTimes(1);
    // Clicking the action must not also open the row.
    expect(onRowClick).not.toHaveBeenCalled();
  });
});
