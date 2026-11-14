import { useContext } from "react";
import { Boxes, FolderGit2, LayoutDashboard, Plus, Settings } from "lucide-react";
import { cn } from "@/lib/utils";
import { LOOKS, ThemeContext } from "@/gallery/looks";
import { REPOS, GROUPS } from "@/gallery/fixtures";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { DataTable, type DataTableColumn } from "@/components/ui/data-table";
import { StatusBadge } from "@/components/status-badge";
import { GroupsNav } from "@/components/groups-nav";
import { STATUS_ORDER, deriveStatus } from "@/lib/status";
import type { RepoSummary } from "@/lib/bindings";

const noop = () => {};

const COLUMNS: DataTableColumn<RepoSummary>[] = [
  { id: "repo", header: "Repo", width: "minmax(90px, 1fr)", frozen: true, cell: (r) => r.localName },
  { id: "status", header: "Status", width: "112px", cell: (r) => <StatusBadge status={deriveStatus(r)} /> },
];

/**
 * A nav row copied from `app-shell.tsx`'s `NavButton`, class for class. That
 * component is not exported and is welded to the whole shell's state, so the
 * gallery cannot mount it alone; the classes are the part a look can change, and
 * the contrast gate's rail pairs are written against these same ones.
 */
function NavRow({ label, Icon, active }: { label: string; Icon: typeof Boxes; active?: boolean }) {
  return (
    <div
      className={cn(
        "flex items-center gap-3 rounded-md border-l-2 border-transparent px-2.5 py-2 text-sm",
        active
          ? "border-l-primary-ink bg-sidebar-accent font-semibold text-foreground"
          : "font-medium text-muted-foreground",
      )}
    >
      <Icon className="size-[17px]" />
      <span className="flex-1 text-left">{label}</span>
    </div>
  );
}

/** One real-component sample, drawn identically in every column. */
function Sample() {
  const rows = REPOS.slice(0, 2);
  return (
    <div className="flex flex-col gap-4">
      <div className="overflow-hidden rounded-lg border border-border bg-sidebar">
        <div className="flex items-center gap-2.5 px-3 py-3">
          <div className="grid size-7 place-items-center rounded-md bg-primary text-sm font-bold text-primary-foreground">R</div>
          <span className="font-semibold">
            Repo<span className="text-primary-ink">Sync</span>
          </span>
        </div>
        <div className="flex flex-col gap-0.5 px-2 pb-2">
          <NavRow label="Dashboard" Icon={LayoutDashboard} active />
          <NavRow label="Repos" Icon={FolderGit2} />
          <NavRow label="Settings" Icon={Settings} />
        </div>
        <div className="border-t border-sidebar-border px-2 py-2">
          <GroupsNav groups={GROUPS.slice(0, 2)} activeGroupId={GROUPS[0]?.id ?? null} onSelectGroup={noop} onClearActiveGroup={noop} refetchGroups={noop} />
        </div>
      </div>

      <Card>
        <CardHeader>
          <CardTitle>Repositories</CardTitle>
          <Button size="sm" variant="secondary" className="ml-auto"><Plus /> Add</Button>
        </CardHeader>
        <DataTable aria-label="Repositories" columns={COLUMNS} rows={rows} rowKey={(r) => r.id} density="compact" />
        <CardContent className="flex flex-wrap gap-1.5">
          {STATUS_ORDER.map((s) => <StatusBadge key={s} status={s} />)}
        </CardContent>
      </Card>

      <div className="flex flex-wrap gap-2">
        <Button>Primary</Button>
        <Button variant="secondary">Secondary</Button>
        <Button variant="outline">Outline</Button>
        <Button variant="ghost">Ghost</Button>
      </div>
      <Input aria-label="Sample input" defaultValue="origin/main" />

      {/* The type ramp. No real component reads `cell`, `meta`, `label` or `micro`
          yet (only `page` is adopted), so without this block the looks' type
          changes would be invisible. These are the named classes themselves. */}
      <div className="flex flex-col gap-1 rounded-lg border border-border bg-card p-3">
        <div className="text-page">Page title</div>
        <div className="text-identity">acme/billing-service</div>
        <div className="text-body">Body: fetched 3 minutes ago, 14 behind.</div>
        <div className="text-cell text-muted-foreground">Cell: two lines of table text<br />wrapped to show the leading.</div>
        <div className="text-meta text-muted-foreground">Meta: two lines of 11px detail<br />wrapped to show the leading.</div>
        <div className="text-label font-mono tracking-wider text-muted-foreground uppercase">Label</div>
        <div className="text-micro text-muted-foreground">Micro (10px today, 11px in both looks)</div>
      </div>
    </div>
  );
}

/**
 * The same small set of real components, once per look, side by side. Each
 * column is its OWN theme root: it carries `data-look` and, when the control
 * bar says dark, the `.dark` class on the same element, which is the pairing the
 * override blocks in `src/index.css` require. The gallery shell sets no look on
 * this section (see `GalleryApp`), so no column inherits another's palette.
 */
export function CompareLooksSection() {
  const dark = useContext(ThemeContext);
  return (
    <div className="grid grid-cols-3 items-start gap-4" data-testid="compare-looks">
      {LOOKS.map((look) => (
        <div
          key={look.id}
          data-look={look.attribute}
          data-compare={look.id}
          className={cn(
            dark && "dark",
            "min-w-0 rounded-xl border border-border bg-background p-4 text-foreground",
          )}
        >
          <div className="mb-3 font-mono text-xs font-bold tracking-wider uppercase">{look.label}</div>
          <Sample />
        </div>
      ))}
    </div>
  );
}
