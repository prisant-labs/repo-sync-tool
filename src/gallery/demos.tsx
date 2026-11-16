import { useState } from "react";
import { Inbox, Plus, RefreshCw, Search } from "lucide-react";
import { Cell, Row } from "@/gallery/cells";
import { REPOS, GROUPS, SCAN_CANDIDATES } from "@/gallery/fixtures";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Switch } from "@/components/ui/switch";
import { Tabs, TabList, TabPanel } from "@/components/ui/tabs";
import { DataTable, type DataTableColumn, type DataTableSort } from "@/components/ui/data-table";
import { ColumnsPopover } from "@/components/ui/columns-popover";
import { Dialog } from "@/components/ui/dialog";
import { Drawer } from "@/components/ui/drawer";
import { FieldLabelContext } from "@/components/ui/field-label";
import { useToast } from "@/hooks/use-toast";
import { StatusBadge } from "@/components/status-badge";
import { FilterChip } from "@/components/filter-chip";
import { LagSignal } from "@/components/lag-signal";
import { IntelSignals } from "@/components/intel-signals";
import { EmptyState, AllClearState } from "@/components/empty-state";
import { AsyncPanel } from "@/components/async-panel";
import { PageShell } from "@/components/page-shell";
import { GroupsNav } from "@/components/groups-nav";
import { GroupDialog } from "@/components/group-dialog";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { ActivityReceipt } from "@/components/activity-receipt";
import { DiagnosticsCard } from "@/components/diagnostics-card";
import { RepoDetailPanel, REPO_DETAIL_TITLE_ID } from "@/components/repo-detail";
import { AppShell } from "@/components/app-shell";
import { DashboardScreen } from "@/screens/dashboard";
import { ReposScreen } from "@/screens/repos";
import { ActivityScreen } from "@/screens/activity";
import { SettingsScreen } from "@/screens/settings";
import { IpcError } from "@/lib/ipc";
import { STATUS_ORDER, deriveStatus, lagLabel, STATUS_STYLE } from "@/lib/status";
import type { RepoStatus } from "@/lib/status";
import type { AsyncState } from "@/hooks/use-async";
import type { RepoSummary } from "@/lib/bindings";

const noop = () => {};

// ---------------------------------------------------------------- Primitives

export function ButtonsSection() {
  const variants = ["default", "secondary", "outline", "ghost"] as const;
  return (
    <>
      <Cell title="Variants and sizes">
        <div className="flex flex-col gap-3">
          {variants.map((v) => (
            <Row key={v}>
              <span className="w-20 font-mono text-cell text-muted-foreground">{v}</span>
              <Button variant={v}>Label</Button>
              <Button variant={v} size="sm">Small</Button>
              <Button variant={v} size="icon" aria-label="Refresh"><RefreshCw /></Button>
              <Button variant={v}><Plus /> With icon</Button>
              <Button variant={v} disabled>Disabled</Button>
            </Row>
          ))}
        </div>
      </Cell>
    </>
  );
}

export function CardsSection() {
  return (
    <Cell title="Card, CardHeader, CardTitle, CardContent">
      <Card className="max-w-sm">
        <CardHeader>
          <CardTitle>Card title</CardTitle>
        </CardHeader>
        <CardContent>Body copy inside a card.</CardContent>
      </Card>
    </Cell>
  );
}

export function FormSection() {
  const [on, setOn] = useState(true);
  const [off, setOff] = useState(false);
  return (
    <>
      <Cell title="Input" note="default, placeholder, disabled">
        <div className="flex max-w-sm flex-col gap-3">
          <Input aria-label="Default input" defaultValue="Some value" />
          <Input aria-label="Placeholder input" placeholder="Placeholder text" />
          <Input aria-label="Disabled input" disabled defaultValue="Disabled" />
        </div>
      </Cell>
      <Cell title="Switch" note="on, off, disabled">
        <Row>
          <Switch aria-label="On switch" checked={on} onCheckedChange={setOn} />
          <Switch aria-label="Off switch" checked={off} onCheckedChange={setOff} />
          <Switch aria-label="Disabled switch" checked={false} onCheckedChange={noop} disabled />
        </Row>
      </Cell>
      <Cell
        title="FieldLabelContext"
        note="A control inside a provider is named by the label; Settings builds its Field this way."
      >
        <FieldLabelContext.Provider value="gallery-field-label">
          <div className="flex max-w-sm flex-col gap-1.5">
            <div id="gallery-field-label" className="text-body font-medium">Editor command</div>
            <Input defaultValue="code" />
          </div>
        </FieldLabelContext.Provider>
      </Cell>
    </>
  );
}

