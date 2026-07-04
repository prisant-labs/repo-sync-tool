import { useCallback, useState } from "react";
import { AlertTriangle, FolderGit2, Plus, RefreshCw } from "lucide-react";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState, AllClearState } from "@/components/empty-state";
import { Drawer } from "@/components/ui/drawer";
import { RepoDetailPanel } from "@/components/repo-detail";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { useBackendEvents, useRepoList, useSummaryToday } from "@/hooks/queries";

const ALL_FILTER = { enabledOnly: null, hostType: null, query: null };

export function DashboardScreen({ onOpenRepos }: { onOpenRepos: () => void }) {
  const repos = useRepoList(ALL_FILTER);
  const summary = useSummaryToday();

  const reposRefetch = repos.refetch;
  const summaryRefetch = summary.refetch;
  const refetch = useCallback(() => {
    reposRefetch();
    summaryRefetch();
  }, [reposRefetch, summaryRefetch]);
  useBackendEvents(refetch);

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);

  const underWatch = repos.data?.length ?? null;
  const noRepos = repos.data !== null && repos.data.length === 0;

  return (
    <div className="mx-auto flex max-w-6xl flex-col gap-5">
      <div className="flex flex-wrap items-end gap-4">
        <div>
          <h2 className="text-2xl font-bold tracking-tight">Dashboard</h2>
          <p className="text-sm text-muted-foreground">A live roll-up of your watched repositories.</p>
        </div>
        <div className="ml-auto flex gap-2">
          <Button variant="outline" size="sm" onClick={refetch}>
            <RefreshCw /> Refresh
          </Button>
          <Button size="sm" onClick={() => setAddOpen(true)}>
            <Plus /> Add repos
          </Button>
        </div>
      </div>

      {noRepos ? (
        <EmptyState
          icon={FolderGit2}
          title="No repositories yet"
          description="Add a folder of repos or a single path to start tracking sync status here."
          action={
            <Button onClick={() => setAddOpen(true)}>
              <Plus /> Add repositories
            </Button>
          }
        />
      ) : (
        <AsyncPanel state={summary}>
          {(s) => (
            <div className="flex flex-col gap-5">
              <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                <Stat label="Under watch" value={underWatch ?? "-"} hint={`${s.noChangeCount} in sync`} />
                <Stat
                  label="Need attention"
                  value={s.attentionCount}
                  hint="dirty, failed, behind"
                  alert={s.attentionCount > 0}
                />
                <Stat label="Updated today" value={s.updatedCount} hint="fast-forwarded, clean" />
                <Stat label="New releases" value={s.releasesCount} hint="upstream tags" />
              </div>

              <Card>
                <CardHeader>
                  <CardTitle>Needs attention</CardTitle>
                  <button onClick={onOpenRepos} className="ml-auto text-xs font-medium text-primary">
                    Open Repos
                  </button>
                </CardHeader>
                {s.attention.length === 0 ? (
                  <CardContent>
                    <AllClearState
                      title="All clear"
                      description="Every watched repo is in sync or intentionally paused."
                    />
                  </CardContent>
                ) : (
                  <ul>
                    {s.attention.map((item) => (
                      <li key={item.repoId}>
                        <button
                          type="button"
                          onClick={() => setSelectedId(item.repoId)}
                          className="flex w-full items-center gap-3 border-b border-border px-4 py-3 text-left last:border-b-0 hover:bg-muted/40 focus-visible:bg-muted/40 focus-visible:outline-none"
                        >
                          <AlertTriangle className="size-4 shrink-0 text-status-failed" />
                          <div className="min-w-0">
                            <div className="truncate font-mono text-sm font-semibold">{item.localName}</div>
                            {item.detail && (
                              <div className="truncate text-xs text-muted-foreground">{item.detail}</div>
                            )}
                          </div>
                        </button>
                      </li>
                    ))}
                  </ul>
                )}
              </Card>
            </div>
          )}
        </AsyncPanel>
      )}

      <Drawer open={selectedId !== null} onClose={() => setSelectedId(null)}>
        {selectedId !== null && (
          <RepoDetailPanel id={selectedId} onChanged={refetch} onClose={() => setSelectedId(null)} />
        )}
      </Drawer>

      <AddReposDialog open={addOpen} onClose={() => setAddOpen(false)} onAdded={refetch} />
    </div>
  );
}

function Stat({
  label,
  value,
  hint,
  alert,
}: {
  label: string;
  value: number | string;
  hint: string;
  alert?: boolean;
}) {
  return (
    <Card>
      <CardContent>
        <div className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
          {label}
        </div>
        <div className={cn("mt-1.5 font-mono text-3xl font-bold tracking-tight", alert && "text-status-failed")}>
          {value}
        </div>
        <div className="mt-1.5 text-xs text-muted-foreground">{hint}</div>
      </CardContent>
    </Card>
  );
}
