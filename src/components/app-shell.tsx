import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Activity, LayoutDashboard, List, Moon, Settings, Sun } from "lucide-react";
import { cn } from "@/lib/utils";
import { events } from "@/lib/bindings";
import { useToast } from "@/hooks/use-toast";
import { Button } from "@/components/ui/button";
import { GroupsNav } from "@/components/groups-nav";
import { useGroups } from "@/hooks/queries";
import { DashboardScreen } from "@/screens/dashboard";
import { ReposScreen } from "@/screens/repos";
import { ActivityScreen } from "@/screens/activity";
import { SettingsScreen } from "@/screens/settings";

type View = "dashboard" | "repos" | "activity" | "settings";

const VIEWS: readonly View[] = ["dashboard", "repos", "activity", "settings"];

function isView(value: string): value is View {
  return (VIEWS as readonly string[]).includes(value);
}

const NAV: { id: View; label: string; Icon: typeof LayoutDashboard }[] = [
  { id: "dashboard", label: "Dashboard", Icon: LayoutDashboard },
  { id: "repos", label: "Repos", Icon: List },
  { id: "activity", label: "Activity", Icon: Activity },
  { id: "settings", label: "Settings", Icon: Settings },
];

function useTheme() {
  const [dark, setDark] = useState(false);
  useEffect(() => {
    document.documentElement.classList.toggle("dark", dark);
  }, [dark]);
  return { dark, toggle: () => setDark((d) => !d) };
}

/**
 * The running app version, read once from Tauri at mount (the real semver
 * from `tauri.conf.json`, not a hand-maintained literal). Falls back to a
 * loading placeholder while the async call resolves, following the same
 * mounted-guard idiom as `useAsync` (hooks/use-async.ts).
 */
function useAppVersion() {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    getVersion().then((v) => {
      if (active) setVersion(v);
    });
    return () => {
      active = false;
    };
  }, []);
  return version;
}

export function AppShell() {
  const [view, setView] = useState<View>("dashboard");
  const [activeGroupId, setActiveGroupId] = useState<number | null>(null);
  const { dark, toggle } = useTheme();
  const appVersion = useAppVersion();
  const active = NAV.find((n) => n.id === view);
  const groupsState = useGroups();
  const groups = groupsState.data ?? [];
  const toast = useToast();

  // Backend-driven shell events (E-13 tray, BL-NI-31):
  //   - `navigate:requested` routes the shell to a named view (the tray "Settings"
  //     item opens the window on the settings view).
  //   - `error:raised` surfaces a background failure that has no synchronous caller
  //     (e.g. a tray "Check All Now" / "Open recent" failure) as an error toast.
  // `setView` (useState) and `toast` (context) are referentially stable, so the
  // subscription is set up once.
  useEffect(() => {
    const subscriptions = [
      events.navigateRequested.listen((e) => {
        if (isView(e.payload.target)) setView(e.payload.target);
      }),
      events.errorRaised.listen((e) => {
        toast("error", e.payload.error.message, e.payload.error.remediation);
      }),
    ];
    return () => {
      void Promise.all(subscriptions).then((unlisteners) => {
        for (const off of unlisteners) off();
      });
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function selectGroup(id: number | null) {
    setActiveGroupId(id);
    setView("repos");
  }

  // Clear the active group filter without switching views. Unlike
  // `selectGroup`, this has no navigation side effect, which matters when the
  // active filter's group is deleted from the sidebar: that can happen from
  // any screen (the sidebar renders everywhere), and should not force-navigate
  // to Repos (E-16 Known defect 6).
  function clearActiveGroup() {
    setActiveGroupId(null);
  }

  return (
    <div className="grid h-svh grid-cols-[232px_1fr] bg-background text-foreground">
      <aside className="flex min-h-0 flex-col border-r border-border bg-sidebar">
        <div className="flex items-center gap-2.5 px-4 py-4">
          <div className="grid size-7 place-items-center rounded-md bg-primary text-sm font-bold text-primary-foreground">
            R
          </div>
          <span className="font-semibold">
            Repo<span className="text-primary">Sync</span>
          </span>
          <span className="ml-auto font-mono text-[11px] text-muted-foreground">
            {appVersion ?? "..."}
          </span>
        </div>
        <nav className="flex flex-col gap-0.5 px-2.5 py-2">
          {NAV.map(({ id, label, Icon }) => (
            <button
              key={id}
              onClick={() => setView(id)}
              className={cn(
                "flex items-center gap-3 rounded-md px-2.5 py-2 text-sm font-medium transition-colors",
                view === id
                  ? "bg-primary/10 text-primary"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              <Icon className="size-[17px]" />
              {label}
            </button>
          ))}
        </nav>
        <GroupsNav
          groups={groups}
          activeGroupId={activeGroupId}
          onSelectGroup={selectGroup}
          onClearActiveGroup={clearActiveGroup}
          refetchGroups={groupsState.refetch}
        />
      </aside>

      <main className="flex min-w-0 flex-col">
        <header className="flex items-center gap-3 border-b border-border px-6 py-3">
          <h1 className="font-mono text-xs font-semibold uppercase tracking-widest text-muted-foreground">
            RepoSync / <span className="text-foreground">{active ? active.label : ""}</span>
          </h1>
          <Button
            variant="outline"
            size="icon"
            className="ml-auto"
            onClick={toggle}
            title="Toggle light / dark"
            aria-label={dark ? "Switch to light theme" : "Switch to dark theme"}
          >
            {dark ? <Sun /> : <Moon />}
          </Button>
        </header>
        <div className="min-h-0 flex-1 overflow-auto p-6">
          {view === "dashboard" && <DashboardScreen onOpenRepos={() => setView("repos")} />}
          {view === "repos" && (
            <ReposScreen
              activeGroupId={activeGroupId}
              groups={groups}
              onClearGroup={clearActiveGroup}
              onGroupsChanged={groupsState.refetch}
            />
          )}
          {view === "activity" && <ActivityScreen />}
          {view === "settings" && <SettingsScreen />}
        </div>
      </main>
    </div>
  );
}
