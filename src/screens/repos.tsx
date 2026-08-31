import { useCallback, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  Clock,
  Folder,
  FolderGit2,
  GitBranch,
  Plus,
  RefreshCw,
  Search,
  X,
} from "lucide-react";
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
  // N2 PR. `repoCheckAll` now resolves a structured `CheckAllSummary`
  // (`targeted`, `completed`, `succeeded`, `noResult`, `failedCheck`; PR #74)
  // rather than a bare count, specifically so this toast can report what
  // actually happened instead of inferring it (fix round after the Codex
  // review of PR #73, finding 3: a bare zero could mean "no enabled repos" OR
  // "every targeted repo failed," and the old wording ("Checking N repos...")
  // read as in-progress when the awaited call had already completed by the
  // time the toast fires).
  //
  // `targeted` is read from `select_check_all_targets`'s output before any
  // task is spawned (its own doc comment), so a zero THERE - and only there -
  // honestly means "no enabled repos." Every other branch below reports from
  // the completion counts, points a failure at Activity (the house idiom;
  // compare `lib/status.ts`'s `checkFailureMessage`), and never uses the
  // "ok" (success-styled) tone when `failedCheck` or `noResult` is nonzero.
  const checkAll = useCallback(async () => {
    setCheckAllBusy(true);
    try {
      const summary = await unwrap(commands.repoCheckAll());
      if (summary.targeted === 0) {
        toast("info", "No enabled repos to check");
      } else if (summary.failedCheck === 0 && summary.noResult === 0) {
        // completed === targeted and every completed check succeeded.
        toast("ok", `All ${summary.targeted} ${summary.targeted === 1 ? "repo" : "repos"} checked`);
      } else if (summary.completed === 0) {
        // Nothing produced a CheckResult at all - every targeted repo failed
        // before or during persistence (`noResult`'s doc comment), not an
        // operational fetch failure.
        toast("error", "Nothing produced a result", "See Activity for details.");
      } else {
        const problems = summary.failedCheck + summary.noResult;
        toast(
          "error",
          `${problems} of ${summary.targeted} ${problems === 1 ? "repo" : "repos"} failed`,
          "See Activity for details.",
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
  // Branch, Ahead, Behind, Groups, Folder, Checked, then the unlabeled actions
  // column.
  //
  // Branch and Folder shipped in this fix round: `RepoSummary` gained
  // `activeBranch` and `localPath` in PR #74 (`feat/summary-fields-and-check-
  // summary`), closing the gap BL-NI-91 recorded when N2 first shipped without
  // them. The five metadata fields PR #72 (N1) added to `RepoSummary`
  // (stars/forks/license/size/visibility, plus `homepage`) are now ALSO
  // available on the wire type, but adding those columns is explicitly out of
  // scope for this fix round (jp's scope ruling: only Branch and Folder).
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
        id: "branch",
        header: "Branch",
        width: "104px",
        icon: GitBranch,
        cell: (r) => {
          // `activeBranch` is `null` on three distinct conditions (its own doc
          // comment on `RepoSummary`): never inspected, a detached HEAD, and an
          // unborn HEAD. `isDetached` distinguishes only the detached case.
          // Rendering "detached" (rather than the empty dash) for that one case
          // is the honest middle ground: a repo checked out to a commit really
          // is on no branch, which is a fact worth a word, not silence: this is
          // color (muted ink) + icon (the column's own GitBranch glyph, which
          // only appears when this returns non-null) + word.
          if (r.activeBranch !== null) return r.activeBranch;
          if (r.isDetached) return <span className="text-muted-foreground">detached</span>;
          return null;
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
        id: "folder",
        header: "Folder",
        // The lab's own Folder width, `minmax(150px,240px)` - a range, not a
        // single fixed value, despite the README's "fixed widths for every
        // column except Repository" rule. Reproduced as the lab wrote it
        // rather than silently picking one end of the range.
        width: "minmax(150px,240px)",
        // No `icon`: the lab's Folder cell draws its own bespoke, differently
        // styled glyph (`.pth .fico`, muted and inline with the path text)
        // rather than using the generic muted data-icon slot every other
        // column uses - see the `icon` field's own doc comment on
        // `DataTableColumn`. Presentational only: this does not call
        // `repoOpenFolder` (that would be new interactive scope beyond what
        // was ratified for this fix round; flagged in the PR body).
        cell: (r) => (
          <span
            className="inline-flex min-w-0 items-center gap-1.5 truncate font-mono text-[11px] font-medium text-muted-foreground"
            title="Open in File Explorer"
          >
            <Folder aria-hidden className="size-3 shrink-0 opacity-70" />
            <span className="truncate">{r.localPath}</span>
          </span>
        ),
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
      // `fill`: lets the table region below claim a bounded height so
      // `DataTable` can own its own internal scroll (both axes) instead of
      // the whole page scrolling underneath a header that can never catch up
      // to it. See `page-shell.tsx`'s `fill` doc comment and `data-table.tsx`'s
      // scroll-ownership doc comment (fix round after the Codex review of
      // PR #73, finding 1).
      fill
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

      {/*
        `min-h-0` lets this region shrink below its content height when
        `PageShell`'s `fill` mode gives it a bounded height to work within,
        so `DataTable` inside can actually claim `max-h-full` and scroll
        internally rather than growing the page. Inert (a harmless no-op) in
        the loading/error/empty states, which size to their own content.
      */}
      <div className="flex min-h-0 flex-1 flex-col">
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
                    {/*
                      A real, focusable button, not a decorative icon (fix
                      round after the Codex review of PR #73, finding 2). It is
                      now THE keyboard path into the drawer: the row itself no
                      longer carries `role="button"`/`tabIndex` (nesting a
                      keyboard-operable row around Check now produced an
                      invalid accessibility tree and an ambiguous Enter/Space
                      target). A mouse click anywhere else on the row still
                      opens the drawer via `onRowClick` above, as a
                      convenience only.
                    */}
                    <Button
                      variant="ghost"
                      size="icon"
                      aria-label="Open details"
                      onClick={() => setSelectedId(r.id)}
                    >
                      <ChevronRight aria-hidden />
                    </Button>
                  </>
                )}
              />
            );
          }}
        </AsyncPanel>
      </div>

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