export function TabsSection() {
  const [value, setValue] = useState("one");
  return (
    <Cell title="Tabs, TabList, TabPanel" note="with and without a count badge">
      <Tabs value={value} onValueChange={setValue}>
        <TabList
          aria-label="Gallery tabs"
          tabs={[
            { value: "one", label: "Overview" },
            { value: "two", label: "Activity", badge: { count: 3, display: "3", description: "3 entries" } },
            { value: "three", label: "Settings" },
          ]}
        />
        <TabPanel value="one" className="pt-3 text-body">Overview panel</TabPanel>
        <TabPanel value="two" className="pt-3 text-body">Activity panel</TabPanel>
        <TabPanel value="three" className="pt-3 text-body">Settings panel</TabPanel>
      </Tabs>
    </Cell>
  );
}

function tableColumns(): DataTableColumn<RepoSummary>[] {
  return [
    { id: "repo", header: "Repo", width: "minmax(180px, 2fr)", frozen: true, cell: (r) => r.localName, sortValue: (r) => r.localName },
    { id: "status", header: "Status", width: "130px", cell: (r) => <StatusBadge status={deriveStatus(r)} />, sortValue: (r) => deriveStatus(r) },
    { id: "branch", header: "Branch", width: "110px", cell: (r) => r.activeBranch ?? "none" },
    { id: "stars", header: "Stars", width: "80px", align: "right", cell: (r) => r.stars ?? "-", sortValue: (r) => r.stars },
  ];
}

export function DataTableSection() {
  const [sort, setSort] = useState<DataTableSort | null>(null);
  const [hidden, setHidden] = useState<Record<string, boolean>>({});
  const [order, setOrder] = useState(["repo", "status", "branch", "stars"]);
  const columns = tableColumns();
  const rows = REPOS.slice(0, 5);
  return (
    <>
      <Cell title="DataTable, full density, sortable" note="click a header to sort">
        <DataTable
          aria-label="Repositories"
          columns={columns}
          rows={rows}
          rowKey={(r) => r.id}
          sort={sort}
          onSortChange={setSort}
          columnOrder={order}
          columnVisibility={hidden}
          actions={() => <Button variant="ghost" size="sm">Open</Button>}
        />
      </Cell>
      <Cell title="DataTable, compact density">
        <DataTable aria-label="Repositories, compact" density="compact" columns={columns} rows={rows} rowKey={(r) => r.id} currentKey={2} onRowClick={noop} />
      </Cell>
      <Cell title="ColumnsPopover" note="drives the sortable table above">
        <div className="flex min-h-72 justify-end">
          <ColumnsPopover
            items={columns.map((c) => ({ id: c.id, header: c.header, locked: c.frozen }))}
            visibility={{ repo: true, status: true, branch: true, stars: true, ...hidden }}
            onToggle={(id) => setHidden((h) => ({ ...h, [id]: h[id] === false }))}
            onReorder={setOrder}
          />
        </div>
      </Cell>
    </>
  );
}

export function DrawerCell() {
  const [open, setOpen] = useState(true);
  return (
    <Cell title="Drawer, default and wide" note="contained in this cell" height={420} contained>
      <Button onClick={() => setOpen(true)}>Open drawer</Button>
      <Drawer open={open} onClose={() => setOpen(false)} aria-label="Example drawer">
        <div className="p-5 text-body">Drawer body. Press Escape or click the scrim to close.</div>
      </Drawer>
    </Cell>
  );
}

