import { useEffect, useRef, useState } from "react";
import { ArrowDown, ArrowUp, Columns3, GripVertical } from "lucide-react";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

/** One row the popover renders: a column's id, its visible label, and
 *  whether it is the locked column (AC-16's "Repository is always shown"). */
export interface ColumnsPopoverItem {
  id: string;
  header: string;
  locked?: boolean;
}

/**
 * The "Columns" button and its popover (E-21 AC-16, round-two mark RC4:
 * "the Columns popover as the table lab drew it... checkboxes, Repository
 * locked, drag to reorder"). No portal, no floating-position library: this
 * codebase has none (`repo-detail.tsx`'s own doc comment on `GroupPills`
 * says so directly - "no popover primitive and Radix is skipped"), so this
 * is a plain `position: relative` wrapper with an `absolute` panel under the
 * trigger, the same `bg-popover`/`shadow-float` surface `Toast` already
 * uses for a floating, non-modal card.
 *
 * Deliberately NOT built on `Dialog`/`Drawer`: both are MODAL (backdrop,
 * full focus trap, `inert` while closed) and this is not - a user should be
 * able to click a table header or scroll the page with the popover open,
 * the way any ordinary dropdown menu behaves. Escape and an outside click
 * both close it; neither traps Tab.
 *
 * Reorder ships two ways, per the criterion ("by dragging, and by keyboard
 * as well"): native HTML5 drag-and-drop on each row (mouse/trackpad only -
 * nothing in jsdom exercises this; the report says so), and a pair of
 * Move-up/Move-down buttons per row, which is the chosen ACCESSIBLE pattern
 * (reachable and operable with Tab/Enter/Space and by a screen reader,
 * unlike drag). The locked row gets neither control and is not a drag
 * source or target: Repository must stay first (`DataTable`'s own `frozen`
 * contract), so nothing here ever offers to move it, and `DataTable` itself
 * also force-sorts a frozen column back to the front as a second,
 * independent guard against a bad stored order.
 */
export function ColumnsPopover({
  items,
  visibility,
  onToggle,
  onReorder,
}: {
  /** All known columns, in the CURRENT display order (locked column(s)
   *  included, wherever they sit - in practice always first). */
  items: ColumnsPopoverItem[];
  /** `id -> false` for hidden; absent or `true` means visible. */
  visibility: Record<string, boolean>;
  onToggle: (id: string) => void;
  /** The full, reordered list of ids (locked columns' positions preserved). */
  onReorder: (nextIds: string[]) => void;
}) {
  const [open, setOpen] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const dragId = useRef<string | null>(null);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(e: MouseEvent) {
      // An outside click chose a different target on purpose - focus stays
      // wherever the click landed, the same as any ordinary dropdown menu.
      if (containerRef.current && !containerRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    }
    function onKeyDown(e: KeyboardEvent) {
      // Escape, unlike an outside click, did not choose a new focus target -
      // without this, closing from a checkbox or a Move button drops focus
      // to `<body>`, which a keyboard-only user has no way to recover from.
      if (e.key === "Escape") {
        setOpen(false);
        triggerRef.current?.focus();
      }
    }
    document.addEventListener("mousedown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("mousedown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const visibleCount = items.filter((c) => c.locked || visibility[c.id] !== false).length;
  const movable = items.filter((c) => !c.locked);
  const lockedId = items.find((c) => c.locked)?.id;

  /** Reorders the MOVABLE ids only, then puts the locked column back in
   *  front - it is never part of the reorderable set (see the file doc
   *  comment on why Repository cannot be a drag source or target). */
  function reorderMovable(nextMovableIds: string[]) {
    onReorder(lockedId ? [lockedId, ...nextMovableIds] : nextMovableIds);
  }

  function move(id: string, delta: number) {
    const ids = movable.map((c) => c.id);
    const i = ids.indexOf(id);
    const j = i + delta;
    if (j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    reorderMovable(ids);
  }

  function dropOn(targetId: string) {
    const draggedId = dragId.current;
    dragId.current = null;
    if (!draggedId || draggedId === targetId) return;
    const ids = movable.map((c) => c.id);
    const from = ids.indexOf(draggedId);
    const to = ids.indexOf(targetId);
    if (from === -1 || to === -1) return;
    ids.splice(to, 0, ids.splice(from, 1)[0]);
    reorderMovable(ids);
  }

  return (
    <div ref={containerRef} className="relative">
      <Button
        ref={triggerRef}
        type="button"
        variant="outline"
        size="sm"
        aria-expanded={open}
        // "dialog", not the default menu semantics "true" would imply: this
        // opens a panel of checkboxes and buttons, not an ARIA menu (arrow-
        // key roving, `menuitem`s).
        aria-haspopup="dialog"
        onClick={() => setOpen((o) => !o)}
      >
        <Columns3 /> Columns {visibleCount}
      </Button>
      {open && (
        <div
          role="region"
          aria-label="Choose and reorder columns"
          className="absolute right-0 top-full z-50 mt-2 w-72 rounded-xl border border-border bg-popover p-2 shadow-float"
        >
          <ul className="flex flex-col gap-0.5">
            {items.map((col) => {
              const checked = col.locked ? true : visibility[col.id] !== false;
              const movableIndex = movable.findIndex((c) => c.id === col.id);
              const lockedNoteId = `columns-popover-locked-${col.id}`;
              return (
                <li
                  key={col.id}
                  draggable={!col.locked}
                  onDragStart={col.locked ? undefined : () => { dragId.current = col.id; }}
                  onDragOver={col.locked ? undefined : (e) => e.preventDefault()}
                  onDrop={col.locked ? undefined : () => dropOn(col.id)}
                  className={cn(
                    "flex items-center gap-1.5 rounded-md px-1.5 py-1",
                    !col.locked && "hover:bg-muted",
                  )}
                >
                  <GripVertical
                    aria-hidden
                    className={cn("size-3.5 shrink-0 text-muted-foreground", col.locked && "opacity-30")}
                  />
                  <label className="flex min-w-0 flex-1 items-center gap-2 text-sm">
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={col.locked}
                      onChange={() => onToggle(col.id)}
                      aria-describedby={col.locked ? lockedNoteId : undefined}
                    />
                    <span className="truncate">{col.header}</span>
                  </label>
                  {col.locked ? (
                    <span id={lockedNoteId} className="shrink-0 text-[11px] text-muted-foreground">
                      Always shown
                    </span>
                  ) : (
                    <div className="flex shrink-0 gap-0.5">
                      <button
                        type="button"
                        aria-label={`Move ${col.header} up`}
                        disabled={movableIndex === 0}
                        onClick={() => move(col.id, -1)}
                        className="rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
                      >
                        <ArrowUp className="size-3" />
                      </button>
                      <button
                        type="button"
                        aria-label={`Move ${col.header} down`}
                        disabled={movableIndex === movable.length - 1}
                        onClick={() => move(col.id, 1)}
                        className="rounded-sm p-1 text-muted-foreground hover:bg-muted hover:text-foreground disabled:pointer-events-none disabled:opacity-40"
                      >
                        <ArrowDown className="size-3" />
                      </button>
                    </div>
                  )}
                </li>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
}
