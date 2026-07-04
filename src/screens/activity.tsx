import { Card } from "@/components/ui/card";
import { AsyncPanel } from "@/components/async-panel";
import { useActivity } from "@/hooks/queries";
import { relativeTime } from "@/lib/status";

const FILTER = { repoId: null, actionType: null, status: null, limit: 60 };

export function ActivityScreen() {
  const activity = useActivity(FILTER);

  return (
    <div className="mx-auto flex max-w-4xl flex-col gap-5">
      <div>
        <h2 className="text-2xl font-bold tracking-tight">Activity</h2>
        <p className="text-sm text-muted-foreground">
          The audit trail of every check and update, newest first.
        </p>
      </div>

      <AsyncPanel
        state={activity}
        emptyWhen={(rows) => rows.length === 0}
        emptyMessage="No activity has been recorded yet."
      >
        {(rows) => (
          <Card className="divide-y divide-border">
            {rows.map((row) => (
              <div key={row.id} className="grid grid-cols-[128px_96px_1fr] items-center gap-3 px-4 py-2.5">
                <span className="font-mono text-[11px] text-muted-foreground">
                  {relativeTime(row.timestamp)}
                </span>
                <span className="inline-flex w-fit rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] font-semibold">
                  {row.actionType}
                </span>
                <span className="truncate text-sm text-foreground/90">{row.summary ?? row.status}</span>
              </div>
            ))}
          </Card>
        )}
      </AsyncPanel>
    </div>
  );
}
