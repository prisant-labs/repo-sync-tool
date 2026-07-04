import { useCallback, useMemo, useState } from "react";
import { FolderGit2, Plus, RefreshCw, Search } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { RepoSummary } from "@/lib/bindings";
import { unwrap } from "@/lib/ipc";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { StatusBadge } from "@/components/status-badge";
import { LagSignal } from "@/components/lag-signal";
import { Drawer } from "@/components/ui/drawer";
import { RepoDetailPanel } from "@/components/repo-detail";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { useBackendEvents, useRepoList } from "@/hooks/queries";
import {
  deriveStatus,
  lagLabel,
  lagMagnitude,
  relativeTime,
  STATUS_STYLE,
  type RepoStatus,
} from "@/lib/status";
import { cn } from "@/lib/utils";

const ALL_FILTER = { enabledOnly: null, hostType: null, query: null };
const GRID = "grid grid-cols-[1.8fr_130px_150px_112px_auto] items-center gap-4";
const STATUS_ORDER: RepoStatus[] = ["behind", "dirty", "failed", "paused", "ahead", "sync"];

type Chip = RepoStatus | "all";

export function ReposScreen() {
  const repos = useRepoList(ALL_FILTER);
  const refetch = repos.refetch;
  useBackendEvents(refetch);

  const [busyId, setBusyId] = useState<number | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [chip, setChip] = useState<Chip>("all");

  const checkNow = useCallback(
    async (id: number) => {
      setBusyId(id);
      try {
        await unwrap(commands.repoCheckNow(id));
      } catch {
        // Outcome also arrives via the repo:check-completed event; row-level
        // error surfacing is a later pass.
      } finally {
        setBusyId(null);
        refetch();
      }
    },
    [refetch],
  );

  const list = useMemo(() => repos.data ?? [], [repos.data]);

  const counts = useMemo(() => {
    const c: Record<RepoStatus, number> = { sync: 0, ahead: 0, behind: 0, dirty: 0, failed: 0, paused: 0 };
    for (const r of list) c[deriveStatus(r)] += 1;
    return c;
  }, [list]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return list.filter((r) => {
      if (chip !== "all" && deriveStatus(r) !== chip) return false;
      if (q && !r.localName.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [list, query, chip]);

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-5">
      <div className="flex flex-wrap items-end gap-4">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Repos</h2>
          <p className="text-sm text-muted-foreground">Every repository RepoSync is watching.</p>
        </div>
        <Button size="sm" className="ml-auto" onClick={() => setAddOpen(true)}>
          <Plus /> Add repos
        </Button>
      </div>

      {list.length > 0 && (
        <div className="flex flex-wrap items-center gap-3">
          <div className="relative w-full max-w-xs">
            <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
            <Input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Filter by name"
              className="pl-8"
              spellCheck={false}
            />
          </div>
          <div className="flex flex-wrap gap-1.5">
            <FilterChip label="All" count={list.length} active={chip === "all"} onClick={() => setChip("all")} />
            {STATUS_ORDER.map(
              (s) =>
                counts[s] > 0 && (
                  <FilterChip
                    key={s}
                    label={STATUS_STYLE[s].label}
                    count={counts[s]}
                    active={chip === s}
                    tone={STATUS_STYLE[s].text}
                    onClick={() => setChip(s)}
                  />
                ),
            )}
          </div>
        </div>
      )}

      <AsyncPanel
        state={repos}
        emptyWhen={(l) => l.length === 0}
        emptyMessage={
          <EmptyState
            icon={FolderGit2}
            title="No repositories yet"
            description="Scan a folder or add a single path to start tracking sync status."
            action={
              <Button onClick={() => setAddOpen(true)}>
                <Plus /> Add repositories
              </Button>
            }
          />
        }
      >
        {() =>
          filtered.length === 0 ? (
            <div className="rounded-xl border border-border bg-card py-16 text-center text-sm text-muted-foreground shadow-sm">
              No repositories match this filter.
            </div>
          ) : (
            <div className="overflow-hidden rounded-xl border border-border bg-card shadow-sm">
              <div
                className={cn(
                  GRID,
                  "border-b border-border bg-muted/40 px-4 py-2.5 font-mono text-[10px] font-bold uppercase tracking-wider text-muted-foreground",
                )}
              >
                <div>Repository</div>
                <div>Status</div>
                <div>Lag signal</div>
                <div>Checked</div>
                <div />
              </div>
              {filtered.map((repo) => (
                <RepoRow
                  key={repo.id}
                  repo={repo}
                  busy={busyId === repo.id}
                  onOpen={() => setSelectedId(repo.id)}
                  onCheck={() => checkNow(repo.id)}
                />
              ))}
            </div>
          )
        }
      </AsyncPanel>

      <Drawer open={selectedId !== null} onClose={() => setSelectedId(null)}>
        {selectedId !== null && (
          <RepoDetailPanel id={selectedId} onChanged={refetch} onClose={() => setSelectedId(null)} />
        )}
      </Drawer>

      <AddReposDialog open={addOpen} onClose={() => setAddOpen(false)} onAdded={refetch} />
    </div>
  );
}

function FilterChip({
  label,
  count,
  active,
  tone,
  onClick,
}: {
  label: string;
  count: number;
  active: boolean;
  tone?: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={cn(
        "flex items-center gap-1.5 rounded-full border px-2.5 py-1 text-xs font-medium transition-colors",
        active
          ? "border-primary bg-primary/10 text-primary"
          : "border-border text-muted-foreground hover:bg-muted",
      )}
    >
      <span className={cn(!active && tone)}>{label}</span>
      <span
        className={cn("rounded-full px-1.5 font-mono text-[10px]", active ? "bg-primary/15" : "bg-muted")}
      >
        {count}
      </span>
    </button>
  );
}

function RepoRow({
  repo,
  busy,
  onOpen,
  onCheck,
}: {
  repo: RepoSummary;
  busy: boolean;
  onOpen: () => void;
  onCheck: () => void;
}) {
  const status = deriveStatus(repo);
  const count =
    status === "behind"
      ? (repo.behindCount ?? 0)
      : status === "ahead"
        ? (repo.aheadCount ?? 0)
        : undefined;

  return (
    <div
      role="button"
      tabIndex={0}
      onClick={onOpen}
      onKeyDown={(e) => {
        if (e.key === "Enter") onOpen();
      }}
      className={cn(
        GRID,
        "cursor-pointer border-b border-border px-4 py-3 last:border-b-0 hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:outline-none",
      )}
    >
      <div className="min-w-0">
        <div className="truncate font-mono text-sm font-semibold">{repo.localName}</div>
        <div className="truncate font-mono text-[11px] text-muted-foreground">
          {repo.hostType}
          {repo.latestReleaseTag ? ` · ${repo.latestReleaseTag}` : ""}
        </div>
      </div>
      <StatusBadge status={status} count={count} />
      <LagSignal status={status} magnitude={lagMagnitude(repo)} label={lagLabel(repo)} />
      <div className="font-mono text-[11px] text-muted-foreground">{relativeTime(repo.lastCheckedAt)}</div>
      <div className="flex justify-end">
        <Button
          variant="ghost"
          size="icon"
          disabled={busy}
          title="Check now"
          onClick={(e) => {
            e.stopPropagation();
            onCheck();
          }}
        >
          <RefreshCw className={busy ? "animate-spin" : undefined} />
        </Button>
      </div>
    </div>
  );
}
