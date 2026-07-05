import { useState } from "react";
import { Loader2 } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { GroupSummary } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useToast } from "@/hooks/use-toast";
import { cn } from "@/lib/utils";

// Preset swatches for a new group's color dot. Mid-lightness oklch values tuned
// to the Graphite palette so each reads clearly on both the light and dark card
// surfaces. Applied to the dot via inline style, never a dynamic Tailwind class.
const GROUP_COLORS = [
  "oklch(0.55 0.16 264)",
  "oklch(0.55 0.13 150)",
  "oklch(0.62 0.13 79)",
  "oklch(0.55 0.19 27)",
  "oklch(0.52 0.19 293)",
  "oklch(0.56 0.19 349)",
  "oklch(0.55 0.1 210)",
  "oklch(0.5 0.03 258)",
];

// Human names for the preset swatches above, matched by index, so each color
// button gets a distinct accessible name (finding 13 / BL-NI-29d; a screen
// reader otherwise hears "Choose group color" eight times with no way to
// tell them apart). Named after the same hues DESIGN.md uses for the status
// taxonomy where they line up (blue is the interaction accent; green/amber/
// red/violet/magenta mirror sync/dirty/failed/behind/release).
const GROUP_COLOR_NAMES = ["Blue", "Green", "Amber", "Red", "Violet", "Magenta", "Teal", "Slate"];

/**
 * Whether an error is `group_create` / `group_rename`'s duplicate-name
 * rejection, keyed on the error code and the `field` it carries (E-16 Known
 * defect 4) rather than string-matching the message, so it stays robust if
 * the wire message text ever changes.
 */
function isDuplicateNameError(e: unknown): boolean {
  if (!(e instanceof IpcError) || e.code !== "config.invalid_setting") return false;
  const context = e.payload.context;
  return typeof context === "object" && context !== null && (context as { field?: unknown }).field === "name";
}

/**
 * Create or rename a group. In create mode it collects a name plus a preset
 * color; in rename mode only the name is editable (the backend `group_rename`
 * command carries no color). On success it toasts and calls `onSaved` so the
 * caller can refetch the group list.
 */
export function GroupDialog({
  open,
  mode,
  group,
  onClose,
  onSaved,
}: {
  open: boolean;
  mode: "create" | "rename";
  group?: GroupSummary | null;
  onClose: () => void;
  onSaved: () => void;
}) {
  const toast = useToast();
  const [name, setName] = useState("");
  const [color, setColor] = useState<string>(GROUP_COLORS[0]);
  const [busy, setBusy] = useState(false);
  const [seedKey, setSeedKey] = useState<string | null>(null);

  // Re-seed the fields whenever the dialog opens for a new target, and drop the
  // seed on close so the next open always starts fresh. Adjusting state during
  // render (rather than in an effect) is React's recommended reset-on-prop
  // pattern and keeps the dialog mounted for its open/close transition.
  const openKey = open ? `${mode}:${group?.id ?? "new"}` : null;
  if (openKey !== seedKey) {
    setSeedKey(openKey);
    if (open) {
      setName(mode === "rename" && group ? group.name : "");
      setColor(mode === "rename" && group?.color ? group.color : GROUP_COLORS[0]);
      setBusy(false);
    }
  }

  async function submit() {
    // Guard re-entrancy: a double-Enter in the name field (or a stray double
    // click) could otherwise fire two creates/renames for the same input
    // before the first `await` resolves and `busy` re-renders (E-16 Known
    // defect 3).
    if (busy) return;
    const trimmed = name.trim();
    if (!trimmed) return;
    setBusy(true);
    try {
      if (mode === "create") {
        await unwrap(commands.groupCreate(trimmed, color));
        toast("ok", "Group created", trimmed);
      } else if (group) {
        await unwrap(commands.groupRename(group.id, trimmed));
        toast("ok", "Group renamed", trimmed);
      }
      onSaved();
      onClose();
    } catch (e) {
      toast(
        "error",
        mode === "create" ? "Could not create group" : "Could not rename group",
        isDuplicateNameError(e)
          ? "That name is already taken."
          : e instanceof IpcError
            ? e.message
            : String(e),
      );
      setBusy(false);
    }
  }

  return (
    <Dialog open={open} onClose={onClose}>
      <div className="border-b border-border px-5 py-4">
        <h2 className="text-base font-semibold">{mode === "create" ? "New group" : "Rename group"}</h2>
        <p className="mt-0.5 text-sm text-muted-foreground">
          {mode === "create"
            ? "Name a group and pick a color to organize your repositories."
            : "Give this group a new name. Its color and members stay the same."}
        </p>
      </div>

      <div className="flex flex-col gap-4 px-5 py-4">
        <div className="flex flex-col gap-1.5">
          <label className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
            Name
          </label>
          <Input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. Work, Personal, Forks"
            autoFocus
            spellCheck={false}
            onKeyDown={(e) => {
              if (e.key === "Enter") void submit();
            }}
          />
        </div>

        {mode === "create" && (
          <div className="flex flex-col gap-1.5">
            <span className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
              Color
            </span>
            <div className="flex flex-wrap gap-2">
              {GROUP_COLORS.map((c, i) => (
                <button
                  key={c}
                  type="button"
                  aria-label={`${GROUP_COLOR_NAMES[i]} (color ${i + 1} of ${GROUP_COLORS.length})`}
                  aria-pressed={color === c}
                  onClick={() => setColor(c)}
                  className={cn(
                    "size-7 rounded-full ring-offset-2 ring-offset-card transition-shadow focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring",
                    color === c ? "ring-2 ring-ring" : "hover:ring-2 hover:ring-border",
                  )}
                  style={{ backgroundColor: c }}
                />
              ))}
            </div>
          </div>
        )}
      </div>

      <div className="flex items-center gap-2 border-t border-border px-5 py-3">
        <Button variant="ghost" size="sm" onClick={onClose}>
          Cancel
        </Button>
        <Button
          size="sm"
          className="ml-auto"
          disabled={!name.trim() || busy}
          onClick={() => void submit()}
        >
          {busy && <Loader2 className="animate-spin" />}
          {mode === "create" ? "Create group" : "Save name"}
        </Button>
      </div>
    </Dialog>
  );
}