export function DialogCell() {
  const [open, setOpen] = useState(true);
  return (
    <Cell title="Dialog" note="contained in this cell" height={320} contained>
      <Button onClick={() => setOpen(true)}>Open dialog</Button>
      <Dialog open={open} onClose={() => setOpen(false)} aria-label="Example dialog">
        <Card className="w-80 p-5 text-body">
          A dialog body.
          <div className="mt-4"><Button onClick={() => setOpen(false)}>Close</Button></div>
        </Card>
      </Dialog>
    </Cell>
  );
}

export function ToastTriggers() {
  const toast = useToast();
  return (
    <Row>
      <Button variant="outline" onClick={() => toast("ok", "Saved", "Your change was saved.")}>Ok toast</Button>
      <Button variant="outline" onClick={() => toast("info", "Heads up", "Something to know.")}>Info toast</Button>
      <Button variant="outline" onClick={() => toast("error", "Failed", "Could not reach the remote.")}>Error toast</Button>
    </Row>
  );
}

export function ToastCell() {
  return (
    <Cell title="Toast" note="the stack is fixed bottom-right of this cell; toasts clear after about four seconds" height={320} contained>
      <ToastTriggers />
    </Cell>
  );
}

// ---------------------------------------------------------- Product components

export function StatusSection() {
  return (
    <>
      <Cell title="StatusBadge, every status" note="STATUS_ORDER from lib/status.ts">
        <Row>
          {STATUS_ORDER.map((s) => <StatusBadge key={s} status={s} />)}
        </Row>
      </Cell>
      <Cell title="StatusBadge with a count" note="only ahead and behind fold the count into the label">
        <Row>
          <StatusBadge status="behind" count={14} />
          <StatusBadge status="ahead" count={3} />
          <StatusBadge status="sync" count={5} />
        </Row>
      </Cell>
      <Cell title="LagSignal" note="fill length encodes magnitude">
        <div className="grid max-w-md gap-4">
          {(["behind", "ahead", "dirty", "failed", "paused", "noUpstream", "sync"] as RepoStatus[]).map((s, i) => (
            <LagSignal key={s} status={s} magnitude={(i + 1) / 8} label={STATUS_STYLE[s].label} />
          ))}
        </div>
      </Cell>
      <Cell title="IntelSignals" note="release tag and open-PR count; renders nothing when both are absent">
        <div className="flex flex-col gap-3">
          <IntelSignals latestReleaseTag="v1.4.0" openPrCount={2} />
          <IntelSignals latestReleaseTag="v1.4.0" openPrCount={null} />
          <IntelSignals latestReleaseTag={null} openPrCount={5} />
          <IntelSignals latestReleaseTag={null} openPrCount={null} />
        </div>
      </Cell>
    </>
  );
}

export function FilterChipSection() {
  const [active, setActive] = useState("all");
  return (
    <Cell title="FilterChip" note="active, inactive, with count, with status tone">
      <Row>
        <FilterChip label="All" count={8} active={active === "all"} onClick={() => setActive("all")} />
        <FilterChip label="Behind" count={1} tone="text-status-behind" active={active === "behind"} onClick={() => setActive("behind")} />
        <FilterChip label="Failed" count={0} tone="text-status-failed" active={active === "failed"} onClick={() => setActive("failed")} />
        <FilterChip label="No count" active={active === "plain"} onClick={() => setActive("plain")} />
      </Row>
    </Cell>
  );
}

export function EmptyStatesSection() {
  return (
    <>
      <Cell title="EmptyState, regular and compact">
        <div className="flex flex-col gap-4">
          <EmptyState icon={Inbox} title="No repositories yet" description="Add a folder of clones and RepoSync will keep them fresh." action={<Button><Plus /> Add repositories</Button>} />
          <EmptyState compact icon={Search} title="Nothing matches" description="Try a different search or clear the filters." />
        </div>
      </Cell>
      <Cell title="AllClearState">
        <AllClearState title="All clear" description="Nothing needs your attention today." />
      </Cell>
    </>
  );
}

