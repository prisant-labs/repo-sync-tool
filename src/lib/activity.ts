import type { ActivityRecord } from "@/lib/bindings";

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
