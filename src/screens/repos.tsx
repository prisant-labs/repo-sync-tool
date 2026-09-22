import { useCallback, useMemo, useState } from "react";
import {
  ArrowDown,
  ArrowUp,
  ChevronRight,
  Clock,
  Folder,
  FolderGit2,
  GitBranch,
  GitFork,
  Plus,
  RefreshCw,
  Search,
  Star,
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
import { RepoDetailPanel, REPO_DETAIL_TITLE_ID } from "@/components/repo-detail";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { PageShell } from "@/components/page-shell";
import { useToast } from "@/hooks/use-toast";
import { useBackendEvents, useRepoGroupMemberships, useRepoList } from "@/hooks/queries";
import { groupScope } from "@/lib/group-scope";
import {
  deriveStatus,
  relativeTime,
  STATUS_ORDER,
  STATUS_STYLE,
  type RepoStatus,
} from "@/lib/status";
import { cn } from "@/lib/utils";

const ALL_FILTER = { enabledOnly: null, hostType: null, query: null };

type Chip = RepoStatus | "all";

export function ReposScreen({
  activeGroupId,
  groups,
  onClearGroup,
  onGroupsChanged,
  addOpen,
  onAddOpenChange,
  onLibraryChanged,
}: {
  activeGroupId: number | null;
  groups: GroupSummary[];
  onClearGroup: () => void;
  onGroupsChanged: () => void;
  /**
   * L4: whether the Add-repositories dialog is open. The STATE lives in the
   * shell; the dialog itself stays here.
   *
   * That split is not arbitrary. The dialog has to stay on this screen
   * because `repo_add` emits no backend event, so `onAdded` wired to this
   * screen's own refetch is the only thing that makes a new repo appear in
   * the table. But the sidebar's add button lives above this screen and
   * outlives it - this component unmounts on every navigation - so a flag
   * owned here could not survive the very navigation the button performs.
   *
   * Controlled rather than a request counter plus an effect: an effect that
   * calls `setState` synchronously is a cascading render, and the lint rule
   * that says so is right.
   */
  addOpen: boolean;
  onAddOpenChange: (open: boolean) => void;
  /**
   * Refreshes the SHELL's own repo list, summary and membership snapshots.
   *
   * Every mutation on this screen has to call it, because none of them emit a
   * backend event: `repo_add`, `repo_remove` and a group-membership toggle all
   * change what the sidebar shows and announce nothing (Codex review of PRs
   * #93-#96, finding 2). Refreshing only this screen's copy leaves the count
   * beside Repos and the attention dot describing a library that no longer
   * exists.
   */
  onLibraryChanged: () => void;
}) {
  const repos = useRepoList(ALL_FILTER);
  const refetch = repos.refetch;
  useBackendEvents(refetch);
  const toast = useToast();

  const [busyId, setBusyId] = useState<number | null>(null);
  const [checkAllBusy, setCheckAllBusy] = useState(false);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const [query, setQuery] = useState("");
  const [chip, setChip] = useState<Chip>("all");

  const list = useMemo(() => repos.data ?? [], [repos.data]);

  // Group memberships for every tracked repo, as Map<repoId, groupId[]>, fetched
  // in a single bulk call (see useRepoGroupMemberships; BL-NI-22). A repo with no
  // groups is absent from the map; `null` means the read is loading or failed.
  const memberships = useRepoGroupMemberships();
  const membershipMap = memberships.data;
  const refetchMemberships = memberships.refetch;

  /**
   * The one authority for "is this repo inside the engaged group", shared with
   * the sidebar and the Dashboard.
   *
   * This screen used to answer that question inline, three separate times, and
   * `group-scope.ts`'s own header says it exists to stop exactly that (E-20
   * AC-19). The populations below are still deliberately different - the chip
   * counts apply the name filter and the group pill's count does not - but the
   * MEMBERSHIP RULE underneath them is now derived once, so the two can only
   * disagree where they are meant to.
   */
  const scope = useMemo(() => groupScope(activeGroupId, membershipMap), [activeGroupId, membershipMap]);

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
  // Also the drawer's remove and its group toggles, which is why the shell is
  // told here too: `repo_remove` emits no event, and removing the LAST
  // repository leaves nothing for a scheduler tick to check, so no later event
  // would ever correct the sidebar.
  const handleRepoChanged = useCallback(() => {
    refetch();
    refetchMemberships();
    onGroupsChanged();
    onLibraryChanged();
  }, [refetch, refetchMemberships, onGroupsChanged, onLibraryChanged]);

  // The Folder cell's click target (walk item R5). Errors get a toast rather
  // than being swallowed the way `checkNow` swallows its own: a failed folder
  // open produces NO other signal anywhere - no activity row, no event, no
  // row state change - so silence would leave the user clicking a button that
  // looks broken. `repo_open_folder` fails for reasons a user can act on (the
  // path was moved, renamed or deleted since the last check), which is exactly
  // when they need to be told.
  const openFolder = useCallback(
    async (id: number, name: string) => {
      try {
        await unwrap(commands.repoOpenFolder(id));
      } catch (e) {
        toast("error", `Could not open ${name}`, e instanceof IpcError ? e.message : String(e));
      }
    },
    [toast],
  );

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

  /**
   * The population the status chips actually filter: the group and name
   * filters applied, the STATUS filter deliberately not - that is the
   * dimension the chips themselves select, so counting after it would make
   * every chip read 0 except the engaged one.
   *
   * `null` means the bulk membership read has not resolved while a group is
   * engaged, so the population is not knowable yet - the same rule
   * `inGroupCount` below already follows, and the reason the counts are not
   * simply zero in that window.
   *
   * This used to count `list`, the whole unfiltered library, which meant that
   * with a group engaged the toolbar read "All 2 / In sync 2" above a single
   * row. A count attached to a filter has to count what that filter will
   * actually show, or it is not a count of anything the user can see.
   */
  const countBase = useMemo(() => {
    if (scope.pending) return null;
    const q = query.trim().toLowerCase();
    return list.filter((r) => {
      if (!scope.includes(r.id)) return false;
      if (q && !r.localName.toLowerCase().includes(q)) return false;
      return true;
    });
  }, [list, query, scope]);

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
    for (const r of countBase ?? []) c[deriveStatus(r)] += 1;
    return c;
  }, [countBase]);

  // Repos in the active group (before the status / name filters narrow
  // further). `null` means "not yet known" (the membership read is still loading
  // or failed), distinct from a genuine zero (finding 7 / BL-NI-27's sibling
  // defect in the E-16 spec: a null map must never read as "no members").
  const inGroupCount = useMemo(() => scope.countRepos(list), [list, scope]);

  // The rows are the same population as the chip counts, with the status
  // dimension applied. Sharing `countBase` is what keeps the two from
  // drifting: the counts cannot describe a different set than the table shows.
  const filtered = useMemo(
    () => (countBase ?? []).filter((r) => chip === "all" || deriveStatus(r) === chip),
    [countBase, chip],
  );

  // Columns, in the ratified order (README settled list + ui-delivery-plan.md
  // ledger B5): Repository (first, frozen, the only flexible width), Status,
  // Branch, Ahead, Behind, Groups, Folder, Checked, Stars, Forks, then the
  // unlabeled actions column.
  //
  // Branch and Folder shipped in the N2 fix round: `RepoSummary` gained
  // `activeBranch` and `localPath` in PR #74 (`feat/summary-fields-and-check-
  // summary`), closing the gap BL-NI-91 recorded when N2 first shipped without
  // them.
  //
  // Stars and Forks ship in N3 (this change): the table lab's `gen_lab2.py`
  // COLS tuple carries `(key, label, width, kind, on, mig)` for all five
  // metadata columns PR #72 (N1) added to `RepoSummary` - only `stars` and
  // `forks` default ON (`("stars","Stars","76px","n",1,1)`,
  // `("forks","Forks","76px","n",1,1)`); `license`, `size` and `visibility` all
  // default OFF (`0`) and are therefore NOT rendered here - the lab's default
  // on/off set decides which render, per the N3 task, since column show/hide
  // is itself still deferred. Extracted values, stated here for veto:
  //   - width 76px, kind "n" (number: label left in the header, value right in
  //     the cell, per the lab's own alignment default), for both.
  //   - icons: `STAR` / `FORK` in the lab's own glyph set, mapped to lucide's
  //     `Star` / `GitFork` (the closest stock equivalents; the lab draws its
  //     own inline SVG paths rather than naming a library icon).
  //   - the lab's `cell()` does NOT give stars/forks the muted ".meta" class
  //     it gives Checked/License/Size/Visibility, so these render at the
  //     normal (non-muted) cell ink, matching Ahead/Behind's own treatment.
  //   - the lab's mock data shows abbreviated values ("1.2k"). No abbreviation
  //     rule is ratified anywhere else in this codebase, so this renders the
  //     exact integer GitHub reports, in mono tabular-nums like Ahead/Behind -
  //     a deviation from the lab's mock, flagged here for veto.
  //   - a real zero renders as "0", not the dash: `stars`/`forks` are `Option`
  //     fields whose OWN doc comment says "never a fabricated zero", meaning a
  //     genuine 0 is real data GitHub reported, not an absence. Only `None`
  //     (never-refreshed / non-GitHub) renders the dash. This is the opposite
  //     rule from Ahead/Behind, whose `0` IS the "nothing to show" case for a
  //     repo that is not ahead or behind at all - the two columns look similar
  //     but the meaning of zero differs, so the two `cell()` functions differ
  //     on purpose.
  //
  // `license`, `size` and `visibility` remain unrendered (lab-default-off,
  // per the note above) - `size` arrives from GitHub in KILOBYTES
  // (`RepoSummary.size`'s own doc comment), unhumanized, which would only
  // matter once that column ships.
  //
  // The two round-five web-link glyphs (globe to the repo's web URL, link
  // glyph to `homepage`) ratified for this identity cell are NOT built here:
  // see BL-NI-94 (web link glyphs need two backend gaps) in `docs/backlog.md`
  // and the PR body. In short, `RepoSummary` (unlike `RepoDetail`) carries no
  // `remoteOriginUrl` to gate the globe's "hidden when null" requirement on,
  // and no IPC command opens an arbitrary URL like `homepage` at all (only
  // `repoOpenRemote`, which opens the git remote specifically) - both are
  // `crates/reposync-core` / `src-tauri` changes, out of scope for this
  // `src/`-only slice per the shell-crate chokepoint.
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
        // 116px, not 104: RR6's longest label ("no commits") wrapped onto two
        // lines inside a fixed 52px row at the old width. Caught in a
        // screenshot, not by a test - nothing in jsdom measures text.
        width: "116px",
        icon: GitBranch,
        cell: (r) => {
          // RR6: an empty Branch cell has three distinct causes, and this now
          // says which. `activeBranch` is `null` for all three; `isDetached`
          // could only ever separate one of them, so the other two - an unborn
          // HEAD and a repo nothing has inspected - collapsed into one bare
          // dash that explained neither.
          //
          // `headState` is what the last inspection OBSERVED (migration 0011).
          // `null` there means no inspection has recorded it, which is the
          // "never run" case, and it is why this reads `headState` rather than
          // inferring from `headSha`: no head SHA also happens when the commit
          // exists but cannot be read, and labelling a damaged repo "no
          // commits" would be a confident wrong answer.
          //
          // Wording is jp's own, from the round-three row bench: "detached",
          // "no commits", "never run". Each is muted ink plus the column's
          // GitBranch glyph, which only appears when this returns non-null - so
          // a labelled reason still reads as subordinate to a real branch name.
          if (r.activeBranch !== null) return r.activeBranch;
          const reason =
            r.headState === "detached"
              ? "detached"
              : r.headState === "unborn"
                ? "no commits"
                : r.headState === null
                  ? // NULL is TWO facts, not one, and telling them apart needs a
                    // second field. It means "no inspection recorded this",
                    // which is "never run" for a repo nothing has looked at -
                    // and ALSO what an inspection writes when it looked and
                    // could not tell, because HEAD would not read (the Rust
                    // side stopped calling that "unborn" in this same change).
                    // Saying "never run" about a repo checked ten minutes ago
                    // would just swap one confident wrong answer for another.
                    r.lastCheckedAt === null
                    ? "never run"
                    : "unreadable"
                  : // `branch` with no `activeBranch` is not a state inspect can
                    // produce, so there is nothing honest to say about it.
                    null;
          return reason === null ? null : (
            <span className="text-muted-foreground">{reason}</span>
          );
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
        // `DataTableColumn`.
        //
        // This cell used to be a plain `<span>` carrying an "Open in File
        // Explorer" tooltip and no click handler, deliberately: wiring it was
        // called new interactive scope and deferred to the maintainer. He
        // ruled on it in the 2026-09-14 walk-through (item R5, "this needs to
        // function"), so it is now a real button.
        //
        // `stopPropagation` matters: the row's `onRowClick` opens the detail
        // drawer, so without it every folder-open would also open the panel
        // behind it. It is also a genuine tab stop, which the row itself is
        // not (see the actions column: the row lost `role="button"`/`tabIndex`
        // because nesting a keyboard-operable row around a button produced an
        // invalid accessibility tree). So this is the only keyboard path to a
        // repo's folder from the list.
        cell: (r) => (
          <button
            type="button"
            title="Open in File Explorer"
            aria-label={`Open ${r.localName} in File Explorer`}
            onClick={(e) => {
              e.stopPropagation();
              void openFolder(r.id, r.localName);
            }}
            // `w-full` is load-bearing, not cosmetic. Content-sized, the button
            // covered only the path text, so the leftover space in the same
            // cell still bubbled to the row and opened the drawer instead -
            // one cell with two outcomes decided by which pixel you hit, and
            // nothing in the styling marked the boundary (Codex adversarial
            // review, finding 1, 2026-09-14). Full-width makes the whole
            // Folder cell do the one thing its column header names.
            className="inline-flex w-full min-w-0 items-center gap-1.5 truncate rounded-sm text-left font-mono text-[11px] font-medium text-muted-foreground hover:text-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Folder aria-hidden className="size-3 shrink-0 opacity-70" />
            <span className="truncate">{r.localPath}</span>
          </button>
        ),
      },
      {
        id: "checked",
        header: "Checked",
        width: "96px",
        icon: Clock,
        cell: (r) => <span className="font-mono text-xs text-muted-foreground">{relativeTime(r.lastCheckedAt)}</span>,
      },
      {
        id: "stars",
        header: "Stars",
        width: "76px",
        align: "right",
        icon: Star,
        // `null` (never refreshed / non-GitHub) is the dash; a real `0` is
        // GitHub's actual answer and renders as "0" - see the doc comment
        // above the column list for why this differs from Ahead/Behind.
        cell: (r) => (r.stars === null ? null : <span className="font-mono text-xs tabular-nums">{r.stars}</span>),
      },
      {
        id: "forks",
        header: "Forks",
        width: "76px",
        align: "right",
        icon: GitFork,
        cell: (r) => (r.forks === null ? null : <span className="font-mono text-xs tabular-nums">{r.forks}</span>),
      },
    ],
    [groupsForRepo, openFolder],
  );

  // ONE toolbar (N5, sidebar restructure and toolbar consolidation;
  // ui-delivery-plan.md ledger B1 / round-five correction, coverage-matrix.md
  // section 11): search, the status filter chips and the
  // group filter control together, all riding in PageShell's sticky header
  // rather than scrolling away with the table. Filters that leave the screen
  // force a scroll back up to change what you are looking at, which is the
  // opposite of what a filter is for.
  //
  // The group control folds in what used to be a separate "Filtered to X /
  // Clear filter" banner rendered below the header (see the removed block
  // near the old `activeGroup &&` site). DECISION FLAGGED FOR VETO in the PR
  // body: coverage-matrix.md section 3 KEEPs that banner as its own row, but
  // once the group control here already shows the active group (dot, name,
  // count) and offers Clear, the banner is a second copy of the same
  // information - the later, more specific round-five toolbar-consolidation
  // decision supersedes that KEEP row rather than sitting beside it.
  //
  // CORRECTED post-review: search, the status chips and Check all stay gated
  // by `list.length > 0` (nothing to search, filter or check-all against an
  // empty list), but the GROUP CONTROL is not part of that gate any more. A
  // Codex adversarial review caught the first cut's real bug: gating the
  // whole toolbar on `list.length` meant an emptied library with an active
  // group filter showed no clear affordance at all, and the filter would
  // silently reapply the moment a repo reappeared - `activeGroupId` lives in
  // `AppShell`, entirely independent of this screen's own repo list, so
  // nothing about an empty `list` ever clears it. The control now renders
  // whenever `activeGroup` is set, full stop, so the filter stays visible and
  // clearable through every state the list can be in - the sidebar's "All
  // repositories" row was never the ONLY way to clear it; it is one of two,
  // as it always should have been.
  const toolbar =
    list.length > 0 || activeGroup ? (
          <div className="flex flex-wrap items-center gap-3">
            {list.length > 0 && (
              <>
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
                  {/* `countBase?.length` rather than a `?? 0`: an unknown
                      population shows the chip with NO number, never a
                      fabricated zero. */}
                  <FilterChip
                    label="All"
                    count={countBase?.length}
                    active={chip === "all"}
                    onClick={() => setChip("all")}
                  />
                  {/* A chip renders when it has something to show OR when it is
                      the SELECTED filter, even at zero (AC-17, Codex review of
                      PRs #93-#96, finding 3).

                      `counts[s] > 0` alone was safe while the counts were the
                      whole library: a selected chip could not reach zero without
                      the library itself emptying. Scoping the counts to the group
                      and the search (the fix in PR #95) broke that. Select
                      Behind, then search for an in-sync repo, and the Behind chip
                      unmounts while `chip` stays "behind" - so the table filters
                      to nothing and the only filter still applied is the one
                      control no longer on screen. An empty table with a visible
                      reason is fine; an empty table with an invisible one is the
                      defect. */}
                  {STATUS_ORDER.map(
                    (s) =>
                      (counts[s] > 0 || chip === s) && (
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
              </>
            )}
            {activeGroup && (
              <GroupFilterControl group={activeGroup} count={inGroupCount} onClear={onClearGroup} />
            )}
            {/* Provisional (N2 PR, veto invited): closes BL-NI-86, repo_check_all
                has no consumer. */}
            {list.length > 0 && (
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
            )}
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
        <Button size="sm" onClick={() => onAddOpenChange(true)}>
          <Plus /> Add repos
        </Button>
      }
      toolbar={toolbar}
    >

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
                <Button onClick={() => onAddOpenChange(true)}>
                  <Plus /> Add repositories
                </Button>
              }
            />
          }
        >
          {() => {
            // With an active group filter, `filtered` depends on the bulk
            // membership read. `scope.pending` means that read is still loading or
            // has failed, not that zero repos match (finding 7): show the shared
            // loading/error presentation instead of the "no matches" empty state
            // until membership is actually known.
            if (scope.pending) {
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

      <Drawer
        open={selectedId !== null}
        onClose={() => setSelectedId(null)}
        size="wide"
        aria-labelledby={REPO_DETAIL_TITLE_ID}
      >
        {selectedId !== null && (
          <RepoDetailPanel
            id={selectedId}
            onChanged={handleRepoChanged}
            onClose={() => setSelectedId(null)}
          />
        )}
      </Drawer>

      <AddReposDialog
        open={addOpen}
        onClose={() => onAddOpenChange(false)}
        onAdded={() => {
          refetch();
          onLibraryChanged();
        }}
      />
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

/**
 * The Repos toolbar's group control (N5): what the old standalone "Filtered
 * to X / Clear filter" banner folded into. Shows the active group exactly the
 * way the sidebar and the Groups column do - a colour dot plus name - plus
 * the count of repos in it, and offers Clear. Selecting a DIFFERENT group
 * still happens from the sidebar (GroupsNav); this control only displays and
 * clears the filter the sidebar set, per the withdrawn F1/F2 group-filter
 * proposals in coverage-matrix.md ("group filtering already works from the
 * sidebar").
 *
 * `count === null` means the bulk membership read is still loading or failed
 * (BL-NI-22 (O(N) group filter)'s sibling honesty finding, coverage-matrix.md
 * section 3): this renders an ellipsis, never a fabricated zero.
 *
 * Wording restored post-review. The first cut dropped the fold to a bare
 * "[dot] Work 1 [Clear filter]" sequence: visually a name and a number with
 * nothing saying what the number counts, and for a screen reader an
 * unlabelled mono "1" with no relationship to "Work" spoken before it. The
 * old banner's explicit "Filtered to X" plus "N repo(s)" wording is restored
 * here instead of only in an ARIA attribute, per jp's show-the-meaning-up-
 * front preference - a sighted user gets the same explicit sentence a screen
 * reader does, not two different experiences of the same control.
 */
function GroupFilterControl({
  group,
  count,
  onClear,
}: {
  group: GroupSummary;
  count: number | null;
  onClear: () => void;
}) {
  return (
    <div className="flex items-center gap-2 rounded-full border border-border bg-muted/40 py-1 pr-1.5 pl-3 text-sm">
      <span className="text-muted-foreground">Filtered to</span>
      <span
        className={cn("size-2.5 shrink-0 rounded-full", group.color === null && "bg-muted-foreground/50")}
        style={group.color ? { backgroundColor: group.color } : undefined}
      />
      <span className="font-semibold">{group.name}</span>
      <span className="font-mono text-xs text-muted-foreground">
        {count === null ? "…" : `${count} ${count === 1 ? "repo" : "repos"}`}
      </span>
      <Button
        variant="ghost"
        size="sm"
        className="h-6 gap-1 px-1.5 text-xs"
        onClick={onClear}
        aria-label={`Clear ${group.name} filter`}
      >
        <X />
        Clear filter
      </Button>
    </div>
  );
}
