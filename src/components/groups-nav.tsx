import { useState } from "react";
import type { ReactNode } from "react";
import { Boxes, Check, Pencil, Plus, Trash2, X } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { GroupSummary } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { GroupDialog } from "@/components/group-dialog";
import { useToast } from "@/hooks/use-toast";
import { cn } from "@/lib/utils";

type DialogState = { mode: "create" | "rename"; group: GroupSummary | null };

/**
 * The sidebar "Groups" section: an "All repositories" clear row, one row per
 * group (color dot + name + member count) with hover rename/delete affordances,
 * and a "New group" control. Selecting a group lifts the active filter to the
 * app shell, which also switches to the Repos view.
 *
 * Destination state vs. filter state (post-review correction). The active
 * group filter PERSISTS across every screen (selecting it once keeps it
 * applied on Repos even after navigating away and back - the shipped
 * cross-screen behaviour this file's own history already documents), but
 * before this fix the row that names the current filter rendered its
 * accent-tinted "selected" fill on every screen, not only Repos. A Codex
 * adversarial review caught the result: Dashboard's own active nav item
 * plus an accent-active "All repositories"/group row under an inactive
 * Repos item, two things reading as "current" in one rail at once, and the
 * same collision on Activity/Settings whenever a group was selected.
 *
 * `railActive` decouples the two: `aria-pressed` always reflects the true,
 * persistent filter state (this row's own semantic "is this filter engaged"
 * fact, unaffected by which screen is showing), while the ACCENT FILL only
 * paints when `railActive` is true (the caller passes `view === "repos"`),
 * so the visual "you are here" language stays exclusive to the primary nav's
 * own active item. The fill itself was left as the pre-existing
 * `bg-primary/10 text-primary` - a light colour TINT with no font-weight
 * change - which is already visually subordinate to the primary nav's own
 * active treatment (a flat, opaque `bg-sidebar-accent` fill plus a left
 * accent bar plus `font-semibold`, `app-shell.tsx`'s `NavButton`): a lighter
 * lever set can never outrank a heavier one painted in the same rail.
 */