export function AsyncPanelSection() {
  const loading: AsyncState<string[]> = { data: null, error: null, loading: true };
  const failed: AsyncState<string[]> = {
    data: null,
    loading: false,
    error: new IpcError({ code: "db_unavailable", message: "Could not read the library", remediation: "Restart RepoSync.", context: null }),
  };
  const plainError: AsyncState<string[]> = { data: null, loading: false, error: new Error("Network down") };
  const emptyData: AsyncState<string[]> = { data: [], error: null, loading: false };
  const withData: AsyncState<string[]> = { data: ["alpha", "beta"], error: null, loading: false };
  const render = (d: string[]) => <ul className="text-body">{d.map((x) => <li key={x}>{x}</li>)}</ul>;
  return (
    <>
      <Cell title="Loading"><AsyncPanel state={loading}>{render}</AsyncPanel></Cell>
      <Cell title="Error from the backend (carries remediation)"><AsyncPanel state={failed}>{render}</AsyncPanel></Cell>
      <Cell title="Error without remediation"><AsyncPanel state={plainError}>{render}</AsyncPanel></Cell>
      <Cell title="Empty"><AsyncPanel state={emptyData} emptyWhen={(d) => d.length === 0} emptyMessage="Nothing here yet.">{render}</AsyncPanel></Cell>
      <Cell title="Data"><AsyncPanel state={withData}>{render}</AsyncPanel></Cell>
    </>
  );
}

export function PageShellSection() {
  return (
    <>
      <Cell title="PageShell, wide, with actions and toolbar" height={300} contained>
        <PageShell title="Page title" actions={<Button variant="outline" size="sm"><RefreshCw /> Refresh</Button>} toolbar={<Input aria-label="Search" placeholder="Search" className="max-w-xs" />}>
          <Card className="p-4 text-body">Page content</Card>
        </PageShell>
      </Cell>
      <Cell title="PageShell, narrow" height={240} contained>
        <PageShell title="Narrow page" width="narrow">
          <Card className="p-4 text-body">Capped measure, left aligned.</Card>
        </PageShell>
      </Cell>
    </>
  );
}

export function GroupsNavSection() {
  const [active, setActive] = useState<number | null>(1);
  return (
    <Cell title="GroupsNav" note="the sidebar's group list">
      <div className="w-60 rounded-md bg-sidebar p-2">
        <GroupsNav groups={GROUPS} activeGroupId={active} onSelectGroup={setActive} onClearActiveGroup={() => setActive(null)} refetchGroups={noop} />
      </div>
    </Cell>
  );
}

export function GroupDialogCells() {
  return (
    <>
      <Cell title="GroupDialog, create" height={420} contained>
        <GroupDialog open mode="create" onClose={noop} onSaved={noop} />
      </Cell>
      <Cell title="GroupDialog, rename" height={420} contained>
        <GroupDialog open mode="rename" group={GROUPS[0]} onClose={noop} onSaved={noop} />
      </Cell>
    </>
  );
}

export function AddReposCell() {
  return (
    <Cell title="AddReposDialog" note="Scan answers with fixture candidates: one already tracked" height={560} contained>
      <AddReposDialog open onClose={noop} onAdded={noop} />
      <p className="text-cell text-muted-foreground">{SCAN_CANDIDATES.length} candidates come back from a scan.</p>
    </Cell>
  );
}

export function ReceiptCell() {
  return (
    <Cell title="ActivityReceipt in a Drawer" height={520} contained>
      <Drawer open onClose={noop} aria-label="Activity receipt">
        <ActivityReceipt
          repoName="sample-service"
          onClose={noop}
          record={{
            id: 2, repoId: 3, timestamp: RECEIPT_TIMESTAMP, actionType: "update", status: "success",
            reasonCode: null, summary: "Fast-forwarded 14 commits", commitRange: "a1b2c3d..e4f5a6b",
            rawCommand: "git pull --ff-only", rawStdout: "Updating a1b2c3d..e4f5a6b\nFast-forward", rawStderr: null, exitCode: 0, durationMs: 1830,
          }}
        />
      </Drawer>
    </Cell>
  );
}

