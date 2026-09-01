import { useMemo, useState } from "react";
import { CheckCircle2, ChevronRight, Clock, History, XCircle } from "lucide-react";
import { Button } from "@/components/ui/button";
import { DataTable, type DataTableColumn } from "@/components/ui/data-table";
import { Drawer } from "@/components/ui/drawer";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { FilterChip } from "@/components/filter-chip";
import { ActivityReceipt, ACTIVITY_RECEIPT_TITLE_ID } from "@/components/activity-receipt";
import { PageShell } from "@/components/page-shell";
import { useActivity, useRepoList } from "@/hooks/queries";
import { ACTIVITY_PAGE_LIMIT, paginate, toActivityFilter } from "@/lib/activity";
import type { ActionTypeFilter, StatusFilter } from "@/lib/activity";
import type { ActivityRecord } from "@/lib/bindings";
import { relativeTime } from "@/lib/status";
import { cn } from "@/lib/utils";

const ALL_REPOS = { enabledOnly: null, hostType: null, query: null };

const ACTION_CHIPS: { value: ActionTypeFilter; label: string }[] = [
  { value: "all", label: "All actions" },
  { value: "check", label: "Checks" },
  { value: "update", label: "Updates" },
];

const STATUS_CHIPS: { value: StatusFilter; label: string; tone?: string }[] = [
  { value: "all", label: "Any outcome" },
  { value: "success", label: "Succeeded" },
  { value: "failed", label: "Failed", tone: "text-destructive" },
];

