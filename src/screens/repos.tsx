import { useCallback, useMemo, useState } from "react";
import { FolderGit2, Plus, RefreshCw, Search, X } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { GroupSummary, RepoSummary } from "@/lib/bindings";
import { unwrap } from "@/lib/ipc";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { StatusBadge } from "@/components/status-badge";
import { LagSignal } from "@/components/lag-signal";
import { IntelSignals } from "@/components/intel-signals";
import { Drawer } from "@/components/ui/drawer";
import { RepoDetailPanel } from "@/components/repo-detail";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { useBackendEvents, useRepoGroupMemberships, useRepoList } from "@/hooks/queries";
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

export function ReposScreen({
  activeGroupId,
  groups,
  onClearGroup,
  onGroupsChanged,
}: {
  activeGroupId: number | null;
  groups: GroupSummary[];
  onClearGroup: () => void;
  onGroupsChanged: () => void;
}) {
  const repos = useRepoList(ALL_FILTER);
  const refetch = repos.refetch;
  useBackendEvents(refetch);

  const [busyId, setBusyId] = useState<number | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [chip, setChip] = useState<Chip>("all");

  const list = useMemo(() => repos.data ?? [], [repos.data]);

  // Group memberships for every tracked repo, as Map<repoId, groupId[]>, fetched
  // in a single bulk call (see useRepoGroupMemberships; BL-NI-22). A repo with no
  // groups is absent from the map; `null` means the read is loading or failed.
  const memberships = useRepoGroupMemberships();
  const membershipMap = memberships.data;
  const refetchMemberships = memberships.refetch;

  const groupById = useMemo(() => {
    const m = new Map<number, GroupSummary>();
    for (const g of groups) m.set(g.id, g);
    return m;
  }, [groups]);

  const activeGroup = activeGroupId === null ? null : (groupById.get(activeGroupId) ?? null);

  // After an assignment change in the drawer, refresh the list, the membership
  // map, and the sidebar group counts together.
  const handleRepoChanged = useCallback(() => {
    refetch();
    refetchMemberships();
    onGroupsChanged();
  }, [refetch, refetchMemberships, onGroupsChanged]);

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

  const counts = useMemo(() => {
    const c: Record<RepoStatus, number> = { sync: 0, ahead: 0, behind: 0, dirty: 0, failed: 0, paused: 0 };
    for (const r of list) c[deriveStatus(r)] += 1;
    return c;
  }, [list]);

  // Repos in the active group (before the status / name filters narrow
  // further). `null` means "not yet known" (the membership read is still loading
  // or failed), distinct from a genuine zero (finding 7 / BL-NI-27's sibling
  // defect in the E-16 spec: a null map must never read as "no members").
  const inGroupCount = useMemo(() => {
    if (activeGroupId === null) return list.length;
    if (membershipMap === null) return null;
    return list.filter((r) => membershipMap.get(r.id)?.includes(activeGroupId)).length;
  }, [list, membershipMap, activeGroupId]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return list.filter((r) => {
      if (activeGroupId !== null && !membershipMap?.get(r.id)?.includes(activeGroupId)) return false;
      if (chip !== "all" && deriveStatus(r) !== chip) return false;
      if (q && !r.localName.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [list, query, chip, activeGroupId, membershipMap]);

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

      {activeGroup && (
        <div className="flex items-center gap-2.5 rounded-lg border border-border bg-muted/40 px-3 py-2">
          <span
            className={cn(
              "size-2.5 shrink-0 rounded-full",
              activeGroup.color === null && "bg-muted-foreground/50",
            )}
            style={activeGroup.color ? { backgroundColor: activeGroup.color } : undefined}
          />
          <span className="text-sm">
            Filtered to <span className="font-semibold">{activeGroup.name}</span>
          </span>
          <span className="font-mono text-xs text-muted-foreground">
            {inGroupCount === null ? "…" : `${inGroupCount} ${inGroupCount === 1 ? "repo" : "repos"}`}
          </span>
          <Button variant="ghost" size="sm" className="ml-auto" onClick={onClearGroup}>
            <X /> Clear filter
          </Button>
        </div>
      )}

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
        {() => {
          // With an active group filter, `filtered` depends on `membershipMap`
          // (from the bulk membership read). A `null` map means that read is still
          // loading or has failed, not that zero repos match (finding 7): show the
          // shared loading/error presentation instead of the "no matches" empty
          // state until membership is actually known.
          if (activeGroupId !== null && membershipMap === null) {
            return (
              <AsyncPanel state={memberships}>
                {/* Unreachable: this branch only renders while membershipMap is
                    null, and AsyncPanel only calls children once state.data is
                    non-null (the outer condition above then takes over). */}
                {() => null}
              </AsyncPanel>
            );
          }

          return filtered.length === 0 ? (
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
              {filtered.map((repo) => {
                const repoGroups = (membershipMap?.get(repo.id) ?? [])
                  .map((gid) => groupById.get(gid))
                  .filter((g): g is GroupSummary => g !== undefined);
                return (
                  <RepoRow
                    key={repo.id}
                    repo={repo}
                    repoGroups={repoGroups}
                    busy={busyId === repo.id}
                    onOpen={() => setSelectedId(repo.id)}
                    onCheck={() => checkNow(repo.id)}
                  />
                );
              })}
            </div>
          );
        }}
      </AsyncPanel>

      <Drawer open={selectedId !== null} onClose={() => setSelectedId(null)}>
        {selectedId !== null && (
          <RepoDetailPanel
            id={selectedId}
            onChanged={handleRepoChanged}
            onClose={() => setSelectedId(null)}
          />
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
  repoGroups,
  busy,
  onOpen,
  onCheck,
}: {
  repo: RepoSummary;
  repoGroups: GroupSummary[];
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
        if (e.key === "Enter") {
          onOpen();
        } else if (e.key === " ") {
          e.preventDefault();
          onOpen();
        }
      }}
      className={cn(
        GRID,
        "cursor-pointer border-b border-border px-4 py-3 last:border-b-0 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
      )}
    >
      <div className="min-w-0">
        <div className="truncate font-mono text-sm font-semibold">{repo.localName}</div>
        <div className="truncate font-mono text-[11px] text-muted-foreground">{repo.hostType}</div>
        <IntelSignals
          latestReleaseTag={repo.latestReleaseTag}
          openPrCount={repo.openPrCount}
          className="mt-1"
        />
        {repoGroups.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-1">
            {repoGroups.map((g) => (
              <GroupChip key={g.id} group={g} />
            ))}
          </div>
        )}
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

function GroupChip({ group }: { group: GroupSummary }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-border px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground">
      <span
        className={cn("size-1.5 rounded-full", group.color === null && "bg-muted-foreground/50")}
        style={group.color ? { backgroundColor: group.color } : undefined}
      />
      {group.name}
    </span>
  );
}
