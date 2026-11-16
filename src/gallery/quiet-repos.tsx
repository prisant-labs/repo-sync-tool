import { useMemo, useState } from "react";
import { ChevronRight, FolderGit2, PauseCircle, Plus, RefreshCw, Search, X } from "lucide-react";
import { Cell } from "@/gallery/cells";
import { GROUPS } from "@/gallery/fixtures";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { DataTable, type DataTableColumn, type DataTableSort } from "@/components/ui/data-table";
import { ColumnsPopover } from "@/components/ui/columns-popover";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { FilterChip } from "@/components/filter-chip";
import { StatusBadge } from "@/components/status-badge";
import { GroupChip } from "@/components/recipes/group-chip";
import { PageShell } from "@/components/page-shell";
import { ReposScreen } from "@/screens/repos";
import { useRepoGroupMemberships, useRepoList } from "@/hooks/queries";
import { deriveStatus, relativeTime, STATUS_ORDER, STATUS_STYLE, type RepoStatus } from "@/lib/status";
import { cn } from "@/lib/utils";
import type { GroupSummary, RepoSummary } from "@/lib/bindings";

/**
 * A PROPOSAL, not shipped. The Repos table with the polish bar applied
 * (`_local/audit/2026-11-13_fable52-plan/06_polish-bar.md` in the main
 * checkout), drawn under today's real screen from the same fixtures.
 *
 * It is composed here from the real primitives (PageShell, DataTable,
 * FilterChip, StatusBadge, GroupChip, ColumnsPopover) rather than by changing
 * `screens/repos.tsx`, so the shipped app renders exactly as it did. Two
 * things could not be composed and needed an opt-in prop on `DataTable`
 * (`headerClassName`, and `headerAlign` on a column); their defaults render
 * as before. FilterChip's monospace is overridden from a wrapper below, which
 * is a gallery-only trick: adopting this needs a real prop on FilterChip.
 *
 * What it changes, rule by rule:
 *   - Healthy rows carry no colour and no chip. "Normal" means in sync,
 *     ahead or behind: the Status column is blank for those, and only dirty,
 *     failed and no-upstream draw a chip. Paused is a muted word, because the
 *     user turned it off. Screen readers still hear the state.
 *   - Monospace only for branches, paths and versions. Names, counts, times,
 *     chips and headers are the sans face.
 *   - Sentence-case headers, no tracked capitals.
 *   - One fact, one place: the chip no longer says "3 ahead" beside an Ahead
 *     column that says 3. Behind keeps its colour on the number itself.
 *   - Even rhythm: every row is one line at the compact 44px height. The
 *     provider and the release and PR signals move onto the same line.
 *   - Toolbar: actions join the title row, so one band sits above the table.
 *   - Numbers are tabular and right-aligned, with the header over them.
 *     A known zero is blank; an unknown value keeps the dash.
 *   - Long paths keep their END visible and carry the full path as a tooltip.
 *   - No resting shadow on the table.
 */

const ALL_FILTER = { enabledOnly: null, hostType: null, query: null };
const noop = () => {};

type Chip = RepoStatus | "all";

/** The states that ask for the user's attention. Everything else is normal. */
const ATTENTION: ReadonlySet<RepoStatus> = new Set<RepoStatus>(["failed", "dirty", "noUpstream"]);

const COLUMN_ORDER = ["repo", "status", "branch", "ahead", "behind", "groups", "folder", "checked", "stars", "forks"];

/** The section: today's real screen, then the quiet proposal, stacked. */
export function QuietReposSection() {
  return (
    <>
      <Cell
        title="Today"
        note="the shipped Repos screen, unmodified"
        height={560}
        contained
      >
        <TodayRepos />
      </Cell>
      <Cell
        title="Quiet (proposal, not shipped)"
        note="the same fixtures through the same stubbed calls, with the polish bar applied. Search, the status filters, sorting and Columns work; Check all, Add repos, the row buttons, the folder link and a row click do nothing here"
        height={560}
        contained
      >
        <QuietRepos />
      </Cell>
    </>
  );
}

function TodayRepos() {
  const [addOpen, setAddOpen] = useState(false);
  return (
    <ReposScreen
      activeGroupId={null}
      groups={GROUPS}
      onClearGroup={noop}
      onGroupsChanged={noop}
      addOpen={addOpen}
      onAddOpenChange={setAddOpen}
      onLibraryChanged={noop}
    />
  );
}

/**
 * The status cell. Returns a node for every row, never `null`, because a
 * `null` cell draws DataTable's dash and a dash on every healthy row would be
 * the noise this proposal removes.
 */
function QuietStatus({ status }: { status: RepoStatus }) {
  if (ATTENTION.has(status)) return <StatusBadge status={status} className="font-sans" />;
  if (status === "paused") {
    return (
      <span className="inline-flex items-center gap-1.5 text-cell text-muted-foreground">
        <PauseCircle aria-hidden className="size-3.5 shrink-0" />
        Paused
      </span>
    );
  }
  return <span className="sr-only">{STATUS_STYLE[status].label}</span>;
}