export function ActivityScreen() {
  const [actionType, setActionType] = useState<ActionTypeFilter>("all");
  const [status, setStatus] = useState<StatusFilter>("all");

  // Built fresh each render, which is fine and deliberate: `useActivity` keys its
  // effect on the primitive fields, not on the object identity, so a new object
  // with the same values does not refetch.
  //
  // The filter goes to the BACKEND rather than narrowing the rows already on
  // screen. `activity_list` has accepted these three fields since E-09 and
  // applies them in SQL before its LIMIT, so filtering server-side searches the
  // whole log. Filtering client-side would search only the capped page, and an
  // audit trail that answers "no failures" when it means "none in the last 60
  // rows" is worse than one with no filter at all.
  //
  // A group filter is deliberately NOT among these fields (N3, BL-NI-93 (group
  // filter needs a repo-set constraint)): the same "before the LIMIT" rule that
  // justifies action/outcome going server-side rules out a repo-membership
  // filter that only the frontend can express today. See that backlog row.
  const activity = useActivity(toActivityFilter(actionType, status));
  // The activity row carries a repoId and no name, so the names come from the
  // repo list. A repo REMOVED after its rows were written has no entry here, and
  // that is the honest outcome: the receipt says "Unknown repo" rather than
  // inventing one or hiding the row, and the audit trail stays complete.
  const repos = useRepoList(ALL_REPOS);
  const [selectedId, setSelectedId] = useState<number | null>(null);

  const repoNames = useMemo(
    () => new Map((repos.data ?? []).map((r) => [r.id, r.localName])),
    [repos.data],
  );

  const selected = (activity.data ?? []).find((r) => r.id === selectedId) ?? null;

  // Whether a filter is narrowing the view, so an empty result can say which
  // kind of empty it is. Without this, selecting "Updates" plus "Failed" on a
  // healthy library renders "No activity yet", which reads as "RepoSync has
  // never done anything" when it actually means "nothing has ever gone wrong".
  const filtered = actionType !== "all" || status !== "all";

  // Columns, in the ratified order (N3, ui-delivery-plan.md ledger B6): Time
  // (never wraps, a round-five correction), Repository, Action (its own column,
  // closing part of BL-NI-89 (Activity debug rows)), Outcome (the existing
  // `OutcomeChip`, now in its own column instead of sharing the summary cell),
  // Summary (the flexible column - a `minmax` range like Repos' own Repository
  // column, not a bare `1fr`, so it does not fight the primitive's own
  // decorative filler track for leftover width).
  //
  // The table lab (`_local/gui/2026-08-28_iterations/`) never modeled Activity;
  // every width and the Summary range below is this change's own judgement,
  // flagged in the PR body for veto, not a lab-sourced value the way the Repos
  // columns are. No column is frozen: five fixed-ish columns plus Summary fit
  // without needing a pinned identity column the way Repos' longer row does.
  const columns: DataTableColumn<ActivityRecord>[] = useMemo(
    () => [
      {
        id: "time",
        header: "Time",
        width: "96px",
        icon: Clock,
        cell: (row) => (
          <span className="font-mono text-xs whitespace-nowrap text-muted-foreground">
            {relativeTime(row.timestamp)}
          </span>
        ),
      },
      {
        id: "repo",
        header: "Repository",
        width: "140px",
        cell: (row) => {
          // A repo removed after its rows were written has no entry in
          // `repoNames` (see the comment above `repos`). Returning `null`
          // renders the primitive's own muted dash placeholder rather than a
          // silently blank cell, the same honesty convention the Branch and
          // Ahead/Behind columns already use in `repos.tsx`.
          const name = repoNames.get(row.repoId);
          return name === undefined ? null : <span className="truncate text-sm">{name}</span>;
        },
      },
      {
        id: "action",
        header: "Action",
        width: "96px",
        cell: (row) => (
          <span className="inline-flex w-fit rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] font-semibold">
            {row.actionType}
          </span>
        ),
      },
      {
        id: "outcome",
        header: "Outcome",
        width: "108px",
        cell: (row) => <OutcomeChip status={row.status} />,
      },
      {
        id: "summary",
        header: "Summary",
        width: "minmax(240px,520px)",
        cell: (row) =>
          row.summary === null ? null : (
            <span className="truncate text-sm text-foreground/90">{row.summary}</span>
          ),
      },
    ],
    [repoNames],
  );

  /*
   * The two chip groups are independent axes that AND together, and as one
   * undifferentiated row of six pills they read as a single either/or set -
   * "Failed" looks like a sibling of "Updates" rather than a second question
   * being asked about the same rows. The axis labels and the rule between the
   * groups are the whole fix; the filter state, the chips and the server-side
   * query are untouched.
   *
   * The labels use the mono/uppercase/tracked field-label register from
   * DESIGN.md rather than plain muted body text, and `text-foreground/70`
   * rather than `text-muted-foreground`, so an 11px label still clears AA.
   *
   * It rides in PageShell's sticky header so the filters do not scroll away
   * from the rows they filter.
   */
  const toolbar = (
        <div className="flex flex-wrap items-center gap-x-5 gap-y-2">
          <div
            role="group"
            aria-labelledby="activity-filter-action-label"
            className="flex flex-wrap items-center gap-1.5"
          >
            <span
              id="activity-filter-action-label"
              className="mr-1 font-mono text-[11px] font-semibold uppercase tracking-wider text-foreground/70"
            >
              Actions:
            </span>
            {ACTION_CHIPS.map((c) => (
              <FilterChip
                key={c.value}
                label={c.label}
                active={actionType === c.value}
                onClick={() => setActionType(c.value)}
              />
            ))}
          </div>
          <span aria-hidden="true" className="h-5 w-px shrink-0 bg-border" />
          <div
            role="group"
            aria-labelledby="activity-filter-outcome-label"
            className="flex flex-wrap items-center gap-1.5"
          >
            <span
              id="activity-filter-outcome-label"
              className="mr-1 font-mono text-[11px] font-semibold uppercase tracking-wider text-foreground/70"
            >
              Outcome:
            </span>
            {STATUS_CHIPS.map((c) => (
              <FilterChip
                key={c.value}
                label={c.label}
                tone={c.tone}
                active={status === c.value}
                onClick={() => setStatus(c.value)}
              />
            ))}
          </div>
        </div>
  );

  return (
    <PageShell
      title="Activity"
      // `fill`: N3 puts Activity on the same internally-scrolling `DataTable`
      // architecture as Repos (N2), rather than leaving the table to grow with
      // its content and letting the whole page scroll underneath a header that
      // can never catch up. See `page-shell.tsx`'s `fill` doc comment and
      // `data-table.tsx`'s scroll-ownership doc comment. Verified in a real
      // browser (not just jsdom) via `pnpm dev` + Playwright, per PR #73's own
      // precedent for exactly this class of change.
      fill
      toolbar={toolbar}
    >
      <div className="flex min-h-0 flex-1 flex-col">
        <AsyncPanel
          state={activity}
          emptyWhen={(rows) => rows.length === 0}
          emptyMessage={
            <EmptyState
              icon={History}
              title={filtered ? "Nothing matches this filter" : "No activity yet"}
              description={
                filtered
                  ? "No entries match the selected action and outcome. Clear the filters to see everything."
                  : "Checks and updates will show up here as soon as RepoSync runs one."
              }
              compact
            />
          }
        >
          {(rows) => (
            <DataTable
              aria-label="Activity log"
              density="compact"
              columns={columns}
              rows={paginate(rows).visible}
              rowKey={(r) => r.id}
              onRowClick={(r) => setSelectedId(r.id)}
              // A single unlabeled action (the receipt chevron), unlike Repos'
              // two, so `actionsWidth` is narrowed from the primitive's
              // two-button default (112px) to fit one 36px button plus the
              // existing 12px cell padding on both sides (12 + 36 + 12 = 60,
              // rounded up).
              actionsWidth="64px"
              actions={(r) => (
                <Button
                  variant="ghost"
                  size="icon"
                  aria-label="Open receipt"
                  onClick={() => setSelectedId(r.id)}
                >
                  <ChevronRight aria-hidden />
                </Button>
              )}
            />
          )}
        </AsyncPanel>
      </div>

      {/*
        Say so when the page was cut, and ONLY then. A list capped at 60 with
        nothing marking the cut looks like the whole history, which matters most
        in exactly the case the filters were added for: narrowing to "Failed",
        seeing a screen of rows, and concluding those are all of them.

        The condition is `hasMore` from the sentinel row, not a length comparison
        against the display limit. Those are not the same test: the request is
        capped, so a response can never exceed the limit, and "we got exactly 60"
        is equally consistent with "there are exactly 60" and "there are
        thousands". Keying on length would have this notice assert the existence
        of older entries in the one case it cannot distinguish, which is the same
        confident-but-unfounded claim it exists to prevent.
      */}
      {paginate(activity.data ?? []).hasMore && (
        <p className="text-xs text-muted-foreground">
          Showing the {ACTIVITY_PAGE_LIMIT} most recent matching entries. There are older ones;
          they are kept and are in the log, but are not listed here yet.
        </p>
      )}

      <Drawer
        open={selected !== null}
        onClose={() => setSelectedId(null)}
        aria-labelledby={ACTIVITY_RECEIPT_TITLE_ID}
      >
        {selected !== null && (
          <ActivityReceipt
            record={selected}
            repoName={repoNames.get(selected.repoId) ?? null}
            onClose={() => setSelectedId(null)}
          />
        )}
      </Drawer>
    </PageShell>
  );
}

