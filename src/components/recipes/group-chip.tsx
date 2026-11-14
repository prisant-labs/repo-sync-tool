import { X } from "lucide-react";
import { cn } from "@/lib/utils";
import type { GroupSummary } from "@/lib/bindings";

/**
 * The small colour dot that stands for a group. A group with no colour gets a
 * muted grey dot so it still reads as a group.
 *
 * `compact` is the smaller dot used inside the table chip. Callers: both
 * `GroupChip` variants below, and the add-to-group candidates in
 * `repo-detail.tsx`.
 */
export function GroupDot({ color, compact = false }: { color: string | null; compact?: boolean }) {
  return (
    <span
      className={cn(
        compact ? "size-1.5" : "size-2",
        "shrink-0 rounded-full",
        color === null && "bg-muted-foreground/50",
      )}
      style={color ? { backgroundColor: color } : undefined}
    />
  );
}

type GroupChipProps =
  | {
      /** A chip with an X button that removes the repo from the group. */
      variant: "removable";
      group: GroupSummary;
      /** Disables the X button while a change is in flight. */
      busy: boolean;
      onRemove: () => void;
    }
  | {
      /** A read-only chip for a table cell: smaller type, no X button. */
      variant: "compact";
      group: GroupSummary;
    };

/**
 * A group's name beside its colour dot, in a rounded outline.
 *
 * The two variants are the two places the app shows group membership, and
 * they have always looked different. This component keeps both exactly as
 * they were; choosing one look for both is a design decision, not a
 * refactoring one.
 *
 * - `removable`: the repository detail panel's header. Body-size cell text,
 *   a regular dot, normal text colour, and an X button on the right.
 * - `compact`: the Repos table's Groups column. Micro text, a smaller dot,
 *   muted text colour, no wrapping, no button.
 *
 * Callers: `GroupPills` in `repo-detail.tsx` (removable) and the Groups
 * column in `screens/repos.tsx` (compact).
 */
export function GroupChip(props: GroupChipProps) {
  const { group } = props;
  if (props.variant === "compact") {
    return (
      <span className="inline-flex items-center gap-1 rounded-full border border-border px-1.5 py-0.5 text-micro font-medium whitespace-nowrap text-muted-foreground">
        <GroupDot color={group.color} compact />
        {group.name}
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-border py-0.5 pr-1 pl-2 text-cell font-medium">
      <GroupDot color={group.color} />
      {group.name}
      <button
        type="button"
        aria-label={`Remove from ${group.name}`}
        disabled={props.busy}
        onClick={props.onRemove}
        className="rounded-full p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
      >
        <X className="size-3" />
      </button>
    </span>
  );
}
