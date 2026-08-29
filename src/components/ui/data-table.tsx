import type { KeyboardEvent, ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Shared table primitive (N2, ui-delivery-plan.md ledger B5), built to the
 * table lab's settled values (`_local/gui/2026-08-28_iterations/README.md` and
 * `_generators/gen_lab2.py`). Presentational only: no sorting, no selection, no
 * column show/hide (all deliberately deferred, ledger C).
 *
 * Extracted lab defaults this file is built to (all of them stated in the N2
 * PR body for veto):
 *   - data icons: on, skipped on an empty cell
 *   - number alignment: label left in the header, value right in the cell
 *   - column lines: solid, 1px, `var(--border)`, on every column except the
 *     frozen column, the flex filler and the actions column; header cells
 *     carry the same rule ("header: match", not "clean")
 *   - cell padding: 12px, one value, every column, every density
 *   - row height: 52px is the lab's own default ("full" density below).
 *     "compact" (44px) is NOT lab-sourced - the lab exposes one continuous
 *     row-height slider (default 52, range 36-72), not two named densities.
 *     44px is the other figure named in the N2 brief; kept as the second
 *     density and flagged in the PR for veto.
 *   - header height: 32px, fixed in the lab's CSS, not a slider; unaffected by
 *     density. (The brief also named ~28px, which traces to a SUPERSEDED
 *     round, `2026-08-27_06_row-standard-v3.html`, not the ratified lab.)
 *   - repo column: minmax(180px, 240px), the only flexible column
 *   - zebra striping: 0% (off) by default; not implemented here since no
 *     screen has asked for it
 *
 * Known CSS traps this file guards against (see AGENTS.md / the N2 task):
 *   - `position: sticky` silently does nothing without an inset, so every
 *     sticky element here carries an explicit inline `top`/`left`/`right` -
 *     inline rather than a Tailwind class, so a jsdom test can read
 *     `style.position` / `style.left` etc. directly without a stylesheet.
 *   - grid items default to `min-width: auto`, which lets the table grow past
 *     the viewport and clip instead of scrolling; every cell forces
 *     `minWidth: 0` inline for the same testability reason.
 *   - `align-items: center` on the ROW (grid container) shrinks each cell to
 *     its content height, so a column rule (`border-left`) renders as a short
 *     floating segment instead of spanning the row. The row stays at the grid
 *     default (stretch); only the CONTENT inside each cell centers itself.
 */

export type DataTableDensity = "full" | "compact";
export type DataTableAlign = "left" | "right";

const ROW_HEIGHT: Record<DataTableDensity, number> = { full: 52, compact: 44 };
const HEADER_HEIGHT = 32;
const CELL_PADDING = 12;
/**
 * The lab's actions column is 64px, sized for its own bespoke 28px icon
 * button. This primitive reuses the house `Button` (`size="icon"`, 36px) for
 * Check now, the same component `repo-detail.tsx` already uses for the
 * identical action, rather than a new bespoke smaller button just to hit the
 * lab's pixel figure. 96px is the adjusted width for that button plus a
 * chevron at the existing cell padding.
 */
const DEFAULT_ACTIONS_WIDTH = "96px";

export interface DataTableColumn<T> {
  /** Stable key, used for the React list key and nothing else. */
  id: string;
  /** Never wraps; always left-aligned regardless of `align` (ratified). */
  header: string;
  /** A CSS grid track for this column, e.g. "124px" or "minmax(180px,240px)". */
  width: string;
  /** Content alignment. Headers stay left; only the cell value follows this. */
  align?: DataTableAlign;
  /** The sticky, frozen-on-scroll column. At most one; must be first. */
  frozen?: boolean;
  /**
   * A small lucide glyph rendered before the cell's value, in the lab's muted
   * data-icon style. Automatically skipped when `cell` returns an empty value
   * (null/undefined), so a column never fills with floating glyphs next to
   * dashes.
   */
  icon?: LucideIcon;
  /** Cell content. Return null/undefined to render the muted dash placeholder. */
  cell: (row: T) => ReactNode;
}

export interface DataTableProps<T> {
  columns: DataTableColumn<T>[];
  rows: T[];
  rowKey: (row: T) => string | number;
  /** Defaults to "full" (52px rows). Neither density is wired to a screen
   *  control yet; both are exercised only by this component's own tests. */
  density?: DataTableDensity;
  onRowClick?: (row: T) => void;
  /** A trailing, unlabeled, sticky-right column (Check now, chevron, etc). */
  actions?: (row: T) => ReactNode;
  actionsWidth?: string;
  className?: string;
  /** Accessible label for the table region, e.g. "Tracked repositories". */
  "aria-label"?: string;
}

