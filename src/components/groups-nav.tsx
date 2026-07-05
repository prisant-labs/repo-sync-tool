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
    <div className="flex min-h-0 flex-1 flex-col border-t border-border pt-3">
      <div className="flex items-center gap-2 px-3.5 pb-1.5">
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
              className={cn(
                "flex items-center gap-2.5 rounded-md px-2.5 py-1.5 text-sm font-medium transition-colors",
                activeGroupId === null
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
        active ? "bg-primary/10 text-primary" : "text-muted-foreground hover:bg-muted",
      )}
    >
      <button
        type="button"
        onClick={onSelect}
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
              active ? "text-primary" : "text-muted-foreground",
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
