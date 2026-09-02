import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Activity, AlertTriangle, LayoutDashboard, List, Settings, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { events } from "@/lib/bindings";
import { useToast } from "@/hooks/use-toast";
import { Button } from "@/components/ui/button";
import { GroupsNav } from "@/components/groups-nav";
import { useDbRecoveryNotice, useGroups } from "@/hooks/queries";
import { DashboardScreen } from "@/screens/dashboard";
import { ReposScreen } from "@/screens/repos";
import { ActivityScreen } from "@/screens/activity";
import { SettingsScreen } from "@/screens/settings";

type View = "dashboard" | "repos" | "activity" | "settings";

const VIEWS: readonly View[] = ["dashboard", "repos", "activity", "settings"];

function isView(value: string): value is View {
  return (VIEWS as readonly string[]).includes(value);
}

// Ratified sidebar order (ui-delivery-plan.md ledger B1 / N5, sidebar
// restructure and toolbar consolidation): Dashboard,
// Activity, Repos - with Groups nested one level beneath Repos (rendered
// separately below, not in this array) - then Settings, bottom-docked
// (its own nav below, separated by a hairline and pushed down with
// `mt-auto`). Split into two arrays rather than one flat NAV so the render
// below can place Settings at the sidebar's foot without reordering `VIEWS`/
// `isView`, which the tray's `navigate:requested` handler validates against
// and must not change shape.
const PRIMARY_NAV: { id: View; label: string; Icon: typeof LayoutDashboard }[] = [
  { id: "dashboard", label: "Dashboard", Icon: LayoutDashboard },
  { id: "activity", label: "Activity", Icon: Activity },
  { id: "repos", label: "Repos", Icon: List },
];
const SETTINGS_NAV: { id: View; label: string; Icon: typeof LayoutDashboard } = {
  id: "settings",
  label: "Settings",
  Icon: Settings,
};

/**
 * One sidebar nav button, shared by the primary list and the bottom-docked
 * Settings entry.
 *
 * Active state (N5, corrected post-review): moved off the accent tint
 * (`bg-primary/10 text-primary`) onto the ratified neutral 1B surface ramp -
 * `bg-sidebar-accent` is the same `0.935`/`0.269` well step `--muted` already
 * sits on. `text-foreground` on `bg-sidebar-accent` is 16.35:1 in light,
 * 14.48:1 in dark (`_generators/contrast.py`).
 *
 * The first cut paired that with `hover:bg-muted/60`, reasoned to differ from
 * active's full-opacity fill by weight alone. A Codex adversarial review
 * measured the actual COMPOSITE (muted painted at 60% alpha over the sidebar,
 * not the raw property value) and found it lands within 0.01 L of active's
 * flat fill - roughly 1.01:1 in light, 1.08:1 in dark, regardless of which
 * alpha is chosen. Re-derived here: `--sidebar` (0.945/0.205) and
 * `--muted`/`--sidebar-accent` (0.935/0.269) are only ~0.01 L apart in this
 * ramp, so ANY alpha blend of one over the other stays within that same 0.01
 * band - there is no opacity value that makes hover "genuinely different"
 * from active while both stay on the neutral ramp. That is smaller than the
 * 4% lightness step the design record already calls sub-threshold (jp has
 * rejected imperceptible option spacing before), so tuning the fill further
 * cannot fix this; it needs a lever outside the greyscale ramp entirely.
 *
 * The fix moves SEVERAL levers on active, none of them tunable-into-collision
 * by a background alpha: a 2px LEFT ACCENT BAR in `--primary` (a hue no
 * resting or hovered item ever carries, so it cannot converge with hover no
 * matter how the neutral ramp is tuned), the flat `bg-sidebar-accent` fill,
 * and `font-semibold`. The border is reserved (`border-l-2 border-transparent`
 * by default) rather than added only when active, so nothing shifts width on
 * activation. Hover keeps a light neutral wash purely as a "this is clickable"
 * touch - by the numbers above it can never be told apart from active on
 * background alone, so it no longer tries to; its real, load-bearing signal is
 * the text-color jump (`text-muted-foreground` to `text-foreground`, already
 * a large, verified contrast delta), which active also carries but hover now
 * shares only that lever, never the bar or the weight.
 */