export function DiagnosticsSection() {
  return <Cell title="DiagnosticsCard"><DiagnosticsCard /></Cell>;
}

export function StatusOverviewSection() {
  return (
    <Cell title="Every fixture repository, with derived status and lag label" note="deriveStatus and lagLabel from lib/status.ts">
      <div className="grid gap-2 text-body">
        {REPOS.map((r) => (
          <div key={r.id} className="flex items-center gap-3">
            <StatusBadge status={deriveStatus(r)} />
            <span className="font-medium">{r.localName}</span>
            <span className="text-muted-foreground">{lagLabel(r)}</span>
          </div>
        ))}
      </div>
    </Cell>
  );
}

// -------------------------------------------------------------------- Screens

const SCREEN_HEIGHT = 760;
const RECEIPT_TIMESTAMP = Math.floor(Date.now() / 1000) - 7200;

/** Holds the state `AppShell` would own, so a screen renders standalone. */
export function ScreenHost({ screen }: { screen: "dashboard" | "repos" | "activity" | "settings" }) {
  const [activeGroupId, setActiveGroupId] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);
  const [dark, setDark] = useState(document.documentElement.classList.contains("dark"));
  const groups = GROUPS;
  return (
    <Cell title={`${screen[0].toUpperCase()}${screen.slice(1)} screen`} note="the real screen component; overlays stay inside this cell" height={SCREEN_HEIGHT} contained framed>
      {screen === "dashboard" && (
        <DashboardScreen onOpenRepos={noop} activeGroupId={activeGroupId} groups={groups} onLibraryChanged={noop} />
      )}
      {screen === "repos" && (
        <ReposScreen
          activeGroupId={activeGroupId}
          groups={groups}
          onClearGroup={() => setActiveGroupId(null)}
          onGroupsChanged={noop}
          addOpen={addOpen}
          onAddOpenChange={setAddOpen}
          onLibraryChanged={noop}
        />
      )}
      {screen === "activity" && <ActivityScreen activeGroupId={activeGroupId} />}
      {screen === "settings" && (
        <SettingsScreen
          dark={dark}
          onToggleTheme={() => {
            // The gallery's own toggle owns the class; this only mirrors it.
            setDark((d) => !d);
          }}
        />
      )}
      <div className="mt-3 flex gap-2 text-cell">
        <button type="button" className="rounded border border-border px-2 py-1" onClick={() => setActiveGroupId(activeGroupId === null ? 1 : null)}>
          {activeGroupId === null ? "Scope to group: work" : "Clear group scope"}
        </button>
        {screen === "repos" && (
          <button type="button" className="rounded border border-border px-2 py-1" onClick={() => setAddOpen(true)}>
            Open Add dialog
          </button>
        )}
      </div>
    </Cell>
  );
}

export function DrawerScreen() {
  const [open, setOpen] = useState(true);
  return (
    <Cell title="Repository detail drawer" note="RepoDetailPanel inside the wide Drawer, repo 1" height={SCREEN_HEIGHT} contained framed>
      <Button onClick={() => setOpen(true)}>Open drawer</Button>
      <Drawer open={open} onClose={() => setOpen(false)} size="wide" aria-labelledby={REPO_DETAIL_TITLE_ID}>
        <RepoDetailPanel id={1} onChanged={noop} onClose={() => setOpen(false)} />
      </Drawer>
    </Cell>
  );
}

export function ShellSection() {
  return (
    <Cell title="AppShell" note="the whole app: sidebar, navigation and screens" height={SCREEN_HEIGHT} contained framed>
      <div className="h-[700px] overflow-hidden rounded-md border border-border"><AppShell /></div>
    </Cell>
  );
}

