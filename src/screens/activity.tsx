import { useMemo, useState } from "react";
import { CheckCircle2, ChevronRight, History, XCircle } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Drawer } from "@/components/ui/drawer";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { FilterChip } from "@/components/filter-chip";
import { ActivityReceipt } from "@/components/activity-receipt";
import { useActivity, useRepoList } from "@/hooks/queries";
import { ACTIVITY_PAGE_LIMIT, paginate, toActivityFilter } from "@/lib/activity";
import type { ActionTypeFilter, StatusFilter } from "@/lib/activity";
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

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-5">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Activity</h2>
      </div>

      {/*
        The two chip groups are independent axes that AND together, and as one
        undifferentiated row of six pills they read as a single either/or set -
        "Failed" looks like a sibling of "Updates" rather than a second
        question being asked about the same rows. The axis labels and the rule
        between the groups are the whole fix; the filter state, the chips, and
        the server-side query are untouched.

        The labels use the mono/uppercase/tracked field-label register from
        DESIGN.md rather than plain muted body text, and `text-foreground/70`
        rather than `text-muted-foreground`, so an 11px label still clears AA.
      */}
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
          <Card className="divide-y divide-border">
            {paginate(rows).visible.map((row) => (
              <button
                key={row.id}
                type="button"
                onClick={() => setSelectedId(row.id)}
                aria-haspopup="dialog"
                className={cn(
                  "grid w-full grid-cols-[128px_96px_1fr_auto] items-center gap-3 px-4 py-2.5 text-left transition-colors first:rounded-t-xl last:rounded-b-xl hover:bg-muted/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-ring",
                  selectedId === row.id && "bg-muted",
                )}
              >
                <span className="font-mono text-[11px] text-muted-foreground">
                  {relativeTime(row.timestamp)}
                </span>
                <span className="inline-flex w-fit rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] font-semibold">
                  {row.actionType}
                </span>
                {/*
                  The chip lives inside the existing summary cell rather than
                  as a fifth grid column: a real outcome column is BL-NI-89 and
                  is blocked on the row-standard decision, so the grid template
                  here is deliberately unchanged.

                  The old `?? row.status` fallback is gone because the chip now
                  always renders that same word; keeping both would print
                  "failed  failed" on every row that has no summary.
                */}
                <span className="flex min-w-0 items-center gap-2">
                  <OutcomeChip status={row.status} />
                  <span className="truncate text-sm text-foreground/90">{row.summary}</span>
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  {repoNames.get(row.repoId) ?? ""}
                </span>
                {/*
                  The row opens the receipt drawer, and until this chevron it
                  said so nowhere: the page byline that used to read "Select a
                  row for the full receipt" was removed with the other three,
                  and a hover background is not an affordance a keyboard or
                  first-time user can see. DESIGN.md asks that destination and
                  open actions stay discoverable, so the cue is a permanent part
                  of the row rather than something hover reveals.

                  `aria-hidden` because the row's own `aria-haspopup="dialog"`
                  already tells a screen reader what activating it does; the
                  glyph would only repeat it as noise.
                */}
                <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground/70" />
              </button>
            ))}
          </Card>
        )}
      </AsyncPanel>

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

      <Drawer open={selected !== null} onClose={() => setSelectedId(null)}>
        {selected !== null && (
          <ActivityReceipt
            record={selected}
            repoName={repoNames.get(selected.repoId) ?? null}
            onClose={() => setSelectedId(null)}
          />
        )}
      </Drawer>
    </div>
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
