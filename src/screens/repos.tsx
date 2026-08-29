import { useCallback, useMemo, useState } from "react";
import { ArrowDown, ArrowUp, ChevronRight, Clock, FolderGit2, Plus, RefreshCw, Search, X } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { GroupSummary, RepoSummary } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DataTable, type DataTableColumn } from "@/components/ui/data-table";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { FilterChip } from "@/components/filter-chip";
import { StatusBadge } from "@/components/status-badge";
import { IntelSignals } from "@/components/intel-signals";
import { Drawer } from "@/components/ui/drawer";
import { RepoDetailPanel } from "@/components/repo-detail";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { PageShell } from "@/components/page-shell";
import { useToast } from "@/hooks/use-toast";
import { useBackendEvents, useRepoGroupMemberships, useRepoList } from "@/hooks/queries";
import { deriveStatus, relativeTime, STATUS_STYLE, type RepoStatus } from "@/lib/status";
import { cn } from "@/lib/utils";

const ALL_FILTER = { enabledOnly: null, hostType: null, query: null };
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
  const toast = useToast();

  const [busyId, setBusyId] = useState<number | null>(null);
  const [checkAllBusy, setCheckAllBusy] = useState(false);
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

  const groupsForRepo = useCallback(
    (repoId: number): GroupSummary[] =>
      (membershipMap?.get(repoId) ?? [])
        .map((gid) => groupById.get(gid))
        .filter((g): g is GroupSummary => g !== undefined),
    [membershipMap, groupById],
  );

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

  // "Check all" (BL-NI-86, repo_check_all has no consumer), provisional per the
  // N2 PR. `repoCheckAll` returns a COUNT ATTEMPTED, not succeeded - per-repo
  // outcomes (including failures) arrive later via `repo:check-completed`,
  // which `useBackendEvents` above already refetches on. Following the
  // `checkNow` honesty idiom: a call that only tells us "N repos were asked
  // to check" can never be reported with the "ok" (success-styled) toast, so
  // this uses "info" even when the call itself resolves cleanly.
  const checkAll = useCallback(async () => {
    setCheckAllBusy(true);
    try {
      const count = await unwrap(commands.repoCheckAll());
      if (count === 0) {
        toast("info", "No enabled repos to check");
      } else {
        toast(
          "info",
          `Checking ${count} ${count === 1 ? "repo" : "repos"}`,
          "Results will appear on each row as checks complete.",
        );
      }
    } catch (e) {
      toast("error", "Could not check all", e instanceof IpcError ? e.message : String(e));
    } finally {
      setCheckAllBusy(false);
    }
  }, [toast]);

  const counts = useMemo(() => {
    const c: Record<RepoStatus, number> = {
      sync: 0,
      ahead: 0,
      behind: 0,
      dirty: 0,
      failed: 0,
      paused: 0,
      noUpstream: 0,
    };
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

  // Columns, in the ratified order (README settled list + ui-delivery-plan.md
  // ledger B5): Repository (first, frozen, the only flexible width), Status,
  // Ahead, Behind, Groups, Checked, then the unlabeled actions column.
  //
  // Branch and Folder are OMITTED. `RepoSummary` in `src/lib/bindings.ts` (this
  // branch, off origin/main) carries neither `branch`/`activeBranch` nor
  // `localPath` - only `RepoDetail` has them. Per the hard constraint, this
  // change does not touch `src-tauri`/`crates` to add them; see the N2 PR body
  // and the new backlog row for the follow-up.
  const columns: DataTableColumn<RepoSummary>[] = useMemo(
    () => [
      {
        id: "repo",
        header: "Repository",
        width: "minmax(180px,240px)",
        frozen: true,
        cell: (r) => (
          <div className="min-w-0 py-2">
            <div className="truncate font-mono text-sm font-semibold">{r.localName}</div>
            <div className="truncate font-mono text-[11px] text-muted-foreground">{r.hostType}</div>
            <IntelSignals
              latestReleaseTag={r.latestReleaseTag}
              openPrCount={r.openPrCount}
              className="mt-1"
            />
          </div>
        ),
      },
      {
        id: "status",
        header: "Status",
        width: "124px",
        cell: (r) => {
          const status = deriveStatus(r);
          const count =
            status === "behind" ? (r.behindCount ?? 0) : status === "ahead" ? (r.aheadCount ?? 0) : undefined;
          return <StatusBadge status={status} count={count} />;
        },
      },
      {
        id: "ahead",
        header: "Ahead",
        width: "64px",
        align: "right",
        icon: ArrowUp,
        cell: (r) => ((r.aheadCount ?? 0) > 0 ? <span className="font-mono text-xs tabular-nums">{r.aheadCount}</span> : null),
      },
      {
        id: "behind",
        header: "Behind",
        width: "64px",
        align: "right",
        icon: ArrowDown,
        cell: (r) =>
          (r.behindCount ?? 0) > 0 ? <span className="font-mono text-xs tabular-nums">{r.behindCount}</span> : null,
      },
      {
        id: "groups",
        header: "Groups",
        width: "160px",
        cell: (r) => {
          const gs = groupsForRepo(r.id);
          if (gs.length === 0) return null;
          return (
            <div className="flex min-w-0 flex-nowrap gap-1 overflow-hidden">
              {gs.map((g) => (
                <GroupChip key={g.id} group={g} />
              ))}
            </div>
          );
        },
      },
      {
        id: "checked",
        header: "Checked",
        width: "96px",
        icon: Clock,
        cell: (r) => <span className="font-mono text-xs text-muted-foreground">{relativeTime(r.lastCheckedAt)}</span>,
      },
    ],
    [groupsForRepo],
  );

  // Search and the status chips ride in PageShell's sticky header rather than
  // scrolling away with the table. Filters that leave the screen force a scroll
  // back up to change what you are looking at, which is the opposite of what a
  // filter is for. Behaviour is unchanged; only where it renders moved.
  const toolbar =
    list.length > 0 ? (
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
            {/* Provisional (N2 PR, veto invited): closes BL-NI-86, repo_check_all
                has no consumer. */}
            <Button
              variant="outline"
              size="sm"
              className="ml-auto"
              disabled={checkAllBusy}
              onClick={checkAll}
            >
              <RefreshCw className={checkAllBusy ? "animate-spin" : undefined} />
              Check all
            </Button>
          </div>
    ) : undefined;

  return (
    <PageShell
      title="Repos"
      actions={
        <Button size="sm" onClick={() => setAddOpen(true)}>
          <Plus /> Add repos
        </Button>
      }
      toolbar={toolbar}
    >

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
            <DataTable
              aria-label="Tracked repositories"
              columns={columns}
              rows={filtered}
              rowKey={(r) => r.id}
              onRowClick={(r) => setSelectedId(r.id)}
              actions={(r) => (
                <>
                  <Button
                    variant="ghost"
                    size="icon"
                    disabled={busyId === r.id}
                    title="Check now"
                    onClick={() => checkNow(r.id)}
                  >
                    <RefreshCw className={busyId === r.id ? "animate-spin" : undefined} />
                  </Button>
                  <ChevronRight aria-hidden className="size-4 text-muted-foreground" />
                </>
              )}
            />
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
    </PageShell>
  );
}

function GroupChip({ group }: { group: GroupSummary }) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-border px-1.5 py-0.5 text-[10px] font-medium whitespace-nowrap text-muted-foreground">
      <span
        className={cn("size-1.5 shrink-0 rounded-full", group.color === null && "bg-muted-foreground/50")}
        style={group.color ? { backgroundColor: group.color } : undefined}
      />
      {group.name}
    </span>
  );
}
