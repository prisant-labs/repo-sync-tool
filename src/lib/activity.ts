import type { ActivityFilter, ActivityRecord } from "@/lib/bindings";

/**
 * Formatting for the activity receipt drawer.
 *
 * Split out of the component so both functions can be tested directly, and so
 * the `null` versus `""` distinction in the captured streams is pinned by a
 * test rather than left to a reader of JSX to notice.
 */

/** Local absolute time, to the minute. The relative form sits beside it in the UI. */
export function absoluteTime(epochSeconds: number): string {
  return new Date(epochSeconds * 1000).toLocaleString(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  });
}

/**
 * The plain-text receipt, for pasting into an issue.
 *
 * A `null` stream and an empty one are rendered DIFFERENTLY, matching the
 * drawer: `null` means RepoSync never captured that stream for this action
 * (a policy decision that skipped without running git, say), while `""` means
 * git ran and printed nothing. Collapsing them would erase the distinction
 * someone reads a receipt to make.
 */
export function formatReceipt(record: ActivityRecord, repoName: string | null): string {
  const streams = [
    ["command", record.rawCommand],
    ["stdout", record.rawStdout],
    ["stderr", record.rawStderr],
  ] as const;

  return [
    `${record.actionType} ${record.status} - ${repoName ?? "unknown repo"}`,
    absoluteTime(record.timestamp),
    record.summary ? `summary: ${record.summary}` : null,
    record.reasonCode ? `reason: ${record.reasonCode}` : null,
    record.commitRange ? `commits: ${record.commitRange}` : null,
    record.exitCode === null ? null : `exit: ${record.exitCode}`,
    record.durationMs === null ? null : `duration: ${record.durationMs} ms`,
    ...streams.map(([name, value]) =>
      value === null ? `${name}: (not captured)` : `${name}:\n${value}`,
    ),
  ]
    .filter((line): line is string => line !== null)
    .join("\n");
}

/**
 * The rows the Activity screen DISPLAYS per page.
 *
 * Deliberately named rather than inlined, because it is the reason the filter
 * chips carry no counts and the reason filtering has to happen in SQL. The
 * backend applies its LIMIT *after* the WHERE, so what comes back is one capped
 * page of an already-filtered query. Narrowing the same rows again in the
 * browser would search only whatever the last unfiltered page happened to hold,
 * which for an audit trail is a lie by omission: "no failed updates for this
 * repo" would really mean "none in the most recent 60 rows".
 *
 * The core's own `ACTIVITY_DEFAULT_LIMIT` is 200 and `ACTIVITY_MAX_LIMIT` is
 * 1000, so this is well under what the backend considers a normal page. Whether
 * it should grow, or the screen should page, is a product decision and not this
 * change's to make.
 */
export const ACTIVITY_PAGE_LIMIT = 60;

/**
 * What the screen actually ASKS the backend for: one more row than it shows.
 *
 * That extra row is a sentinel, and it exists because a page of exactly N rows
 * cannot tell you whether there were more. Requesting N and getting N back is
 * consistent with "there are exactly N matches" and with "there are thousands",
 * so a truncation notice keyed on it would assert the existence of older entries
 * it has no evidence for - which is precisely the kind of confident-and-unfounded
 * claim the notice was added to prevent. Ask for N+1: if it arrives, more exist,
 * and if it does not, the page is the whole result.
 */
export const ACTIVITY_FETCH_LIMIT = ACTIVITY_PAGE_LIMIT + 1;

/** The action-type chip selection. `all` means "do not constrain". */
export type ActionTypeFilter = "all" | "check" | "update";

/** The status chip selection. `all` means "do not constrain". */
export type StatusFilter = "all" | "success" | "failed";

/**
 * Build the wire filter from the two chip selections.
 *
 * Split out of the component for the same reason the formatters above were: it
 * is a pure mapping with one rule worth pinning by test rather than by reading
 * JSX. The rule is that `"all"` maps to `null`, NOT to a wildcard string. The
 * backend treats a `null` field as "no constraint" and applies a literal `=`
 * comparison otherwise, so sending `"all"` would filter for rows whose
 * `action_type` is the string `"all"` and return nothing at all - an empty
 * screen that looks exactly like "you have no activity".
 *
 * `repoId` stays `null` here: repo-scoped filtering needs a control this app
 * does not have yet, and picking one is a UI decision that has not been made.
 */
export function toActivityFilter(
  actionType: ActionTypeFilter,
  status: StatusFilter,
): ActivityFilter {
  return {
    repoId: null,
    actionType: actionType === "all" ? null : actionType,
    status: status === "all" ? null : status,
    limit: ACTIVITY_FETCH_LIMIT,
  };
}

/**
 * Split a fetched page into what to render and whether anything was cut off.
 *
 * Pure, so the boundary cases that a rendered list makes awkward to check
 * (exactly at the limit, one over, one under) are pinned by test instead. The
 * sentinel row is dropped rather than shown: it was requested to answer a
 * question, not to be read.
 */
export function paginate<T>(rows: T[]): { visible: T[]; hasMore: boolean } {
  return {
    visible: rows.slice(0, ACTIVITY_PAGE_LIMIT),
    hasMore: rows.length > ACTIVITY_PAGE_LIMIT,
  };
}