/**
 * The activity row's own `status` string, which is not the repo status
 * taxonomy (`RepoStatus` / `StatusBadge`): `"success"` and `"failed"` are the
 * only two values the backend writes, and neither is a `RepoStatus` member the
 * badge could take.
 *
 * This is a deliberate duplicate of `StatusChip` in `activity-receipt.tsx`,
 * down to the class strings and the predicate, so the list row and the receipt
 * it opens cannot drift apart visually. Consolidating the two into one shared
 * component is the right follow-up; it is not done here because the receipt is
 * outside this change's file scope, and a second near-miss pattern would be a
 * worse outcome than one honest copy.
 *
 * The key is "is this bad", not `=== "success"`. `status` is typed as a bare
 * `string` on the wire, so an unmapped value must not be able to render as a
 * failure; falling through to the sync styling keeps an unknown value from
 * inventing an alarm.
 *
 * DESIGN.md requires color PLUS icon PLUS word, and the icon is the part that
 * is easy to get wrong: the pill's SHAPE is identical in both states, so it
 * carries no information and cannot be counted as the third channel. The
 * differing lucide glyph is what makes this legible in grayscale and to a
 * red-green colorblind reader, so it is not decoration.
 *
 * The tint is /10 rather than /15 because at /15 the light-mode success pill
 * computes to 4.40:1 against its own background, under the 4.5:1 AA floor that
 * DESIGN.md requires of small text. At /10 it is 4.60:1 and failed is 5.34:1.
 */
function OutcomeChip({ status }: { status: string }) {
  const bad = status === "failed" || status === "error";
  const Icon = bad ? XCircle : CheckCircle2;
  return (
    <span
      className={cn(
        "inline-flex w-fit shrink-0 items-center gap-1 rounded-md px-2 py-0.5 font-mono text-[11px] font-semibold",
        bad ? "bg-status-failed/10 text-status-failed" : "bg-status-sync/10 text-status-sync",
      )}
    >
      <Icon aria-hidden className="size-3" />
      {status}
    </span>
  );
}
