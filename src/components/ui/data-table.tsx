import type { ReactNode } from "react";
import type { LucideIcon } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Shared table primitive (N2, ui-delivery-plan.md ledger B5), built to the
 * table lab's settled values (`_local/gui/2026-08-28_iterations/README.md` and
 * `_generators/gen_lab2.py`). Presentational only: no sorting, no interactive
 * row/column selection, no column show/hide (all deliberately deferred,
 * ledger C).
 *
 * `currentKey` (D2, restoring a highlight the pre-DataTable Activity screen
 * had) is NOT that deferred selection model: it does not let the table track
 * or emit which row a user picked, it only lets a caller that already knows
 * which row it has open elsewhere (a drawer, a receipt) mark that one row.
 * See the prop's own doc comment.
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
 *
 * Scroll ownership (fix round after the Codex review of PR #73, finding 1).
 * The first cut put a sticky `top:0` header inside an `overflow-x: auto`
 * wrapper, on the assumption the header would pin against the page's OWN
 * vertical scroller (`AppShell`'s `overflow-auto` element, per
 * `page-shell.tsx`'s doc comment). That assumption was wrong on plain CSS
 * grounds: ANY non-`visible` overflow value on EITHER axis makes an element a
 * scroll container, and `position: sticky` always pins to the NEAREST scroll
 * container ancestor - there is no per-axis split where a header sticks
 * horizontally to one ancestor and vertically to another. `overflow-x: auto`
 * therefore made the wrapper itself the header's scrollport, and since that
 * wrapper never scrolls vertically (nothing bounds its height), the header
 * never pinned; it rode away with the page like any other content.
 *
 * The fix restores the table lab's OWN architecture instead: the table owns
 * BOTH axes of scrolling internally, in one `overflow-auto` element, whose
 * height is capped by `max-h-full` against whatever bounded-height ancestor
 * the caller provides (a plain block ancestor with no defined height makes
 * `max-height: 100%` compute to `none` per the CSS spec, so this degrades
 * to ordinary content-driven growth and page-level scroll for any caller
 * that does NOT opt into a bounded layout - safe by construction, not by
 * convention). `min-h-0` on both the root and the inner scroller overrides
 * flexbox's default `min-height: auto`, which would otherwise refuse to
 * shrink either box below its content size and push the overflow onto
 * whatever wraps it instead of scrolling internally - the same "min-width:
 * auto" grid trap this file already guards against, on the other axis.
 */

export type DataTableDensity = "full" | "compact";
export type DataTableAlign = "left" | "right";

const ROW_HEIGHT: Record<DataTableDensity, number> = { full: 52, compact: 44 };
const HEADER_HEIGHT = 32;
const CELL_PADDING = 12;
/**
 * The lab's actions column is 64px, sized for its own bespoke 28px icon
 * button plus a small decorative chevron glyph. This primitive reuses the
 * house `Button` (`size="icon"`, 36px square) for BOTH Check now and the
 * details chevron - the chevron became a real focusable button in the fix
 * round after the Codex review of PR #73 (finding 2: a decorative icon
 * inside a keyboard-operable row left no valid keyboard path once the row
 * itself stopped being one) - rather than a bespoke smaller button just to
 * hit the lab's pixel figure. 112px fits two 36px buttons, the `gap-1`
 * between them, and the existing 12px cell padding on both sides
 * (12 + 36 + 4 + 36 + 12 = 100, plus a small margin).
 */
const DEFAULT_ACTIONS_WIDTH = "112px";

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
   * dashes. Leave unset for a column whose own cell renderer draws a bespoke,
   * differently-styled icon (e.g. Folder's in-cell glyph) - `icon` is only the
   * generic muted data-icon slot.
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
  /**
   * Mouse convenience only: clicking anywhere on a row (outside the actions
   * cell, which stops propagation) calls this. Deliberately NOT a keyboard
   * entry point (no `tabIndex`, no row `role="button"`, no row `onKeyDown`) -
   * nesting a focusable, keyboard-operable row around further focusable
   * controls (Check now, the actions chevron) produced an invalid
   * accessibility tree and an ambiguous Enter/Space target (Codex review of
   * PR #73, finding 2). The caller's own `actions` content is the real
   * keyboard path into whatever a row click would have done (see
   * `repos.tsx`'s chevron button).
   */
  onRowClick?: (row: T) => void;
  /** A trailing, unlabeled, sticky-right column (Check now, chevron, etc). */
  actions?: (row: T) => ReactNode;
  actionsWidth?: string;
  className?: string;
  /** Accessible label for the table region, e.g. "Tracked repositories". */
  "aria-label"?: string;
  /**
   * Optional: the `rowKey` of the row to mark as the one a caller's own UI
   * currently has open elsewhere (D2 - the Activity row behind an open
   * receipt drawer). Compared against each row's `rowKey(row)` with `===`,
   * so `0` is a legitimate key and matches correctly; `undefined`/`null`
   * (the default) marks no row. The matching row gets `aria-current="true"`
   * plus the baseline's `bg-muted` fill across the whole row - not a new
   * visual, and not the deferred selection model described in the file doc
   * comment, since the table itself never decides or emits this value.
   * Only Activity passes it today; Repos never highlighted a row this way
   * (baseline `9a254c2`) and does not opt in.
   */
  currentKey?: string | number | null;
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
  currentKey,
}: DataTableProps<T>) {
  const rowHeight = ROW_HEIGHT[density];
  const clickable = onRowClick !== undefined;
  const template =
    columns.map((c) => c.width).join(" ") + " 1fr" + (actions ? ` ${actionsWidth}` : "");

  return (
    <div
      className={cn(
        "flex min-h-0 max-h-full flex-col overflow-hidden rounded-xl border border-border bg-card shadow-sm",
        className,
      )}
      role="table"
      aria-label={ariaLabel}
    >
      {/*
        Horizontal AND vertical scroll both live here, inside the table
        container, never on the page - see the scroll-ownership doc comment
        above. `min-h-0` lets this box shrink below its content height so
        `max-h-full` on the root above actually has room to bite.
      */}
      <div className="min-h-0 min-w-0 flex-1 overflow-auto">
        <div className="min-w-max">
          {/*
            `contents` (found empirically in a real browser, not jsdom, during
            the fix round after the Codex review of PR #73's finding 1): a
            `role="rowgroup"` wrapper that renders as a normal block box
            becomes the sticky header row's CONTAINING block, and that box is
            exactly as tall as the header row itself (nothing else lives in
            it). A sticky element can never move outside its containing
            block's box, so with zero extra height to move within, the header
            has zero room to "stick" and just scrolls with everything else -
            a DIFFERENT sticky trap than a missing inset (the one this file
            already documented), caught only by scrolling a real page: jsdom
            has no layout engine and every unit-style assertion here (`style.
            position`, `style.top`) still reads "sticky"/"0px" whether or not
            it actually holds one. `display: contents` removes the wrapper's
            own box entirely - its ARIA role still exposes normally in
            Chromium/WebView2 - so the header row's containing block becomes
            THIS div (`min-w-max`, which spans the table's full scrollable
            height), giving it room to stick. Applied to both rowgroups for
            symmetry, though only the header one is load-bearing.
          */}
          <div role="rowgroup" className="contents">
            <div
              role="row"
              className="grid border-b border-border bg-muted"
              style={{
                gridTemplateColumns: template,
                height: HEADER_HEIGHT,
                position: "sticky",
                top: 0,
                zIndex: 3,
              }}
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
                  aria-hidden
                  className="z-[4] bg-muted"
                  style={{ position: "sticky", right: 0, zIndex: 4, minWidth: 0 }}
                />
              )}
            </div>
          </div>

          <div role="rowgroup" className="contents">
            {rows.map((row) => {
              const key = rowKey(row);
              // `!= null` (not bare truthiness) so a falsy-but-real key like
              // `0` still matches - see the prop's own doc comment.
              const isCurrent = currentKey !== undefined && currentKey !== null && key === currentKey;
              return (
                <div
                  key={key}
                  role="row"
                  onClick={clickable ? () => onRowClick(row) : undefined}
                  aria-current={isCurrent ? "true" : undefined}
                  className={cn(
                    "group grid border-b border-border last:border-b-0",
                    clickable && "cursor-pointer hover:bg-muted",
                    isCurrent && "bg-muted",
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
                          isCurrent && "bg-muted",
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
                  <div aria-hidden className={cn("min-w-0 bg-card group-hover:bg-muted", isCurrent && "bg-muted")} />
                  {actions && (
                    <div
                      role="cell"
                      className={cn(
                        "flex items-center justify-end gap-1 bg-card group-hover:bg-muted",
                        isCurrent && "bg-muted",
                      )}
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
    </div>
  );
}