function NavButton({
  label,
  Icon,
  active,
  onClick,
}: {
  label: string;
  Icon: typeof LayoutDashboard;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      aria-current={active ? "page" : undefined}
      className={cn(
        "flex items-center gap-3 rounded-md border-l-2 border-transparent px-2.5 py-2 text-sm transition-colors",
        active
          ? "border-l-primary bg-sidebar-accent font-semibold text-foreground"
          : "font-medium text-muted-foreground hover:bg-muted/40 hover:text-foreground",
      )}
    >
      <Icon className="size-[17px]" />
      {label}
    </button>
  );
}

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
  const groupsState = useGroups();
  const groups = groupsState.data ?? [];
  const toast = useToast();

  // E-02 AC7 / BL-NI-33: the one-time database-recovery notice, read once at
  // launch. It surfaces only when the startup migration failed and the old
  // database was moved aside; the user can dismiss it for the session.
  const recovery = useDbRecoveryNotice();
  const [recoveryDismissed, setRecoveryDismissed] = useState(false);
  const showRecovery = !recoveryDismissed && recovery.data?.recovered === true;

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
          {PRIMARY_NAV.map(({ id, label, Icon }) => (
            <NavButton key={id} label={label} Icon={Icon} active={view === id} onClick={() => setView(id)} />
          ))}
        </nav>

        {/*
          Groups, nested one level beneath Repos (ui-delivery-plan.md ledger
          B1 / N5, coverage-matrix.md section 1). The indent plus the left
          guide rail are what say "this belongs to Repos" rather than "this is
          a second top-level nav list"; no mockup specified an exact value, so
          this materialization is provisional and named in the PR body for
          veto. Every shipped Groups behaviour (matrix section 2) is
          unchanged: only this wrapper and GroupsNav's own outer spacing
          moved, nothing inside it did.
        */}
        <div className="ml-[23px] flex min-h-0 flex-1 flex-col border-l border-border pl-2">
          <GroupsNav
            groups={groups}
            activeGroupId={activeGroupId}
            railActive={view === "repos"}
            onSelectGroup={selectGroup}
            onClearActiveGroup={clearActiveGroup}
            refetchGroups={groupsState.refetch}
          />
        </div>

        {/*
          Settings, bottom-docked (N5): pushed to the sidebar's foot with
          `mt-auto` and separated from Groups above it by a hairline, rather
          than living in the primary nav list.
        */}
        <nav className="mt-auto border-t border-border px-2.5 py-2">
          <NavButton
            label={SETTINGS_NAV.label}
            Icon={SETTINGS_NAV.Icon}
            active={view === SETTINGS_NAV.id}
            onClick={() => setView(SETTINGS_NAV.id)}
          />
        </nav>
      </aside>

      {/*
        `min-h-0` matters here for the same reason it matters everywhere else
        in this chain (grid/flex items default to `min-height: auto`, which
        refuses to shrink below CONTENT size): `main` is a grid item of the
        `h-svh` grid above, and without this override a tall enough screen
        (a long Repos table, pre-`fill`) grows `main` past the grid's row
        instead of letting it stretch to fill and scroll internally. Found
        empirically (a real browser, not jsdom) in the fix round after the
        Codex review of PR #73, finding 1: `DataTable`'s own internal-scroll
        fix could not work until THIS ancestor was also bounded - `overflow-
        auto` below only ever does anything once every ancestor up to a
        definite-height one agrees to actually stop growing.
      */}
      <main className="flex min-h-0 min-w-0 flex-col">
        {showRecovery && recovery.data && (
          // Q1 -> 1A (ui-delivery-plan.md decision queue, N7 consistency
          // sweep): the old full-fill `bg-status-dirty/12` region (with its
          // own tinted `border-status-dirty/40` bottom hairline) becomes a
          // thin left-edge stripe on a neutral surface, matching the
          // Diagnostics warnings band's identical treatment
          // (`diagnostics-card.tsx`) and PR #78's active-nav bar idiom: a
          // solid `bg-muted` fill (never an alpha tint) plus a 2px
          // `border-l-status-dirty` bar, with the bottom hairline reverting
          // to the plain neutral `border-border`. Icon, wording and dismiss
          // behavior are unchanged.
          //
          // Measured (`_generators/contrast.py`, N7 section): status-dirty on
          // the `--muted` well (non-text 3:1 floor) is 4.71:1 light, 8.03:1
          // dark. The two text lines (text-foreground opaque, and the
          // pre-existing text-foreground/90 body copy - alpha-composited in
          // gamma-encoded sRGB, the CSS default) clear the 4.5:1 text floor
          // by a wide margin: 16.35:1 / 13.40:1 light, 14.48:1 / 11.97:1 dark.
          <div className="flex items-start gap-3 border-b border-l-2 border-border border-l-status-dirty bg-muted px-6 py-3">
            <AlertTriangle className="mt-0.5 size-4 shrink-0 text-status-dirty" />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-foreground">
                Database was reset after a failed migration
              </p>
              <p className="mt-0.5 text-xs text-foreground/90">
                RepoSync could not migrate your existing database, so it started a fresh one.
                {recovery.data.backupPath ? (
                  <>
                    {" "}
                    Your previous database was preserved at{" "}
                    <span className="break-all font-mono">{recovery.data.backupPath}</span>.
                  </>
                ) : (
                  " Your previous database was preserved alongside it."
                )}
              </p>
            </div>
            <Button
              variant="ghost"
              size="icon"
              className="-mr-2 shrink-0"
              onClick={() => setRecoveryDismissed(true)}
              aria-label="Dismiss database recovery notice"
            >
              <X />
            </Button>
          </div>
        )}
        {/*
          The page inset lives in `PageShell`, not here. It used to be `p-6` on
          this scroller, which meant a screen wanting a sticky header had nowhere
          to stick to: `top-0` would pin to the padding box and content would
          scroll through the gap above it. This element now owns scrolling and
          nothing else.
        */}
        <div className="min-h-0 flex-1 overflow-auto">
          {view === "dashboard" && (
            <DashboardScreen
              onOpenRepos={() => setView("repos")}
              activeGroupId={activeGroupId}
              groups={groups}
            />
          )}
          {view === "repos" && (
            <ReposScreen
              activeGroupId={activeGroupId}
              groups={groups}
              onClearGroup={clearActiveGroup}
              onGroupsChanged={groupsState.refetch}
            />
          )}
          {view === "activity" && <ActivityScreen />}
          {view === "settings" && <SettingsScreen dark={dark} onToggleTheme={toggle} />}
        </div>
      </main>
    </div>
  );
}
