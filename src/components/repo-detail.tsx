import { useCallback, useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import {
  AlertTriangle,
  ArrowDownToLine,
  Ban,
  CheckCircle2,
  ChevronRight,
  Clock,
  ExternalLink,
  FolderOpen,
  GitBranch,
  GitCompare,
  GitPullRequest,
  Globe,
  Hash,
  History,
  Link2,
  Package,
  Pause,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  SlidersHorizontal,
  Terminal,
  X,
  XCircle,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { commands } from "@/lib/bindings";
import type {
  ActivityRecord,
  GroupSummary,
  RepoDetail as RepoDetailData,
  UpdateMode,
} from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { AsyncPanel } from "@/components/async-panel";
import { StatusBadge } from "@/components/status-badge";
import { LagSignal } from "@/components/lag-signal";
import { ActivityReceipt, ACTIVITY_RECEIPT_TITLE_ID } from "@/components/activity-receipt";
import { Drawer } from "@/components/ui/drawer";
import { Tabs, TabList, TabPanel } from "@/components/ui/tabs";
import { useToast } from "@/hooks/use-toast";
import {
  useActivity,
  useGroups,
  useGroupsForRepo,
  useRepoBackendEvents,
  useRepoDetail,
  useSettings,
} from "@/hooks/queries";
import { ACTIVITY_FETCH_LIMIT, ACTIVITY_PAGE_LIMIT, paginate } from "@/lib/activity";
import type { AsyncState } from "@/hooks/use-async";
import {
  checkFailureMessage,
  deriveStatus,
  lagLabel,
  lagMagnitude,
  relativeTime,
  STATUS_STYLE,
  type RepoStatus,
} from "@/lib/status";
import { cn } from "@/lib/utils";

/**
 * The id of the drawer header's repo-name span, so the OUTER `Drawer` (owned
 * by whichever screen opened this panel - `screens/repos.tsx` or
 * `screens/dashboard.tsx`) can name itself via `aria-labelledby` from the
 * same visible heading a sighted user reads, rather than a generic "dialog"
 * a screen reader would otherwise announce (Codex adversarial review,
 * finding 3, confirmed - neither this panel's own drawer nor its nested
 * receipt drawer had an accessible name). Exported rather than duplicated as
 * a string literal at each call site, so the two ends cannot drift.
 * Reusing one id across those two screens is safe: `AppShell` renders only
 * one screen at a time, so at most one `RepoDetailPanel` is ever mounted.
 */
export const REPO_DETAIL_TITLE_ID = "repo-detail-panel-title";

/** Fire a mutating command, then toast + refetch + tell the parent list to refresh. */
type RunFn = (
  key: string,
  action: () => Promise<unknown>,
  okTitle: string,
  okMessage?: string,
) => void;

const FOCAL_BORDER: Record<RepoStatus, string> = {
  sync: "border-status-sync/40",
  ahead: "border-status-sync/40",
  behind: "border-status-behind/40",
  dirty: "border-status-dirty/40",
  failed: "border-status-failed/40",
  paused: "border-status-paused/40",
  noUpstream: "border-status-no-upstream/40",
};

// The two dirtyHandling / branchPolicy fields are not exposed on RepoDetail, so
// changing the mode resends the safest V1 defaults (skip a dirty tree, default
// branch only). These can only narrow risk, never widen it.
const POLICY_OPTIONS: { mode: UpdateMode; label: string; blurb: string; disabled?: boolean }[] = [
  { mode: "check_only", label: "Check only", blurb: "Detect changes, never fetch or pull." },
  { mode: "fetch_only", label: "Fetch only", blurb: "Download objects, leave the working tree untouched." },
  { mode: "pull_ff_only", label: "Fast-forward", blurb: "Pull only when it fast-forwards cleanly." },
  { mode: "pull_standard", label: "Merge pull", blurb: "Not available in this release.", disabled: true },
  { mode: "pull_rebase", label: "Rebase pull", blurb: "Not available in this release.", disabled: true },
];

type PanelTab = "overview" | "activity" | "settings";

const PANEL_TABS: { value: PanelTab; label: string }[] = [
  { value: "overview", label: "Overview" },
  { value: "activity", label: "Activity" },
  { value: "settings", label: "Settings" },
];

export function RepoDetailPanel({
  id,
  onChanged,
  onClose,
}: {
  id: number;
  onChanged: () => void;
  onClose: () => void;
}) {
  const detail = useRepoDetail(id);
  const groupsState = useGroups();
  const memberships = useGroupsForRepo(id);
  const settings = useSettings();
  // Scoped to this repo (BL-NI-93's groupId is not needed here: a single repo
  // id is already the tightest possible constraint). No action/status/group
  // narrowing per the ratified shape - the panel's Activity tab carries no
  // filter controls.
  const activity = useActivity({
    repoId: id,
    groupId: null,
    actionType: null,
    status: null,
    limit: ACTIVITY_FETCH_LIMIT,
  });
  // Trimmed, because a whitespace-only command is as unusable as an empty one
  // and the Settings field accepts free text.
  const hasEditorCommand = (settings.data?.editorCommand ?? "code").trim().length > 0;
  const hasTerminalCommand = (settings.data?.terminalCommand ?? "wt").trim().length > 0;
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const [groupBusyId, setGroupBusyId] = useState<number | null>(null);
  // Persists across a repo switch (this component instance is reused when the
  // parent list swaps `id` without remounting the drawer): staying on the
  // Activity tab while stepping through a few repos in a row seemed more
  // useful than always resetting to Overview. Deliberate choice, flagged in
  // the PR for veto.
  const [tab, setTab] = useState<PanelTab>("overview");
  const refetch = detail.refetch;
  const refetchGroups = groupsState.refetch;
  const refetchMemberships = memberships.refetch;
  const refetchActivity = activity.refetch;

  // Keep the open drawer live when a background scheduled check/update
  // completes for this repo, instead of only refreshing after this drawer's
  // own actions (finding 11 / BL-NI-28). Now also refreshes the Activity tab,
  // since a background check writes a new activity row this repo's own log
  // should show without requiring the user to close and reopen the drawer.
  const onBackendEvent = useCallback(() => {
    refetch();
    refetchActivity();
  }, [refetch, refetchActivity]);
  useRepoBackendEvents(id, onBackendEvent);

  const run = useCallback<RunFn>(
    (key, action, okTitle, okMessage) => {
      setBusy(key);
      action()
        .then(
          () => {
            toast("ok", okTitle, okMessage);
            refetch();
            onChanged();
          },
          (e: unknown) => {
            toast("error", "Action failed", e instanceof IpcError ? e.message : String(e));
          },
        )
        .finally(() => setBusy(null));
    },
    [toast, refetch, onChanged],
  );

  /**
   * "Check now" cannot use `run`, because `run` decides success from whether the
   * promise resolved and that stopped being the right question.
   *
   * Since BL-NI-04 a check whose fetch failed RESOLVES, carrying `failed: true`
   * and a typed reason, so that the completion event fires and every window hears
   * about it. Routed through `run`, the same failed check would toast
   * "Checked <name>" in the success style: strictly worse than the bug that was
   * fixed, because silence about a failure is bad and a green confirmation of one
   * is a lie.
   *
   * A rejected promise still means the check could not RUN at all (git missing,
   * path gone), which is a different message and keeps the generic error arm.
   */
  const repoName = detail.data?.localName ?? "this repository";
  const checkNow = useCallback(() => {
    setBusy("check");
    unwrap(commands.repoCheckNow(id))
      .then(
        (result) => {
          if (result.failed) {
            toast("error", `Check failed for ${repoName}`, checkFailureMessage(result.reason));
          } else {
            toast("ok", `Checked ${repoName}`);
          }
          refetch();
          onChanged();
        },
        (e: unknown) => {
          toast("error", "Could not check", e instanceof IpcError ? e.message : String(e));
        },
      )
      .finally(() => setBusy(null));
  }, [id, repoName, toast, refetch, onChanged]);

  /**
   * Removal cannot use `run` either: `run` refetches this repo's detail on
   * success, and after a remove the id no longer exists, so the refetch would
   * flash NotFound inside a drawer that is about to close. Success closes the
   * drawer and lets the parent refresh its list; failure leaves the drawer
   * open so the repo is still there to look at.
   *
   * `alive` guards the close. The backend holds the per-repo lock across the
   * delete, so a removal can resolve after this panel is gone (the user closed
   * the drawer, or opened a different repo, while a scheduled check held the
   * lock). `onClose` closes WHATEVER drawer is open at that moment, so a stale
   * resolve must refresh the list without touching it.
   */
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  const removeRepo = useCallback(() => {
    setBusy("remove");
    const finish = (message: string) => {
      toast("ok", `Removed ${repoName}`, message);
      if (alive.current) onClose();
      onChanged();
    };
    unwrap(commands.repoRemove(id))
      .then(
        () => finish("The folder on disk was not touched."),
        (e: unknown) => {
          // An already-gone repo IS the requested end state, not a failure:
          // another instance sharing the database (BL-NI-73) may have removed
          // it first. Converge instead of stranding a drawer on a dead id.
          if (e instanceof IpcError && e.code === "db.not_found") {
            finish("It was already gone; the folder on disk was not touched.");
            return;
          }
          toast(
            "error",
            "Could not remove",
            e instanceof IpcError
              ? [e.message, e.remediation].filter(Boolean).join(" ")
              : String(e),
          );
        },
      )
      .finally(() => setBusy(null));
  }, [id, repoName, toast, onClose, onChanged]);

  const toggleGroup = useCallback(
    async (group: GroupSummary, isMember: boolean) => {
      setGroupBusyId(group.id);
      try {
        await unwrap(
          isMember ? commands.groupUnassign(id, group.id) : commands.groupAssign(id, group.id),
        );
        toast("ok", isMember ? "Removed from group" : "Added to group", group.name);
        // Refresh this repo's memberships and the group list (member counts),
        // then let the parent refresh its list + membership map + sidebar.
        refetchMemberships();
        refetchGroups();
        onChanged();
      } catch (e) {
        toast("error", "Could not update group", e instanceof IpcError ? e.message : String(e));
      } finally {
        setGroupBusyId(null);
      }
    },
    [id, toast, refetchMemberships, refetchGroups, onChanged],
  );

  return (
    <div className="flex h-full flex-col">
      <div className="flex items-center gap-2 border-b border-border px-5 py-4">
        <GitBranch className="size-4 shrink-0 text-muted-foreground" />
        <span id={REPO_DETAIL_TITLE_ID} className="truncate font-mono text-sm font-semibold">
          {detail.data?.localName ?? "Repository"}
        </span>
        <Button variant="ghost" size="icon" className="ml-auto" onClick={onClose} aria-label="Close">
          <X />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-hidden">
        <AsyncPanel state={detail}>
          {(r) => (
            <DetailBody
              r={r}
              busy={busy}
              run={run}
              onCheckNow={checkNow}
              globalMinutes={settings.data?.globalCheckMinutes ?? null}
              hasEditorCommand={hasEditorCommand}
              hasTerminalCommand={hasTerminalCommand}
              groups={groupsState.data ?? []}
              memberIds={memberships.data ?? []}
              groupBusyId={groupBusyId}
              onToggleGroup={toggleGroup}
              onRemove={removeRepo}
              tab={tab}
              onTabChange={setTab}
              activity={activity}
            />
          )}
        </AsyncPanel>
      </div>
    </div>
  );
}

function DetailBody({
  r,
  busy,
  run,
  onCheckNow,
  globalMinutes,
  hasEditorCommand,
  hasTerminalCommand,
  groups,
  memberIds,
  groupBusyId,
  onToggleGroup,
  onRemove,
  tab,
  onTabChange,
  activity,
}: {
  r: RepoDetailData;
  busy: string | null;
  run: RunFn;
  onCheckNow: () => void;
  globalMinutes: number | null;
  /** Whether `settings.editorCommand` is set, so "Open in -> Editor" can succeed. */
  hasEditorCommand: boolean;
  /** Whether `settings.terminalCommand` is set, so "Open in -> Terminal" can succeed. */
  hasTerminalCommand: boolean;
  groups: GroupSummary[];
  memberIds: number[];
  groupBusyId: number | null;
  onToggleGroup: (group: GroupSummary, isMember: boolean) => void;
  onRemove: () => void;
  tab: PanelTab;
  onTabChange: (tab: PanelTab) => void;
  activity: AsyncState<ActivityRecord[]>;
}) {
  const status = deriveStatus(r);
  const style = STATUS_STYLE[status];
  const isBusy = busy !== null;
  const badgeCount =
    status === "behind" ? (r.behindCount ?? 0) : status === "ahead" ? (r.aheadCount ?? 0) : undefined;

  return (
    <div className="flex h-full min-h-0 flex-col">
      {/*
        Chrome above the tabs (ratified 2026-08-31 shape): repo name/description,
        then the third header line carrying the status chip, the archived pill,
        and the member-only group pills (G2: in the header). Deliberate reading
        of a header that pre-dated a "path" line in the mockup this decision
        traces to - no path row is added here, since Path stays inside "Where it
        lives" (Overview tab) rather than duplicating it in chrome. Flagged in
        the PR for veto.
      */}
      <div className="flex flex-col gap-2 px-5 pt-5 pb-3">
        <div className="font-mono text-lg font-bold">{r.localName}</div>
        {r.description && <p className="text-sm text-muted-foreground">{r.description}</p>}
        <div className="flex flex-wrap items-center gap-2">
          <StatusBadge status={status} count={badgeCount} />
          {r.isArchived && (
            <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">
              archived
            </span>
          )}
        </div>
        <GroupPills
          groups={groups}
          memberIds={memberIds}
          groupBusyId={groupBusyId}
          onToggleGroup={onToggleGroup}
        />
      </div>

      <Tabs value={tab} onValueChange={(v) => onTabChange(v as PanelTab)} className="flex min-h-0 flex-1 flex-col">
        <TabList aria-label="Repository sections" tabs={PANEL_TABS} className="px-5" />
        {/*
          `overflow-auto` lives on EACH TabPanel, not on a shared wrapper
          around all three, and that is what keeps each tab's scroll position
          its OWN.

          `TabPanel` keeps every panel mounted and toggles the native `hidden`
          attribute rather than unmounting (see `ui/tabs.tsx`'s file doc
          comment). A panel therefore never loses its `scrollTop`: leave
          Settings scrolled, switch away, come back, and Settings is where you
          left it. Per-panel scroll regions are what stop that from leaking
          across tabs. With ONE shared scroll container the single `scrollTop`
          would be shared by all three, so switching in from a scrolled tab
          would land on Overview already scrolled - caught by a real-browser
          check during verification, with the Focal card clipped off the top.

          This is also the ARIA-correct shape: a focusable (`tabIndex=0`)
          `tabpanel` owning its own scrolling.

          HISTORY, because this comment said the opposite until 2026-09-04: it
          was written when `TabPanel` unmounted its content, and argued that a
          fresh mount always starts at the top. A later commit in the same pull
          request switched to mounted-and-hidden - its own message calls the
          resulting scroll retention "a discovered behavior change" - and the
          comment was never updated. Per-panel scrolling stayed correct
          throughout; only the reason changed.
        */}
          <TabPanel value="overview" className="min-h-0 flex-1 overflow-auto flex flex-col gap-5 p-5">
            <div className={cn("rounded-lg border p-4", style.tint, FOCAL_BORDER[status])}>
              <Focal r={r} status={status} busy={busy} run={run} onCheckNow={onCheckNow} />
            </div>

            <SyncStateSection r={r} />

            <div className="flex flex-wrap gap-2">
              <Button variant="outline" size="sm" disabled={isBusy} onClick={onCheckNow}>
                <RefreshCw className={busy === "check" ? "animate-spin" : undefined} /> Check now
              </Button>
              <Button
                variant="outline"
                size="sm"
                disabled={isBusy}
                onClick={() => run("meta", () => unwrap(commands.repoRefreshMetadata(r.id)), "Metadata refreshed")}
              >
                <Package /> Refresh metadata
              </Button>
              {r.enabled && !r.autoPaused && (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={isBusy}
                  onClick={() => run("pause", () => unwrap(commands.repoSetEnabled(r.id, false)), `Paused ${r.localName}`)}
                >
                  <Pause /> Pause
                </Button>
              )}
            </div>

            <section>
              <SectionLabel icon={ExternalLink}>Open in</SectionLabel>
              <div className="flex flex-wrap gap-2">
                <OpenButton
                  label="Folder"
                  icon={FolderOpen}
                  disabled={isBusy}
                  onClick={() => run("folder", () => unwrap(commands.repoOpenFolder(r.id)), "Opened folder")}
                />
                {/*
                  Terminal and Editor are gated on their settings being set at
                  all. Both commands return `InvalidSetting` when the column is
                  NULL, so before 0009 backfilled them these buttons looked live
                  and failed on click, with a Settings field whose placeholder
                  ("code", "default") read like a configured value. A button
                  that cannot succeed should say so before it is pressed rather
                  than after.

                  `?? true` while settings are loading: assume available rather
                  than flashing a disabled control that enables a moment later.
                */}
                <OpenButton
                  label="Terminal"
                  icon={Terminal}
                  disabled={isBusy || !hasTerminalCommand}
                  title={hasTerminalCommand ? undefined : "Set a terminal command in Settings"}
                  onClick={() => run("terminal", () => unwrap(commands.repoOpenTerminal(r.id)), "Opened terminal")}
                />
                <OpenButton
                  label="Editor"
                  icon={Pencil}
                  disabled={isBusy || !hasEditorCommand}
                  title={hasEditorCommand ? undefined : "Set an editor command in Settings"}
                  onClick={() => run("editor", () => unwrap(commands.repoOpenEditor(r.id)), "Opened editor")}
                />
                {r.remoteOriginUrl && (
                  <>
                    <OpenButton
                      label="Remote"
                      icon={ExternalLink}
                      disabled={isBusy}
                      onClick={() => run("remote", () => unwrap(commands.repoOpenRemote(r.id)), "Opened remote")}
                    />
                    {/*
                      The round-five web-link glyph (BL-NI-94): a globe that
                      opens the repo's web URL. Deliberately redundant with the
                      "Remote" button above - both call `repoOpenRemote`,
                      because that command IS "the existing remote-open path"
                      the ratified shape names, and no separate binding exists
                      for a repo's "web view" distinct from its git remote's
                      translated URL. Shipping both, as the ratified shape
                      states them, flagged in the PR for veto.
                    */}
                    <Button
                      variant="secondary"
                      size="icon"
                      aria-label="Open repository website"
                      title="Open repository website"
                      disabled={isBusy}
                      onClick={() => run("remote", () => unwrap(commands.repoOpenRemote(r.id)), "Opened remote")}
                    >
                      <Globe />
                    </Button>
                  </>
                )}
                {r.homepage && (
                  <Button
                    variant="secondary"
                    size="icon"
                    aria-label="Open homepage"
                    title="Open homepage"
                    disabled={isBusy}
                    onClick={() => run("homepage", () => unwrap(commands.repoOpenHomepage(r.id)), "Opened homepage")}
                  >
                    <Link2 />
                  </Button>
                )}
              </div>
            </section>

            {r.latestReleaseTag && (
              <section>
                <SectionLabel icon={Package}>Latest release</SectionLabel>
                <div className="flex items-center gap-2 rounded-md border border-border bg-background/40 px-3 py-2">
                  <Package className="size-4 shrink-0 text-status-release" />
                  <span className="font-mono text-sm font-semibold">{r.latestReleaseTag}</span>
                  {r.latestReleaseAt !== null && (
                    <span className="ml-auto text-xs text-muted-foreground">{relativeTime(r.latestReleaseAt)}</span>
                  )}
                </div>
              </section>
            )}

            <section>
              <SectionLabel icon={FolderOpen}>Where it lives</SectionLabel>
              <dl className="overflow-hidden rounded-md border border-border">
                <KvRow label="Path" icon={FolderOpen} value={r.localPath} mono breakAll />
                <KvRow
                  label="Remote"
                  icon={Globe}
                  value={
                    r.remoteOriginUrl ? (
                      <button
                        type="button"
                        disabled={isBusy}
                        onClick={() => run("remote", () => unwrap(commands.repoOpenRemote(r.id)), "Opened remote")}
                        className="text-left text-primary underline-offset-2 hover:underline disabled:cursor-not-allowed disabled:text-muted-foreground disabled:no-underline"
                      >
                        {r.remoteOriginUrl}
                      </button>
                    ) : (
                      "none"
                    )
                  }
                  mono
                  breakAll
                />
                <KvRow label="Last checked" icon={Clock} value={relativeTime(r.lastCheckedAt)} />
                <KvRow label="Last fetched" icon={RefreshCw} value={relativeTime(r.lastFetchedAt)} />
              </dl>
            </section>
          </TabPanel>

          <TabPanel value="activity" className="min-h-0 flex-1 overflow-auto flex flex-col gap-3 p-5">
            <ActivitySection repoName={r.localName} state={activity} />
          </TabPanel>

          <TabPanel value="settings" className="min-h-0 flex-1 overflow-auto flex flex-col gap-5 p-5">
            {/*
              Keyed distinctly from RemoveSection below, not both `key={r.id}`
              (a pre-existing defect found while verifying N4 in a real
              browser: React warns "two children with the same key" because
              both keys resolved to the identical string regardless of the
              two components' different types - key uniqueness is scoped to
              the whole sibling list, not per element type). Both still need
              a key on the repo id so switching repos resets each section's
              own local state (the confirm-armed flag, the cadence draft).
            */}
            <CadenceSection key={`cadence-${r.id}`} r={r} globalMinutes={globalMinutes} busy={busy} run={run} />

            <section>
              <SectionLabel icon={SlidersHorizontal}>Update policy</SectionLabel>
              <div className="flex flex-col gap-1.5">
                {POLICY_OPTIONS.map((opt) => (
                  <PolicyOption
                    key={opt.mode}
                    opt={opt}
                    current={r.updateMode}
                    disabled={isBusy}
                    onSelect={() =>
                      run(
                        "policy",
                        () =>
                          unwrap(
                            commands.repoSetPolicy(r.id, {
                              mode: opt.mode,
                              dirtyHandling: "skip",
                              branchPolicy: "default_branch_only",
                            }),
                          ),
                        `Policy set to ${opt.label}`,
                      )
                    }
                  />
                ))}
              </div>
            </section>

            <RemoveSection key={`remove-${r.id}`} name={r.localName} busy={busy} onRemove={onRemove} />
          </TabPanel>
      </Tabs>
    </div>
  );
}

function Focal({
  r,
  status,
  busy,
  run,
  onCheckNow,
}: {
  r: RepoDetailData;
  status: RepoStatus;
  busy: string | null;
  run: RunFn;
  onCheckNow: () => void;
}) {
  const style = STATUS_STYLE[status];
  const isBusy = busy !== null;

  if (status === "behind") {
    return (
      <>
        <div className={cn("text-sm font-bold", style.text)}>
          {r.behindCount ?? 0} {r.behindCount === 1 ? "commit" : "commits"} behind origin
        </div>
        <p className="mt-0.5 text-xs text-foreground/80">
          Fast-forward pulls only new commits; it never rewrites history and stops if the merge would not
          be clean.
        </p>
        <LagSignal className="mt-3" status={status} magnitude={lagMagnitude(r)} label={lagLabel(r)} />
        <Button
          className="mt-3"
          size="sm"
          disabled={isBusy}
          onClick={() =>
            run(
              "ff",
              () => unwrap(commands.repoUpdateNow(r.id, "pull_ff_only")),
              `Fast-forwarded ${r.localName}`,
              "Advanced to match origin.",
            )
          }
        >
          <ArrowDownToLine className={busy === "ff" ? "animate-spin" : undefined} /> Fast-forward now
        </Button>
      </>
    );
  }

  if (status === "dirty") {
    return (
      <>
        <div className={cn("text-sm font-bold", style.text)}>Uncommitted local changes</div>
        <p className="mt-0.5 text-xs text-foreground/80">
          RepoSync will not pull over a dirty working tree. Commit, stash, or discard your changes, then
          check again.
        </p>
      </>
    );
  }

  if (status === "failed") {
    return (
      <>
        <div className={cn("text-sm font-bold", style.text)}>Last check failed</div>
        <p className="mt-0.5 font-mono text-xs text-foreground/80">{r.lastErrorCode ?? "unknown error"}</p>
        <Button
          className="mt-3"
          variant="outline"
          size="sm"
          disabled={isBusy}
          onClick={onCheckNow}
        >
          {/*
            "check", not "retry". Both this button and the header's "Check now"
            run the same handler, which sets the busy key to "check"; the two
            differ only in where they sit and what they say. Left as "retry" the
            spinner would simply never appear, which is the least visible kind of
            regression: the click works, the state updates, and the only thing
            missing is the feedback that it is running.
          */}
          <RefreshCw className={busy === "check" ? "animate-spin" : undefined} /> Retry check
        </Button>
      </>
    );
  }

  if (status === "paused") {
    return (
      <>
        <div className={cn("text-sm font-bold", style.text)}>
          {r.autoPaused ? "Auto-paused after repeated failures" : "Watching paused"}
        </div>
        <p className="mt-0.5 text-xs text-foreground/80">
          This repo is not being checked on a schedule. Resume to fold it back into the rotation.
        </p>
        <Button
          className="mt-3"
          size="sm"
          disabled={isBusy}
          onClick={() => run("resume", () => unwrap(commands.repoSetEnabled(r.id, true)), `Resumed ${r.localName}`)}
        >
          <Play /> Resume watching
        </Button>
      </>
    );
  }

  return (
    <>
      <div className={cn("text-sm font-bold", style.text)}>
        {status === "ahead" ? `${r.aheadCount ?? 0} ahead of origin, clean` : "Up to date with origin"}
      </div>
      <p className="mt-0.5 text-xs text-foreground/80">
        {status === "ahead"
          ? "You have local commits not yet pushed. RepoSync leaves pushing to you."
          : "Nothing to do. RepoSync keeps watching on schedule."}
      </p>
    </>
  );
}

/**
 * Sync state (E-17): the old `IntelSection` ("Branch & PR intelligence") folds
 * in here rather than staying its own labelled section, per the ratified N4
 * shape. Everything this panel knows about where the repo stands relative to
 * its upstream and its GitHub remote, in one block: ahead/behind vs upstream,
 * branch identity, last-commit recency (always available for any git remote),
 * and the open pull-request counts (GitHub-only). Degrades gracefully:
 *   - a non-GitHub remote shows local intel and "unavailable" for PRs;
 *   - an un-refreshed or private-inaccessible repo shows "not yet checked", NEVER a
 *     fabricated "0 PRs" (E-17 AC5);
 *   - offline / rate-limited keeps the last-known counts and shows an "as of <time>"
 *     staleness marker from prLastCheckedAt (E-17 AC8);
 *   - a missing upstream reports "No upstream" rather than a fabricated 0 ahead/behind.
 *
 * Deliberate consolidation into ONE block (some earlier notes named two
 * destinations, "Sync state and Remote"): Branch/Head/Upstream moved here
 * from the old "Where it lives" list, since they are sync-state facts, not
 * where-it-physically-lives facts. Flagged in the PR for veto.
 */
function SyncStateSection({ r }: { r: RepoDetailData }) {
  const isGithub = r.hostType === "github";
  const aheadBehind =
    r.aheadCount === null && r.behindCount === null
      ? "No upstream"
      : `${r.aheadCount ?? 0} ahead · ${r.behindCount ?? 0} behind`;
  const prValue = !isGithub
    ? "Unavailable (non-GitHub remote)"
    : r.openPrCount === null
      ? "Not yet checked"
      : r.defaultBranchPrCount !== null && r.defaultBranchPrCount > 0
        ? `${r.openPrCount} open · ${r.defaultBranchPrCount} to ${r.defaultBranch ?? "default"}`
        : `${r.openPrCount} open`;

  return (
    <section>
      <SectionLabel icon={GitBranch}>Sync state</SectionLabel>
      <dl className="overflow-hidden rounded-md border border-border">
        <KvRow label="Ahead / behind" icon={GitCompare} value={aheadBehind} />
        <KvRow label="Branch" icon={GitBranch} value={r.activeBranch ?? r.defaultBranch ?? "unknown"} mono />
        <KvRow label="Head" icon={Hash} value={r.headSha ? r.headSha.slice(0, 10) : "unknown"} mono />
        <KvRow label="Upstream" icon={Link2} value={r.upstreamBranch ?? "none"} mono />
        <KvRow label="Last commit" icon={History} value={relativeTime(r.lastLocalCommitAt)} />
        <KvRow label="Open PRs" icon={GitPullRequest} value={prValue} />
        {isGithub && r.openPrCount !== null && (
          <KvRow label="PRs as of" icon={Clock} value={relativeTime(r.prLastCheckedAt)} />
        )}
        <KvRow label="Consecutive failures" icon={AlertTriangle} value={String(r.consecutiveFailures)} />
      </dl>
    </section>
  );
}

function SectionLabel({ icon: Icon, children }: { icon?: LucideIcon; children: ReactNode }) {
  return (
    <div className="mb-1.5 flex items-center gap-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
      {Icon && <Icon aria-hidden className="size-3.5 shrink-0" />}
      {children}
    </div>
  );
}

/** The small colour dot used by both group pills and the add-to-group candidates. */
function GroupDot({ color }: { color: string | null }) {
  return (
    <span
      className={cn("size-2 shrink-0 rounded-full", color === null && "bg-muted-foreground/50")}
      style={color ? { backgroundColor: color } : undefined}
    />
  );
}

function GroupPill({
  group,
  busy,
  onRemove,
}: {
  group: GroupSummary;
  busy: boolean;
  onRemove: () => void;
}) {
  return (
    <span className="inline-flex items-center gap-1 rounded-full border border-border py-0.5 pr-1 pl-2 text-xs font-medium">
      <GroupDot color={group.color} />
      {group.name}
      <button
        type="button"
        aria-label={`Remove from ${group.name}`}
        disabled={busy}
        onClick={onRemove}
        className="rounded-full p-0.5 text-muted-foreground hover:bg-muted hover:text-foreground disabled:opacity-50"
      >
        <X className="size-3" />
      </button>
    </span>
  );
}

/**
 * Member-only group pills on the third header line (G2, ui-delivery-plan.md
 * ledger B7): only groups this repo actually belongs to render as pills, each
 * removable in place, plus one "+ Add" affordance that must be able to reach
 * EVERY non-member group (the matrix names losing this as the exact risk of
 * a naive pills-only rebuild).
 *
 * No floating menu: this codebase has no popover primitive and Radix is
 * skipped for N4 (D2), so "+ Add" expands an INLINE disclosure listing every
 * non-member group as its own button, rather than an absolutely positioned
 * dropdown. Keeps every control in the normal tab order and inside the
 * drawer's existing focus trap with no extra plumbing.
 */
function GroupPills({
  groups,
  memberIds,
  groupBusyId,
  onToggleGroup,
}: {
  groups: GroupSummary[];
  memberIds: number[];
  groupBusyId: number | null;
  onToggleGroup: (group: GroupSummary, isMember: boolean) => void;
}) {
  const [addOpen, setAddOpen] = useState(false);

  if (groups.length === 0) {
    return (
      <p className="text-xs text-muted-foreground">
        No groups yet. Create one from the sidebar to organize this repo.
      </p>
    );
  }

  const memberGroups = groups.filter((g) => memberIds.includes(g.id));
  const nonMemberGroups = groups.filter((g) => !memberIds.includes(g.id));

  return (
    <div className="flex flex-col gap-1.5">
      <div className="flex flex-wrap items-center gap-1.5">
        {memberGroups.map((g) => (
          <GroupPill
            key={g.id}
            group={g}
            busy={groupBusyId === g.id}
            onRemove={() => onToggleGroup(g, true)}
          />
        ))}
        {nonMemberGroups.length > 0 && (
          <button
            type="button"
            aria-expanded={addOpen}
            aria-label="Add this repo to a group"
            onClick={() => setAddOpen((v) => !v)}
            className="inline-flex items-center gap-1 rounded-full border border-dashed border-border px-2 py-0.5 text-xs font-medium text-muted-foreground hover:bg-muted"
          >
            <Plus className="size-3" /> Add
          </button>
        )}
      </div>
      {addOpen && nonMemberGroups.length > 0 && (
        <div className="flex flex-wrap items-center gap-1.5 rounded-md border border-dashed border-border p-1.5">
          {nonMemberGroups.map((g) => (
            <button
              key={g.id}
              type="button"
              disabled={groupBusyId === g.id}
              onClick={() => onToggleGroup(g, false)}
              className="inline-flex items-center gap-1 rounded-full border border-border px-2 py-0.5 text-xs font-medium text-muted-foreground hover:bg-muted disabled:opacity-50"
            >
              <GroupDot color={g.color} /> {g.name}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/**
 * Per-repo check cadence (BL-NI-30). Two mutually exclusive options in the same
 * always-visible idiom as the update-policy cards: "Inherit global" (writes
 * `checkFrequencyMin = 0`, the inherit sentinel) and "Custom interval" (a positive
 * per-repo override in minutes). The effective cadence is shown outright so the
 * consequence of each choice is never hidden.
 */
function CadenceSection({
  r,
  globalMinutes,
  busy,
  run,
}: {
  r: RepoDetailData;
  globalMinutes: number | null;
  busy: string | null;
  run: RunFn;
}) {
  const isBusy = busy !== null;
  const isInherit = r.checkFrequencyMin === 0;
  const effective = isInherit ? globalMinutes : r.checkFrequencyMin;
  // Draft minutes for the Custom option, seeded from the current override (or the
  // global default while inheriting, so the field starts on a sensible value). The
  // section is keyed on the repo id at the call site, so switching repos reseeds it.
  const [draft, setDraft] = useState<number>(
    r.checkFrequencyMin > 0 ? r.checkFrequencyMin : (globalMinutes ?? 360),
  );
  const draftInt = Math.trunc(draft);
  const draftValid = Number.isFinite(draft) && draftInt >= 1;

  return (
    <section>
      <SectionLabel icon={Clock}>Check cadence</SectionLabel>
      <div className="flex flex-col gap-1.5">
        <button
          type="button"
          disabled={isBusy || isInherit}
          onClick={() =>
            run("cadence", () => unwrap(commands.repoSetCadence(r.id, 0)), "Cadence set to inherit global")
          }
          className={cn(
            "flex items-start gap-3 rounded-md border px-3 py-2 text-left transition-colors disabled:cursor-not-allowed",
            isInherit ? "border-primary bg-primary/10" : "border-border hover:bg-muted",
          )}
        >
          <CadenceRadio active={isInherit} />
          <span className="min-w-0">
            <span className="block text-sm font-medium">Inherit global</span>
            <span className="block text-xs text-muted-foreground">
              {globalMinutes !== null
                ? `Follows the global cadence (every ${globalMinutes} min). Changing the global setting re-cadences this repo.`
                : "Follows the global cadence. Changing the global setting re-cadences this repo."}
            </span>
          </span>
        </button>

        <div
          className={cn(
            "flex flex-col gap-2 rounded-md border px-3 py-2 transition-colors",
            !isInherit ? "border-primary bg-primary/10" : "border-border",
          )}
        >
          <div className="flex items-start gap-3">
            <CadenceRadio active={!isInherit} />
            <span className="min-w-0">
              <span className="block text-sm font-medium">Custom interval</span>
              <span className="block text-xs text-muted-foreground">
                Override the global default for just this repo.
              </span>
            </span>
          </div>
          <div className="flex items-center gap-2 pl-7">
            <Input
              type="number"
              min={1}
              className="w-24 text-right"
              value={draft}
              onChange={(e) => setDraft(Number(e.target.value))}
            />
            <span className="text-xs text-muted-foreground">min</span>
            <Button
              size="sm"
              variant="outline"
              className="ml-auto"
              disabled={isBusy || !draftValid || draftInt === r.checkFrequencyMin}
              onClick={() =>
                run(
                  "cadence",
                  () => unwrap(commands.repoSetCadence(r.id, draftInt)),
                  `Cadence set to every ${draftInt} min`,
                )
              }
            >
              Apply
            </Button>
          </div>
        </div>
      </div>
      <p className="mt-2 text-xs text-foreground/80">
        Effective cadence: {effective !== null ? `every ${effective} min` : "loading..."}
      </p>
    </section>
  );
}

/** The small filled/empty radio dot used by the cadence option cards. */
function CadenceRadio({ active }: { active: boolean }) {
  return (
    <span
      className={cn(
        "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border",
        active ? "border-primary" : "border-muted-foreground/40",
      )}
    >
      {active && <span className="size-2 rounded-full bg-primary" />}
    </span>
  );
}

/**
 * A key-value row (ratified N4 key-value treatment): a hairline under every
 * pair, an icon per label, and the VALUE LEFT-ALIGNED - replacing the old
 * right-aligned, single-line-truncated-with-a-tooltip shape. `breakAll` opts
 * a value (a path or a URL) into wrapping instead of ever needing that
 * tooltip in the first place.
 */
function KvRow({
  label,
  value,
  icon: Icon,
  mono,
  breakAll,
}: {
  label: string;
  value: ReactNode;
  icon?: LucideIcon;
  mono?: boolean;
  breakAll?: boolean;
}) {
  return (
    <div className="flex items-start gap-3 border-b border-border px-3 py-2 last:border-b-0">
      <dt className="flex w-32 shrink-0 items-center gap-1.5 pt-px text-xs text-muted-foreground">
        {Icon && <Icon aria-hidden className="size-3.5 shrink-0" />}
        {label}
      </dt>
      <dd className={cn("min-w-0 flex-1 text-left text-xs", mono && "font-mono", breakAll && "break-all")}>
        {value}
      </dd>
    </div>
  );
}

function OpenButton({
  label,
  icon: Icon,
  disabled,
  title,
  onClick,
}: {
  label: string;
  icon: typeof FolderOpen;
  disabled: boolean;
  /** Why the button is disabled, when it is. Omitted when it is not. */
  title?: string;
  onClick: () => void;
}) {
  return (
    <Button variant="secondary" size="sm" disabled={disabled} title={title} onClick={onClick}>
      <Icon /> {label}
    </Button>
  );
}

function PolicyOption({
  opt,
  current,
  disabled,
  onSelect,
}: {
  opt: (typeof POLICY_OPTIONS)[number];
  current: string;
  disabled: boolean;
  onSelect: () => void;
}) {
  const active = opt.mode === current;
  return (
    <button
      type="button"
      disabled={disabled || opt.disabled || active}
      onClick={onSelect}
      className={cn(
        "flex items-start gap-3 rounded-md border px-3 py-2 text-left transition-colors disabled:cursor-not-allowed",
        active ? "border-primary bg-primary/10" : "border-border hover:bg-muted",
        opt.disabled && "opacity-50",
      )}
    >
      <span
        className={cn(
          "mt-0.5 grid size-4 shrink-0 place-items-center rounded-full border",
          active ? "border-primary" : "border-muted-foreground/40",
        )}
      >
        {active && <span className="size-2 rounded-full bg-primary" />}
      </span>
      <span className="min-w-0">
        <span className="block text-sm font-medium">{opt.label}</span>
        <span className="block text-xs text-muted-foreground">{opt.blurb}</span>
      </span>
    </button>
  );
}

/**
 * The panel's Activity tab (NEW, ratified N4 shape): this repo's own recent
 * activity, scoped server-side by `repoId` through the existing `activityList`
 * binding, honoring the same truncation-sentinel contract as the Activity
 * screen (`lib/activity.ts`'s `paginate`/`ACTIVITY_PAGE_LIMIT`). Per the
 * ratified round-five corrections: no filter controls (this list is already
 * scoped to one repository), a short block title, dates that never wrap.
 *
 * Simple rows rather than the `DataTable` primitive: this call is the task's
 * own to make, flagged for veto. A five-column, scroll-owning, sticky-header
 * table felt like the wrong tool inside a region that is already a single
 * scrolling tab panel a few hundred pixels tall - the primitive's own ceremony
 * (frozen columns, a header rowgroup, its own internal scrollbar) solves
 * problems this list does not have.
 */
function ActivitySection({
  repoName,
  state,
}: {
  repoName: string;
  state: AsyncState<ActivityRecord[]>;
}) {
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const rows = state.data ?? [];
  const selected = rows.find((row) => row.id === selectedId) ?? null;

  return (
    <section>
      <SectionLabel icon={History}>Recent activity</SectionLabel>
      <AsyncPanel
        state={state}
        emptyWhen={(r) => r.length === 0}
        emptyMessage="No activity yet for this repository."
      >
        {(allRows) => {
          const { visible, hasMore } = paginate(allRows);
          return (
            <>
              <div className="overflow-hidden rounded-md border border-border">
                {visible.map((row) => (
                  <ActivityRow key={row.id} row={row} onClick={() => setSelectedId(row.id)} />
                ))}
              </div>
              {hasMore && (
                <p className="mt-2 text-xs text-muted-foreground">
                  Showing the {ACTIVITY_PAGE_LIMIT} most recent entries. There are older ones; they are
                  kept and are in the log, but are not listed here yet.
                </p>
              )}
            </>
          );
        }}
      </AsyncPanel>

      {/*
        A nested drawer (the same shared `Drawer` primitive the Activity screen
        uses for this exact component) is a new composition for this codebase:
        two focus-trapped modals mounted at once. The wrapper's `onKeyDown`
        stops propagation while the receipt is open so a Tab press does not
        also reach the OUTER drawer's own trap, which would otherwise recompute
        its first/last focusable using `querySelectorAll` (which cannot tell
        the inner drawer's controls are the "current" modal) and steal focus
        back. Escape already stops propagation inside `useModalA11y` itself,
        so only Tab needed this extra stop here.
      */}
      <div onKeyDown={(e) => selected !== null && e.stopPropagation()}>
        <Drawer
          open={selected !== null}
          onClose={() => setSelectedId(null)}
          aria-labelledby={ACTIVITY_RECEIPT_TITLE_ID}
        >
          {selected !== null && (
            <ActivityReceipt record={selected} repoName={repoName} onClose={() => setSelectedId(null)} />
          )}
        </Drawer>
      </div>
    </section>
  );
}

/**
 * One row of the panel's Activity tab. Deliberately duplicates the outcome
 * chip's shape from `activity.tsx`'s `OutcomeChip` (itself already a
 * deliberate duplicate of `activity-receipt.tsx`'s `StatusChip`, "so list and
 * receipt cannot drift") rather than importing a third screen's private
 * component - this file's own established idiom for the same reason.
 */
function ActivityRow({ row, onClick }: { row: ActivityRecord; onClick: () => void }) {
  const bad = row.status === "failed" || row.status === "error";
  const Icon = bad ? XCircle : CheckCircle2;
  return (
    <button
      type="button"
      onClick={onClick}
      aria-haspopup="dialog"
      className="flex w-full items-center gap-3 border-b border-border px-3 py-2 text-left last:border-b-0 hover:bg-muted"
    >
      <span className="w-16 shrink-0 font-mono text-xs whitespace-nowrap text-muted-foreground">
        {relativeTime(row.timestamp)}
      </span>
      <span className="inline-flex w-fit shrink-0 rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] font-semibold">
        {row.actionType}
      </span>
      <span
        className={cn(
          "inline-flex w-fit shrink-0 items-center gap-1 rounded-md px-2 py-0.5 font-mono text-[11px] font-semibold",
          bad ? "bg-status-failed/10 text-status-failed" : "bg-status-sync/10 text-status-sync",
        )}
      >
        <Icon aria-hidden className="size-3" />
        {row.status}
      </span>
      <span className="min-w-0 flex-1 truncate text-sm text-foreground/90">{row.summary ?? "-"}</span>
      <ChevronRight aria-hidden className="size-4 shrink-0 text-muted-foreground" />
    </button>
  );
}

/**
 * Removal (BL-NI-85). Two-step inline confirm in the same idiom as the group
 * delete in `groups-nav.tsx`: the first click only arms the confirm, so a stray
 * click can never clear history. The consequence copy stays visible BEFORE the
 * first click, not behind it, because this is the one action in the drawer that
 * cannot be undone. Keyed on the repo id at the call site so switching repos
 * disarms a half-armed confirm.
 *
 * Icon (ledger B14, ratified 08-27 log decision 14): a `Ban` no-entry mark,
 * never a trash can, because nothing on disk is deleted - only RepoSync's own
 * tracking data. Verified during the N7 consistency sweep that this section
 * had shipped with `Trash2` (a real trash can) on all three of its icons,
 * contradicting the ratification; fixed here rather than left as a silent gap.
 * `groups-nav.tsx`'s own delete keeps `Trash2` deliberately - that action
 * really does delete the group record, so the same idiom does not apply.
 */
function RemoveSection({
  name,
  busy,
  onRemove,
}: {
  name: string;
  busy: string | null;
  onRemove: () => void;
}) {
  const [confirming, setConfirming] = useState(false);
  const removing = busy === "remove";
  const isBusy = busy !== null;
  const armButton = useRef<HTMLButtonElement>(null);
  const confirmButton = useRef<HTMLButtonElement>(null);
  // Arming swaps the focused trigger out of the DOM, which would drop keyboard
  // focus to the body and out of the drawer's focus trap; follow the swap in
  // both directions. `armed` keeps mount from stealing focus when the drawer
  // opens with the section in its resting state.
  const armed = useRef(false);
  useEffect(() => {
    if (confirming) {
      armed.current = true;
      confirmButton.current?.focus();
    } else if (armed.current) {
      armed.current = false;
      armButton.current?.focus();
    }
  }, [confirming]);

  return (
    <section>
      <SectionLabel icon={Ban}>Remove</SectionLabel>
      <div className="rounded-md border border-border px-3 py-2.5">
        <p className="text-xs text-foreground/80">
          Stops tracking <span className="font-semibold">{name}</span> and deletes its RepoSync
          data: check history, group assignments, notes, and its policy and cadence settings. The
          folder and its git repo on disk are not touched.
        </p>
        {confirming ? (
          <div className="mt-2.5 flex flex-wrap items-center gap-2">
            <span role="alert" className="text-xs font-medium text-status-failed">
              Remove {name}? This cannot be undone.
            </span>
            <Button
              ref={confirmButton}
              variant="outline"
              size="sm"
              className="border-status-failed/40 text-status-failed hover:bg-status-failed/15"
              disabled={isBusy}
              onClick={onRemove}
            >
              <Ban className={removing ? "animate-pulse" : undefined} /> Remove
            </Button>
            <Button variant="ghost" size="sm" disabled={isBusy} onClick={() => setConfirming(false)}>
              Cancel
            </Button>
          </div>
        ) : (
          <Button
            ref={armButton}
            variant="outline"
            size="sm"
            className="mt-2.5 text-status-failed hover:bg-status-failed/15"
            disabled={isBusy}
            onClick={() => setConfirming(true)}
          >
            <Ban /> Remove from RepoSync
          </Button>
        )}
      </div>
    </section>
  );
}