export function GroupsNav({
  groups,
  activeGroupId,
  railActive,
  onSelectGroup,
  onClearActiveGroup,
  refetchGroups,
}: {
  groups: GroupSummary[];
  activeGroupId: number | null;
  /** Whether the current screen is Repos - gates the ACCENT FILL only; the
   * underlying filter (and `aria-pressed`) is unaffected by this flag. */
  railActive: boolean;
  onSelectGroup: (id: number | null) => void;
  onClearActiveGroup: () => void;
  refetchGroups: () => void;
}) {
  const toast = useToast();
  const [dialog, setDialog] = useState<DialogState | null>(null);
  const [confirmDeleteId, setConfirmDeleteId] = useState<number | null>(null);
  const [busyDeleteId, setBusyDeleteId] = useState<number | null>(null);

  async function doDelete(id: number) {
    setBusyDeleteId(id);
    try {
      await unwrap(commands.groupDelete(id));
      toast("ok", "Group deleted");
      // Clear the filter without the navigation side effect `onSelectGroup`
      // carries (it force-switches to the Repos view; deleting the active
      // group filter can happen from any screen, since the sidebar renders
      // everywhere - E-16 Known defect 6).
      if (activeGroupId === id) onClearActiveGroup();
      refetchGroups();
    } catch (e) {
      toast("error", "Could not delete group", e instanceof IpcError ? e.message : String(e));
    } finally {
      setBusyDeleteId(null);
      setConfirmDeleteId(null);
    }
  }

  return (
    // N5 (sidebar restructure and toolbar consolidation): no top border/pt-3
    // here any more - this section now renders nested
    // directly beneath the Repos nav button (app-shell.tsx supplies the
    // indent and the left guide rail that say "nested," not a rule separating
    // two unrelated top-level blocks).
    <div className="flex min-h-0 flex-1 flex-col pt-1">
      <div className="flex items-center gap-2 px-2 pb-1.5">
        <span className="text-[10px] font-bold uppercase tracking-wider text-muted-foreground">
          Groups
        </span>
        <button
          type="button"
          onClick={() => setDialog({ mode: "create", group: null })}
          title="New group"
          aria-label="New group"
          className="ml-auto grid size-5 place-items-center rounded text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
        >
          <Plus className="size-3.5" />
        </button>
      </div>

      <div className="flex min-h-0 flex-col gap-0.5 overflow-auto px-2.5">
        {groups.length === 0 ? (
          <p className="px-1 py-1 text-xs text-muted-foreground">
            No groups yet. Create one to organize your repos.
          </p>
        ) : (
          <>
            <button
              type="button"
              onClick={() => onSelectGroup(null)}
              aria-pressed={activeGroupId === null}
              className={cn(
                "flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm font-medium transition-colors",
                activeGroupId === null && railActive
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              <Boxes className="size-[15px] shrink-0" />
              <span className="truncate">All repositories</span>
            </button>

            {groups.map((group) => (
              <GroupRow
                key={group.id}
                group={group}
                active={activeGroupId === group.id}
                railActive={railActive}
                confirming={confirmDeleteId === group.id}
                deleting={busyDeleteId === group.id}
                onSelect={() => onSelectGroup(group.id)}
                onRename={() => setDialog({ mode: "rename", group })}
                onAskDelete={() => setConfirmDeleteId(group.id)}
                onCancelDelete={() => setConfirmDeleteId(null)}
                onConfirmDelete={() => void doDelete(group.id)}
              />
            ))}
          </>
        )}
      </div>

      <GroupDialog
        open={dialog !== null}
        mode={dialog?.mode ?? "create"}
        group={dialog?.group}
        onClose={() => setDialog(null)}
        onSaved={refetchGroups}
      />
    </div>
  );
}

function GroupRow({
  group,
  active,
  railActive,
  confirming,
  deleting,
  onSelect,
  onRename,
  onAskDelete,
  onCancelDelete,
  onConfirmDelete,
}: {
  group: GroupSummary;
  /** The true, persistent filter state: is THIS group the active filter. */
  active: boolean;
  /** Whether the current screen is Repos - gates the visual fill only. */
  railActive: boolean;
  confirming: boolean;
  deleting: boolean;
  onSelect: () => void;
  onRename: () => void;
  onAskDelete: () => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
}) {
  // Destination vs. filter (see the module doc comment): `active` is the
  // real, always-true-when-selected filter fact, carried to assistive tech
  // via `aria-pressed` on the row's own button regardless of which screen is
  // showing. `visuallyActive` is what actually paints, and only paints on
  // Repos, so this row can never look "current" on a screen the primary nav
  // already marked as current with a heavier, categorically different (bar
  // plus weight plus opaque fill) treatment.
  const visuallyActive = active && railActive;
  return (
    <div
      className={cn(
        "group/row relative flex items-center rounded-md text-sm font-medium transition-colors",
        visuallyActive ? "bg-primary/10 text-primary" : "text-muted-foreground hover:bg-muted",
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-pressed={active}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-2.5 py-1.5 pl-2.5 text-left",
          !visuallyActive && "hover:text-foreground",
        )}
      >
        <span
          className={cn(
            "size-2.5 shrink-0 rounded-full",
            group.color === null && "bg-muted-foreground/50",
          )}
          style={group.color ? { backgroundColor: group.color } : undefined}
        />
        <span className="truncate">{group.name}</span>
      </button>

      {confirming ? (
        <span className="flex items-center gap-1 pr-1.5">
          <span className="text-[11px] text-muted-foreground">Delete?</span>
          <RowIcon
            label="Confirm delete"
            onClick={onConfirmDelete}
            disabled={deleting}
            className="text-status-failed hover:bg-status-failed/15"
          >
            <Check className="size-3.5" />
          </RowIcon>
          <RowIcon label="Cancel delete" onClick={onCancelDelete} disabled={deleting}>
            <X className="size-3.5" />
          </RowIcon>
        </span>
      ) : (
        <span className="flex items-center pr-2.5">
          <span
            className={cn(
              "font-mono text-[11px] tabular-nums transition-opacity group-hover/row:opacity-0 group-focus-within/row:opacity-0",
              visuallyActive ? "text-primary" : "text-muted-foreground",
            )}
          >
            {group.repoCount}
          </span>
          <span className="absolute right-1.5 flex items-center gap-0.5 opacity-0 transition-opacity group-hover/row:opacity-100 group-focus-within/row:opacity-100">
            <RowIcon
              label={`Rename ${group.name}`}
              onClick={onRename}
              className="hover:bg-muted-foreground/15 hover:text-foreground"
            >
              <Pencil className="size-3.5" />
            </RowIcon>
            <RowIcon
              label={`Delete ${group.name}`}
              onClick={onAskDelete}
              className="hover:bg-status-failed/15 hover:text-status-failed"
            >
              <Trash2 className="size-3.5" />
            </RowIcon>
          </span>
        </span>
      )}
    </div>
  );
}

function RowIcon({
  label,
  onClick,
  disabled,
  className,
  children,
}: {
  label: string;
  onClick: () => void;
  disabled?: boolean;
  className?: string;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "grid size-6 place-items-center rounded text-muted-foreground transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50",
        className,
      )}
    >
      {children}
    </button>
  );
}
