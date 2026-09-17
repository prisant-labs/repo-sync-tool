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
 * A2: the engaged group is marked on EVERY screen. The filter has always
 * persisted across screens; now the mark does too, so the rail answers "what
 * am I scoped to" wherever you are standing rather than only on Repos.
 *
 * Destination and filter remain two different facts carried on two different
 * attributes, which is what keeps one rail from reading as two "current"
 * things at once: `aria-current="page"` marks WHERE YOU ARE and is set only
 * by the primary nav; `aria-pressed` marks WHAT IS ENGAGED and is set only
 * here. Both can be true in the same rail without competing, because they
 * are not the same claim.
 *
 * The visual treatments are deliberately unequal for the same reason. A nav
 * item's active state is a flat, opaque `bg-sidebar-accent` fill plus a 2px
 * left accent bar plus `font-semibold` (`app-shell.tsx`'s `NavButton`); a
 * group row's is a light `bg-primary/10` TINT with no weight change. A
 * lighter lever set cannot outrank a heavier one painted in the same rail.
 *
 * `text-primary-ink` rather than `text-primary` is load-bearing, not
 * cosmetic: accent text on this tint measures 4.27:1 light and 3.99:1 dark
 * with `--primary`, both under the 4.5:1 floor (D-14, fixed in PR #92).
 */
export function GroupsNav({
  groups,
  activeGroupId,
  onSelectGroup,
  onClearActiveGroup,
  refetchGroups,
}: {
  groups: GroupSummary[];
  activeGroupId: number | null;
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
    // A1: no top border here. This section is a plain line under the whole
    // nav, so it needs neither a rule separating it from the nav above nor
    // the indent and guide rail an earlier nesting used - app-shell.tsx no
    // longer supplies those either.
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
                activeGroupId === null
                  ? "bg-primary/10 text-primary-ink"
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
  confirming,
  deleting,
  onSelect,
  onRename,
  onAskDelete,
  onCancelDelete,
  onConfirmDelete,
}: {
  group: GroupSummary;
  /** The persistent filter state: is THIS group the engaged filter. Carried
   * to assistive tech via `aria-pressed` and painted as a tint, on every
   * screen (A2) - the semantic fact and the visual one no longer diverge. */
  active: boolean;
  confirming: boolean;
  deleting: boolean;
  onSelect: () => void;
  onRename: () => void;
  onAskDelete: () => void;
  onCancelDelete: () => void;
  onConfirmDelete: () => void;
}) {
  return (
    <div
      className={cn(
        "group/row relative flex items-center rounded-md text-sm font-medium transition-colors",
        active ? "bg-primary/10 text-primary-ink" : "text-muted-foreground hover:bg-muted",
      )}
    >
      <button
        type="button"
        onClick={onSelect}
        aria-pressed={active}
        className={cn(
          "flex min-w-0 flex-1 items-center gap-2.5 py-1.5 pl-2.5 text-left",
          !active && "hover:text-foreground",
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
              active ? "text-primary-ink" : "text-muted-foreground",
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
