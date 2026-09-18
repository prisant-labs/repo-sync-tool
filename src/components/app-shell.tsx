import { useCallback, useEffect, useMemo, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";
import { Activity, AlertTriangle, LayoutDashboard, List, Plus, Settings, X } from "lucide-react";
import { cn } from "@/lib/utils";
import { events } from "@/lib/bindings";
import { useToast } from "@/hooks/use-toast";
import { Button } from "@/components/ui/button";
import { GroupsNav } from "@/components/groups-nav";
import {
  useBackendEvents,
  useDbRecoveryNotice,
  useGroups,
  useRepoGroupMemberships,
  useRepoList,
  useSummaryToday,
} from "@/hooks/queries";
import { groupScope } from "@/lib/group-scope";
import { DashboardScreen } from "@/screens/dashboard";
import { ReposScreen } from "@/screens/repos";
import { ActivityScreen } from "@/screens/activity";
import { SettingsScreen } from "@/screens/settings";

type View = "dashboard" | "repos" | "activity" | "settings";

// Every tracked repo, unfiltered - the shell's own read, narrowed afterwards
// by group rather than by the backend, because the sidebar needs the group
// intersection anyway and `repo_list` has no group parameter.
const ALL_REPOS = { enabledOnly: null, hostType: null, query: null };

const VIEWS: readonly View[] = ["dashboard", "repos", "activity", "settings"];

function isView(value: string): value is View {
  return (VIEWS as readonly string[]).includes(value);
}

// Sidebar order (SB6): Dashboard, Repos, Activity - then Settings,
// bottom-docked (its own nav below, separated by a hairline and pushed down
// with `mt-auto`). Groups renders below this list as a plain line under the
// WHOLE nav (A1), not as a subtree of Repos.
//
// SB6 and A1 both REVERSE the earlier ratified N5 / ledger-B1 shape, which
// put Activity above Repos and nested Groups one level beneath Repos behind
// an indent and a guide rail. The reversal is recorded in
// `_local/design/3-decisions/ui-delivery-plan.md` (H.1 for the order, F.1
// confirmed at J.1 for the placement), which is the only file that may
// record a decision.
//
// Still split into two arrays rather than one flat NAV so the render below
// can place Settings at the sidebar's foot without reordering `VIEWS`/
// `isView`, which the tray's `navigate:requested` handler validates against
// and must not change shape.
const PRIMARY_NAV: { id: View; label: string; Icon: typeof LayoutDashboard }[] = [
  { id: "dashboard", label: "Dashboard", Icon: LayoutDashboard },
  { id: "repos", label: "Repos", Icon: List },
  { id: "activity", label: "Activity", Icon: Activity },
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
 * (`bg-primary/10 text-primary-ink`) onto the ratified neutral 1B surface ramp -
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
  badge,
  dot,
  dotLabel,
}: {
  label: string;
  Icon: typeof LayoutDashboard;
  active: boolean;
  onClick: () => void;
  /**
   * SB4: a count rendered at the row's right edge. `null` renders NOTHING,
   * and that distinction is the whole point - `null` means "not knowable
   * yet", which a `0` would misreport as "none". A real zero (an empty
   * library) also renders nothing, matching the composite, which hides the
   * badge and the dot entirely on a fresh install: "Repos 0" beside "No
   * repositories yet" is noise, not information.
   */
  badge?: number | null;
  /** SB3: a status dot at the row's right edge. Same null-vs-false rule. */
  dot?: boolean;
  /** What the dot means, for anyone who cannot see a coloured circle. */
  dotLabel?: string;
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
      <span className="flex-1 text-left">{label}</span>
      {dot === true && (
        <>
          {/*
            The dot is a shape with a meaning, so it is hidden from the
            accessible name and the meaning is supplied as words beside it.
            NOT `role="status"`: that declares a live region, which would make
            a screen reader announce the dot every time a background check
            changes it, on every screen, unprompted.
          */}
          <span aria-hidden className="size-[7px] shrink-0 rounded-full bg-status-dirty" />
          <span className="sr-only">{`, ${dotLabel}`}</span>
        </>
      )}
      {badge != null && badge > 0 && (
        <>
          <span
            aria-hidden
            className={cn(
              "shrink-0 rounded-full px-1.5 font-mono text-[10px] tabular-nums",
              active ? "bg-background text-foreground" : "bg-muted text-muted-foreground",
            )}
          >
            {badge}
          </span>
          {/*
            A bare "2" in the accessible name reads as "Repos 2", which could
            be a count, a version, or a keyboard hint. The count is real
            information a sighted user gets, so it is not hidden - it is said
            properly instead.
          */}
          <span className="sr-only">
            {badge === 1 ? ", 1 repository" : `, ${badge} repositories`}
          </span>
        </>
      )}
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

  /**
   * SB3 and SB4: the sidebar reports two facts about the library it is a rail
   * for - whether anything needs attention (a dot on Dashboard) and how many
   * repositories there are (a count on Repos).
   *
   * Both are SCOPED to the engaged group, and that is not a free choice. The
   * screens they point at are already scoped: Dashboard intersects its
   * attention list against group membership, and Repos filters its table the
   * same way. An unscoped dot over a scoped Dashboard says "something needs
   * you" above a screen that says "All clear", and the user cannot tell which
   * one is lying. The scoping rule itself is `lib/group-scope.ts`, shared with
   * Dashboard so the two cannot drift apart.
   *
   * These reads duplicate Dashboard's own while Dashboard is showing. They go
   * to local SQLite through an IPC call that is already made on every screen
   * change, so the cost is small and the alternative - lifting Dashboard's
   * entire scoping block into the shell and threading it back down - is a far
   * larger change than the two indicators justify.
   */
  const shellRepos = useRepoList(ALL_REPOS);
  const shellSummary = useSummaryToday();
  const shellMemberships = useRepoGroupMemberships();
  const reposRefetch = shellRepos.refetch;
  const summaryRefetch = shellSummary.refetch;
  const membershipsRefetch = shellMemberships.refetch;
  // Without this the dot is a snapshot from mount: a check that turns a repo
  // dirty in the background would leave the rail claiming All clear.
  useBackendEvents(
    useCallback(() => {
      reposRefetch();
      summaryRefetch();
      membershipsRefetch();
    }, [reposRefetch, summaryRefetch, membershipsRefetch]),
  );

  const scope = useMemo(
    () => groupScope(activeGroupId, shellMemberships.data),
    [activeGroupId, shellMemberships.data],
  );
  const repoCount = scope.countRepos(shellRepos.data);
  const attentionCount = scope.countItems(shellSummary.data?.attention ?? null);
  // `null` (not knowable yet) and `0` (knowably nothing) both render no dot;
  // only a positive count does. Written as an explicit comparison rather than
  // a truthiness check so a future `null` cannot quietly read as false for the
  // wrong reason.
  const needsAttention = attentionCount !== null && attentionCount > 0;

  /**
   * L4: the sidebar's add-repo button does not own the Add-repositories
   * dialog - it asks the Repos screen to open its own.
   *
   * Lifting `AddReposDialog` into the shell looked simpler and is wrong:
   * `repo_add` emits no backend event (see the `events` list in bindings.ts),
   * so `ReposScreen` learns about a new repo only through the `onAdded`
   * callback wired to its own refetch. A shell-owned dialog would add repos
   * that the table behind it does not show until something else happens to
   * refetch.
   *
   * The open/closed flag lives HERE rather than on the Repos screen because
   * this button outlives that screen: `ReposScreen` unmounts on every
   * navigation, so a flag owned there could not survive the very navigation
   * this button performs. The screen takes it as a controlled prop.
   */
  const [addOpen, setAddOpen] = useState(false);
  function requestAddRepos() {
    setView("repos");
    setAddOpen(true);
  }

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
            Repo<span className="text-primary-ink">Sync</span>
          </span>
          <span className="ml-auto font-mono text-[11px] text-muted-foreground">
            {appVersion ?? "..."}
          </span>
        </div>
        <nav className="flex flex-col gap-0.5 px-2.5 py-2">
          {PRIMARY_NAV.map(({ id, label, Icon }) => (
            <NavButton
              key={id}
              label={label}
              Icon={Icon}
              active={view === id}
              onClick={() => setView(id)}
              badge={id === "repos" ? repoCount : undefined}
              dot={id === "dashboard" ? needsAttention : undefined}
              dotLabel={
                // Lower case: this lands mid-name, after "Dashboard, ".
                activeGroupId === null
                  ? "some repositories need attention"
                  : "some repositories in this group need attention"
              }
            />
          ))}
        </nav>

        {/*
          Groups: a plain line under the WHOLE nav (A1, asked for again at
          J.1 - "under the whole nav. I thought that was clarified elsewhere
          several times"). The indent and the left guide rail that used to
          say "this belongs to Repos" are gone, because the placement itself
          is no longer a claim about ownership. Every shipped Groups
          behaviour is unchanged; only this wrapper moved.
        */}
        <div className="flex min-h-0 flex-1 flex-col">
          <GroupsNav
            groups={groups}
            activeGroupId={activeGroupId}
            onSelectGroup={selectGroup}
            onClearActiveGroup={clearActiveGroup}
            refetchGroups={groupsState.refetch}
          />
        </div>

        {/*
          L4: add-repo above the hairline over Settings, icon and label LEFT
          aligned, in a reverse-contrast colour DISTINCT from the nav
          selection. Distinct is the requirement that shapes it: the nav's
          active item already owns the accent (a 2px `--primary` bar), so this
          button uses the neutral inversion instead - `bg-foreground` with
          `text-background`, 16.35:1 light and 14.48:1 dark on the sidebar,
          and no hue at all, so it can never be mistaken for "you are here".
          Hover darkens the fill rather than filtering brightness, which would
          also lighten the text.
        */}
        <div className="mt-auto px-2.5 pb-2">
          <button
            type="button"
            onClick={requestAddRepos}
            className="flex w-full items-center gap-3 rounded-md bg-foreground px-2.5 py-2 text-left text-sm font-semibold text-background transition-colors hover:bg-foreground/85 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
          >
            <Plus className="size-[17px] shrink-0" />
            Add repositories
          </button>
        </div>

        {/*
          Settings, bottom-docked (SB5): separated from what is above it by a
          hairline, rather than living in the primary nav list. `mt-auto` moved
          up to the add-repo block, which is now the first thing in the foot.
        */}
        <nav className="border-t border-border px-2.5 py-2">
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
              addOpen={addOpen}
              onAddOpenChange={setAddOpen}
              onReposChanged={reposRefetch}
            />
          )}
          {view === "activity" && <ActivityScreen activeGroupId={activeGroupId} />}
          {view === "settings" && <SettingsScreen dark={dark} onToggleTheme={toggle} />}
        </div>
      </main>
    </div>
  );
}