/** A count that is blank at a known zero and a dash when unknown. */
function count(value: number | null, className?: string) {
  if (value === null) return null;
  if (value === 0) return "";
  return <span className={cn("text-cell tabular-nums", className)}>{value}</span>;
}

/** Why a branch cell is empty, in today's words (repos.tsx, RR6). */
function branchReason(r: RepoSummary): string | null {
  if (r.headState === "detached") return "detached";
  if (r.headState === "unborn") return "no commits";
  if (r.headState === null) return r.lastCheckedAt === null ? "never run" : "unreadable";
  return null;
}

function QuietRepos() {
  const repos = useRepoList(ALL_FILTER);
  const memberships = useRepoGroupMemberships();
  const [query, setQuery] = useState("");
  const [chip, setChip] = useState<Chip>("all");
  const [sort, setSort] = useState<DataTableSort | null>(null);
  const [visibility, setVisibility] = useState<Record<string, boolean>>({});
  const [order, setOrder] = useState(COLUMN_ORDER);

  const list = useMemo(() => repos.data ?? [], [repos.data]);
  const groupById = useMemo(() => new Map<number, GroupSummary>(GROUPS.map((g) => [g.id, g])), []);

  const named = useMemo(() => {
    const q = query.trim().toLowerCase();
    return q ? list.filter((r) => r.localName.toLowerCase().includes(q)) : list;
  }, [list, query]);

  const counts = useMemo(() => {
    const c: Record<RepoStatus, number> = { sync: 0, ahead: 0, behind: 0, dirty: 0, failed: 0, paused: 0, noUpstream: 0 };
    for (const r of named) c[deriveStatus(r)] += 1;
    return c;
  }, [named]);

  const rows = useMemo(
    () => named.filter((r) => chip === "all" || deriveStatus(r) === chip),
    [named, chip],
  );

  const columns: DataTableColumn<RepoSummary>[] = useMemo(
    () => [
      {
        id: "repo",
        header: "Repository",
        width: "minmax(180px,240px)",
        frozen: true,
        cell: (r) => {
          const paused = deriveStatus(r) === "paused";
          const prs = r.openPrCount ?? 0;
          return (
            <div className="flex min-w-0 items-baseline gap-2" title={`${r.localName} (${r.hostType})`}>
              <span className={cn("truncate text-identity", paused && "text-muted-foreground")}>{r.localName}</span>
              {r.latestReleaseTag && (
                <span className="shrink-0 font-mono text-meta text-muted-foreground" title={`Latest release ${r.latestReleaseTag}`}>
                  {r.latestReleaseTag}
                </span>
              )}
              {prs > 0 && (
                <span className="shrink-0 text-meta text-muted-foreground tabular-nums">
                  {prs} {prs === 1 ? "PR" : "PRs"}
                </span>
              )}
            </div>
          );
        },
        sortValue: (r) => r.localName.toLowerCase(),
      },
      {
        id: "status",
        header: "Status",
        width: "120px",
        cell: (r) => <QuietStatus status={deriveStatus(r)} />,
        sortValue: (r) => STATUS_ORDER.indexOf(deriveStatus(r)),
      },
      {
        id: "branch",
        header: "Branch",
        width: "116px",
        cell: (r) => {
          if (r.activeBranch !== null) {
            return (
              <span className="truncate font-mono text-cell text-muted-foreground" title={r.activeBranch}>
                {r.activeBranch}
              </span>
            );
          }
          const reason = branchReason(r);
          return reason === null ? null : <span className="text-cell text-muted-foreground">{reason}</span>;
        },
        sortValue: (r) => r.activeBranch?.toLowerCase(),
      },
      {
        id: "ahead",
        header: "Ahead",
        width: "64px",
        align: "right",
        headerAlign: "right",
        cell: (r) => count(r.aheadCount),
        sortValue: (r) => r.aheadCount,
      },
      {
        id: "behind",
        header: "Behind",
        width: "64px",
        align: "right",
        headerAlign: "right",
        cell: (r) => count(r.behindCount, "font-semibold text-status-behind"),
        sortValue: (r) => r.behindCount,
      },
      {
        id: "groups",
        header: "Groups",
        width: "160px",
        cell: (r) => {
          const gs = (memberships.data?.get(r.id) ?? [])
            .map((id) => groupById.get(id))
            .filter((g): g is GroupSummary => g !== undefined);
          if (gs.length === 0) return "";
          return (
            <div className="flex min-w-0 flex-nowrap gap-1 overflow-hidden">
              {gs.map((g) => (
                <GroupChip key={g.id} variant="compact" group={g} />
              ))}
            </div>
          );
        },
      },
      {
        id: "folder",
        header: "Folder",
        width: "minmax(150px,240px)",
        cell: (r) => (
          <button
            type="button"
            title={r.localPath}
            aria-label={`Open ${r.localName} in File Explorer`}
            onClick={(e) => e.stopPropagation()}
            // `dir="rtl"` moves the ellipsis to the START, so the end of the
            // path (the folder's own name) stays visible. The inner `bdi`
            // keeps the characters themselves in their normal order.
            dir="rtl"
            className="w-full min-w-0 truncate rounded-sm text-left font-mono text-meta text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <bdi dir="ltr">{r.localPath}</bdi>
          </button>
        ),
        sortValue: (r) => r.localPath.toLowerCase(),
      },
      {
        id: "checked",
        header: "Checked",
        width: "88px",
        cell: (r) => (
          <span className="text-cell whitespace-nowrap text-muted-foreground tabular-nums">{relativeTime(r.lastCheckedAt)}</span>
        ),
        sortValue: (r) => r.lastCheckedAt,
      },
      {
        id: "stars",
        header: "Stars",
        width: "72px",
        align: "right",
        headerAlign: "right",
        cell: (r) => (r.stars === null ? null : <span className="text-cell tabular-nums">{r.stars}</span>),
        sortValue: (r) => r.stars,
      },
      {
        id: "forks",
        header: "Forks",
        width: "72px",
        align: "right",
        headerAlign: "right",
        cell: (r) => (r.forks === null ? null : <span className="text-cell tabular-nums">{r.forks}</span>),
        sortValue: (r) => r.forks,
      },
    ],
    [memberships.data, groupById],
  );

  const sortedLabel = sort ? columns.find((c) => c.id === sort.columnId)?.header : undefined;

  // The sort note rides in the title row, not the toolbar: in the toolbar it
  // wrapped onto a band of its own, which is the rule this proposal is for.
  const actions =
    list.length > 0 ? (
      <>
        {sort && sortedLabel && (
          <Button variant="ghost" size="sm" className="text-muted-foreground" onClick={() => setSort(null)}>
            <X />
            Sorted by {sortedLabel}, {sort.direction === "asc" ? "ascending" : "descending"}
          </Button>
        )}
        <ColumnsPopover
          items={order.map((id) => ({ id, header: columns.find((c) => c.id === id)?.header ?? id, locked: id === "repo" }))}
          visibility={visibility}
          onToggle={(id) => id !== "repo" && setVisibility((v) => ({ ...v, [id]: v[id] === false }))}
          onReorder={setOrder}
        />
        <Button variant="outline" size="sm">
          <RefreshCw /> Check all
        </Button>
        <Button size="sm">
          <Plus /> Add repos
        </Button>
      </>
    ) : (
      <Button size="sm">
        <Plus /> Add repos
      </Button>
    );

  const toolbar =
    list.length > 0 ? (
      <div className="flex flex-wrap items-center gap-3">
        <div className="relative w-56">
          <Search className="pointer-events-none absolute top-1/2 left-2.5 size-4 -translate-y-1/2 text-muted-foreground" />
          <Input
            aria-label="Filter by name"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="Filter by name"
            className="pl-8"
            spellCheck={false}
          />
        </div>
        {/* FilterChip is monospace inside the component. The descendant
            selectors set the sans face from outside, for this proposal only.
            No `tone`: a filter is a control, not a status, so it carries no
            status colour until it is engaged. */}
        <div
          className="flex flex-wrap gap-1.5 [&_button]:font-sans [&_button_span]:font-sans [&_button_span]:tabular-nums"
          role="group"
          aria-label="Filter by status"
        >
          <FilterChip label="All" count={named.length} active={chip === "all"} onClick={() => setChip("all")} />
          {STATUS_ORDER.map(
            (s) =>
              (counts[s] > 0 || chip === s) && (
                <FilterChip
                  key={s}
                  label={STATUS_STYLE[s].label}
                  count={counts[s]}
                  active={chip === s}
                  onClick={() => setChip(s)}
                />
              ),
          )}
        </div>
      </div>
    ) : undefined;

  return (
    <PageShell title="Repos" fill actions={actions} toolbar={toolbar}>
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
                <Button>
                  <Plus /> Add repositories
                </Button>
              }
            />
          }
        >
          {() =>
            rows.length === 0 ? (
              <div className="rounded-xl border border-border bg-card py-16 text-center text-body text-muted-foreground">
                No repositories match this filter.
              </div>
            ) : (
              <DataTable
                aria-label="Tracked repositories, quiet proposal"
                className="shadow-none"
                headerClassName="font-sans text-cell font-medium tracking-normal normal-case"
                density="compact"
                columns={columns}
                rows={rows}
                rowKey={(r) => r.id}
                sort={sort}
                onSortChange={setSort}
                columnOrder={order}
                columnVisibility={visibility}
                onRowClick={noop}
                actions={() => (
                  <>
                    <Button variant="ghost" size="icon" title="Check now" aria-label="Check now">
                      <RefreshCw />
                    </Button>
                    <Button variant="ghost" size="icon" aria-label="Open details">
                      <ChevronRight aria-hidden />
                    </Button>
                  </>
                )}
              />
            )
          }
        </AsyncPanel>
      </div>
    </PageShell>
  );
}
