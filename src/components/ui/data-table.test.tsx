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
    const fullRow = full.querySelectorAll('[role="row"]')[1] as HTMLElement; // [0] is the header row
    expect(fullRow.style.minHeight).toBe("52px");

    cleanup();

    const { container: compact } = render(
      <DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} density="compact" />,
    );
    const compactRow = compact.querySelectorAll('[role="row"]')[1] as HTMLElement;
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
    const bodyRow = container.querySelectorAll('[role="row"]')[1] as HTMLElement;
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

  it("a row is role=row with no tabIndex or button role - mouse-only, no keyboard entry point of its own", () => {
    // Fix round after the Codex review of PR #73, finding 2: a keyboard-
    // operable row (`role="button"`, `tabIndex`) wrapped around further
    // focusable controls (Check now, the actions chevron) is an invalid
    // accessibility tree with an ambiguous Enter/Space target. The row is
    // mouse-only now; the caller's own `actions` content is the keyboard path.
    const { container } = render(
      <DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} onRowClick={vi.fn()} />,
    );
    const dataRow = container.querySelectorAll('[role="row"]')[1] as HTMLElement;
    expect(dataRow.getAttribute("role")).toBe("row");
    expect(dataRow.hasAttribute("tabindex")).toBe(false);
  });

  it("clicking inside the actions cell does not also fire onRowClick (stopPropagation)", async () => {
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

    await user.click(screen.getAllByRole("button", { name: "Check" })[0]);

    expect(onCheck).toHaveBeenCalledTimes(1);
    expect(onRowClick).not.toHaveBeenCalled();
  });

  it("has a valid table accessibility tree: table > two rowgroups (header, body) > row > columnheader/cell", () => {
    render(
      <DataTable
        columns={columns()}
        rows={ROWS}
        rowKey={(r) => r.id}
        actions={() => <button>Check</button>}
      />,
    );

    const table = screen.getByRole("table");
    const rowgroups = screen.getAllByRole("rowgroup");
    expect(rowgroups).toHaveLength(2);
    expect(table.contains(rowgroups[0])).toBe(true);
    expect(table.contains(rowgroups[1])).toBe(true);
    // Header rowgroup holds exactly one row of columnheaders; body rowgroup
    // holds one row per data row, each with cells (including the actions
    // cell, also role="cell", never a bare unlabeled columnheader).
    const [headerGroup, bodyGroup] = rowgroups;
    expect(headerGroup.querySelectorAll('[role="row"]')).toHaveLength(1);
    expect(headerGroup.querySelectorAll('[role="columnheader"]')).toHaveLength(columns().length);
    expect(bodyGroup.querySelectorAll('[role="row"]')).toHaveLength(ROWS.length);
  });

  it("the table root and its scroll region are height-bounded (min-h-0/max-h-full), not forced to fill (flex-1)", () => {
    // Scroll-ownership fix (finding 1): the table caps its OWN height against
    // whatever bounded ancestor a caller opts into (`max-h-full`), rather than
    // being forced to fill available space (`flex-1`), so a short table does
    // not drag an empty border box to the floor. `min-h-0` on both the root
    // and the inner scroller is what lets either one actually shrink below
    // its content size when a caller's ancestor IS bounded - the flex
    // "min-height: auto" trap on the other axis from the grid "min-width:
    // auto" trap this file already guards against.
    const { container } = render(<DataTable columns={columns()} rows={ROWS} rowKey={(r) => r.id} />);
    const root = screen.getByRole("table");
    const rootTokens = root.className.split(/\s+/);
    expect(rootTokens).toContain("max-h-full");
    expect(rootTokens).toContain("min-h-0");
    expect(rootTokens).not.toContain("flex-1");
    // Exact-token check, not a substring match: "max-h-full" legitimately
    // contains the substring "h-full", but the root must not ALSO carry the
    // standalone "h-full" utility (which would force it to fill its
    // container instead of merely capping at it).
    expect(rootTokens).not.toContain("h-full");

    const scroller = container.querySelector(".overflow-auto") as HTMLElement;
    expect(scroller).not.toBeNull();
    expect(scroller.className).toMatch(/\bmin-h-0\b/);
    expect(scroller.className).toMatch(/\boverflow-auto\b/);
    // Never split per-axis: overflow-x-auto alone (the first cut's bug) makes
    // the wrapper itself the sticky header's scrollport on BOTH axes, since
    // any non-visible overflow value on either axis makes an element a
    // scroll container - there is no such thing as scrolling horizontally
    // against one ancestor and vertically against another.
    expect(scroller.className).not.toMatch(/\boverflow-x-auto\b/);
  });
});
