import { useState } from "react";
import { FolderOpen, FolderSearch, Loader2, Plus } from "lucide-react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { commands } from "@/lib/bindings";
import type { ScanCandidate } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Dialog } from "@/components/ui/dialog";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { useToast } from "@/hooks/use-toast";
import { cn } from "@/lib/utils";

export function AddReposDialog({
  open,
  onClose,
  onAdded,
}: {
  open: boolean;
  onClose: () => void;
  onAdded: () => void;
}) {
  const toast = useToast();
  const [path, setPath] = useState("");
  const [busy, setBusy] = useState<"scan" | "add" | null>(null);
  const [candidates, setCandidates] = useState<ScanCandidate[] | null>(null);
  const [selected, setSelected] = useState<Set<string>>(new Set());

  function close() {
    setPath("");
    setCandidates(null);
    setSelected(new Set());
    setBusy(null);
    onClose();
  }

  async function browse() {
    // Native OS folder picker (Explorer on Windows / Finder on macOS) via
    // tauri-plugin-dialog. Cancel resolves to null; a pick resolves to a single
    // directory path string, which feeds the same `path` state Scan and Add read
    // (so no extra wiring - Browse just populates the input). Import is aliased to
    // openDialog because this component already binds a prop named `open`.
    const picked = await openDialog({ directory: true, multiple: false });
    if (typeof picked === "string") setPath(picked);
  }

  async function scan() {
    const target = path.trim();
    if (!target) return;
    setBusy("scan");
    setCandidates(null);
    try {
      const result = await unwrap(commands.repoScanParent(target));
      setCandidates(result.discovered);
      setSelected(
        new Set(result.discovered.filter((c) => !c.alreadyTracked).map((c) => c.localPath)),
      );
      if (result.discovered.length === 0) {
        toast("info", "No repositories found", `Nothing git-tracked under ${result.parentPath}.`);
      }
    } catch (e) {
      toast("error", "Scan failed", e instanceof IpcError ? e.message : String(e));
    } finally {
      setBusy(null);
    }
  }

  async function addDirect() {
    const target = path.trim();
    if (!target) return;
    setBusy("add");
    try {
      await unwrap(commands.repoAddPath(target));
      toast("ok", "Repository added", target);
      onAdded();
      close();
    } catch (e) {
      toast("error", "Could not add repo", e instanceof IpcError ? e.message : String(e));
      setBusy(null);
    }
  }

  async function addSelected() {
    const paths = [...selected];
    if (paths.length === 0) return;
    setBusy("add");
    let added = 0;
    let failed = 0;
    for (const p of paths) {
      try {
        await unwrap(commands.repoAddPath(p));
        added += 1;
      } catch {
        failed += 1;
      }
    }
    if (added > 0) {
      toast(
        "ok",
        `Added ${added} ${added === 1 ? "repo" : "repos"}`,
        failed > 0 ? `${failed} could not be added.` : undefined,
      );
    } else {
      toast("error", "Nothing added", "None of the selected repositories could be added.");
    }
    onAdded();
    close();
  }

  function toggle(p: string) {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(p)) next.delete(p);
      else next.add(p);
      return next;
    });
  }

  const scanning = busy === "scan";
  const adding = busy === "add";

  return (
    <Dialog open={open} onClose={close}>
      <div className="border-b border-border px-5 py-4">
        <h2 className="text-base font-semibold">Add repositories</h2>
        <p className="mt-0.5 text-sm text-muted-foreground">
          Browse for a folder or type a path, then scan it for git repositories or add a single repository directly.
        </p>
      </div>

      <div className="flex flex-col gap-3 px-5 py-4">
        <div className="flex gap-2">
          <Input
            value={path}
            onChange={(e) => setPath(e.target.value)}
            placeholder="C:\Users\you\code"
            spellCheck={false}
            onKeyDown={(e) => {
              if (e.key === "Enter") void scan();
            }}
          />
          <Button variant="outline" onClick={() => void browse()}>
            <FolderOpen /> Browse
          </Button>
          <Button variant="secondary" disabled={!path.trim() || scanning} onClick={() => void scan()}>
            {scanning ? <Loader2 className="animate-spin" /> : <FolderSearch />} Scan
          </Button>
        </div>

        {candidates !== null && (
          <div className="max-h-72 overflow-auto rounded-md border border-border">
            {candidates.length === 0 ? (
              <div className="px-3 py-6 text-center text-sm text-muted-foreground">
                No git repositories found in that folder.
              </div>
            ) : (
              candidates.map((c) => {
                const checked = selected.has(c.localPath);
                return (
                  <label
                    key={c.localPath}
                    className={cn(
                      "flex cursor-pointer items-center gap-3 border-b border-border px-3 py-2 last:border-b-0",
                      c.alreadyTracked ? "opacity-60" : "hover:bg-muted",
                    )}
                  >
                    <input
                      type="checkbox"
                      checked={checked}
                      disabled={c.alreadyTracked}
                      onChange={() => toggle(c.localPath)}
                      className="size-4 accent-primary"
                    />
                    <div className="min-w-0 flex-1">
                      <div className="truncate font-mono text-sm font-medium">{c.localName}</div>
                      <div className="truncate font-mono text-[11px] text-muted-foreground">{c.localPath}</div>
                    </div>
                    {c.alreadyTracked && (
                      <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">
                        tracked
                      </span>
                    )}
                  </label>
                );
              })
            )}
          </div>
        )}
      </div>

      <div className="flex items-center gap-2 border-t border-border px-5 py-3">
        <Button variant="ghost" size="sm" onClick={close}>
          Cancel
        </Button>
        <div className="ml-auto flex gap-2">
          <Button variant="outline" size="sm" disabled={!path.trim() || adding} onClick={() => void addDirect()}>
            Add this path
          </Button>
          {candidates !== null && candidates.length > 0 && (
            <Button size="sm" disabled={selected.size === 0 || adding} onClick={() => void addSelected()}>
              {adding ? <Loader2 className="animate-spin" /> : <Plus />} Add {selected.size} selected
            </Button>
          )}
        </div>
      </div>
    </Dialog>
  );
}