export function DataTable<T>({
  columns,
  rows,
  rowKey,
  density = "full",
  onRowClick,
  actions,
  actionsWidth = DEFAULT_ACTIONS_WIDTH,
  className,
  "aria-label": ariaLabel,
}: DataTableProps<T>) {
  const rowHeight = ROW_HEIGHT[density];
  const template =
    columns.map((c) => c.width).join(" ") + " 1fr" + (actions ? ` ${actionsWidth}` : "");

  return (
    <div
      className={cn("min-w-0 overflow-hidden rounded-xl border border-border bg-card shadow-sm", className)}
      role="table"
      aria-label={ariaLabel}
    >
      {/* Horizontal scroll lives HERE, inside the table container, never on the page. */}
      <div className="min-w-0 overflow-x-auto">
        <div className="min-w-max">
          <div
            role="row"
            className="grid border-b border-border bg-muted"
            style={{ gridTemplateColumns: template, height: HEADER_HEIGHT, position: "sticky", top: 0, zIndex: 3 }}
          >
            {columns.map((col, i) => (
              <div
                key={col.id}
                role="columnheader"
                className={cn(
                  "flex min-w-0 items-center overflow-hidden bg-muted font-mono text-[11px] font-bold tracking-wider text-muted-foreground uppercase whitespace-nowrap",
                  i > 0 && "border-l border-border",
                  col.frozen && "z-[4]",
                )}
                style={{
                  padding: `0 ${CELL_PADDING}px`,
                  minWidth: 0,
                  ...(col.frozen ? { position: "sticky", left: 0, zIndex: 4 } : {}),
                }}
              >
                {col.header}
              </div>
            ))}
            <div aria-hidden />
            {actions && (
              <div
                className="z-[4] bg-muted"
                style={{ position: "sticky", right: 0, zIndex: 4, minWidth: 0 }}
              />
            )}
          </div>

          {rows.map((row) => {
            const key = rowKey(row);
            const clickable = onRowClick !== undefined;
            const handleKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
              if (!clickable) return;
              if (e.key === "Enter") {
                onRowClick(row);
              } else if (e.key === " ") {
                e.preventDefault();
                onRowClick(row);
              }
            };
            return (
              <div
                key={key}
                role={clickable ? "button" : "row"}
                tabIndex={clickable ? 0 : undefined}
                onClick={clickable ? () => onRowClick(row) : undefined}
                onKeyDown={handleKeyDown}
                className={cn(
                  "group grid border-b border-border last:border-b-0",
                  clickable &&
                    "cursor-pointer hover:bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                )}
                style={{ gridTemplateColumns: template, minHeight: rowHeight }}
              >
                {columns.map((col, i) => {
                  const content = col.cell(row);
                  const empty = content === null || content === undefined;
                  const Icon = col.icon;
                  return (
                    <div
                      key={col.id}
                      role="cell"
                      className={cn(
                        "flex min-w-0 items-center overflow-hidden bg-card text-sm",
                        i > 0 && "border-l border-border",
                        col.align === "right" && !empty ? "justify-end text-right" : "justify-start text-left",
                        col.frozen && "group-hover:bg-muted",
                      )}
                      style={{
                        padding: `0 ${CELL_PADDING}px`,
                        minWidth: 0,
                        ...(col.frozen ? { position: "sticky", left: 0, zIndex: 2 } : {}),
                      }}
                    >
                      {empty ? (
                        <span className="text-muted-foreground/55">-</span>
                      ) : (
                        <>
                          {Icon && (
                            <Icon
                              aria-hidden
                              className="mr-1.5 size-[11px] shrink-0 text-muted-foreground/75"
                            />
                          )}
                          {content}
                        </>
                      )}
                    </div>
                  );
                })}
                <div aria-hidden className="min-w-0 bg-card group-hover:bg-muted" />
                {actions && (
                  <div
                    className="flex items-center justify-end gap-1 bg-card group-hover:bg-muted"
                    style={{ padding: `0 ${CELL_PADDING}px`, position: "sticky", right: 0, zIndex: 2, minWidth: 0 }}
                    onClick={(e) => e.stopPropagation()}
                  >
                    {actions(row)}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
