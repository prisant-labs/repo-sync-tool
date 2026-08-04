import { useMemo, useState } from "react";
import { History } from "lucide-react";
import { Card } from "@/components/ui/card";
import { Drawer } from "@/components/ui/drawer";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState } from "@/components/empty-state";
import { ActivityReceipt } from "@/components/activity-receipt";
import { useActivity, useRepoList } from "@/hooks/queries";
import { relativeTime } from "@/lib/status";
import { cn } from "@/lib/utils";

const FILTER = { repoId: null, actionType: null, status: null, limit: 60 };
const ALL_REPOS = { enabledOnly: null, hostType: null, query: null };

export function ActivityScreen() {
  const activity = useActivity(FILTER);
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

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-5">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Activity</h2>
        <p className="text-sm text-muted-foreground">
          The audit trail of every check and update, newest first. Select a row for the full
          receipt.
        </p>
      </div>

      <AsyncPanel
        state={activity}
        emptyWhen={(rows) => rows.length === 0}
        emptyMessage={
          <EmptyState
            icon={History}
            title="No activity yet"
            description="Checks and updates will show up here as soon as RepoSync runs one."
            compact
          />
        }
      >
        {(rows) => (
          <Card className="divide-y divide-border">
            {rows.map((row) => (
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
                <span className="truncate text-sm text-foreground/90">
                  {row.summary ?? row.status}
                </span>
                <span className="truncate text-xs text-muted-foreground">
                  {repoNames.get(row.repoId) ?? ""}
                </span>
              </button>
            ))}
          </Card>
        )}
      </AsyncPanel>

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
