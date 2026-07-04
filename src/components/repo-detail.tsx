import { useCallback, useState } from "react";
import type { ReactNode } from "react";
import {
  ArrowDownToLine,
  ExternalLink,
  FolderOpen,
  GitBranch,
  Package,
  Pause,
  Pencil,
  Play,
  RefreshCw,
  Terminal,
  X,
} from "lucide-react";
import { commands } from "@/lib/bindings";
import type { GroupSummary, RepoDetail as RepoDetailData, UpdateMode } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Button } from "@/components/ui/button";
import { Switch } from "@/components/ui/switch";
import { AsyncPanel } from "@/components/async-panel";
import { StatusBadge } from "@/components/status-badge";
import { LagSignal } from "@/components/lag-signal";
import { useToast } from "@/hooks/use-toast";
import { useGroups, useGroupsForRepo, useRepoDetail } from "@/hooks/queries";
import {
  deriveStatus,
  lagLabel,
  lagMagnitude,
  relativeTime,
  STATUS_STYLE,
  type RepoStatus,
} from "@/lib/status";
import { cn } from "@/lib/utils";

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
  const toast = useToast();
  const [busy, setBusy] = useState<string | null>(null);
  const [groupBusyId, setGroupBusyId] = useState<number | null>(null);
  const refetch = detail.refetch;
  const refetchGroups = groupsState.refetch;
  const refetchMemberships = memberships.refetch;

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
        <span className="truncate font-mono text-sm font-semibold">
          {detail.data?.localName ?? "Repository"}
        </span>
        <Button variant="ghost" size="icon" className="ml-auto" onClick={onClose} aria-label="Close">
          <X />
        </Button>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <AsyncPanel state={detail}>
          {(r) => (
            <DetailBody
              r={r}
              busy={busy}
              run={run}
              groups={groupsState.data ?? []}
              memberIds={memberships.data ?? []}
              groupBusyId={groupBusyId}
              onToggleGroup={toggleGroup}
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
  groups,
  memberIds,
  groupBusyId,
  onToggleGroup,
}: {
  r: RepoDetailData;
  busy: string | null;
  run: RunFn;
  groups: GroupSummary[];
  memberIds: number[];
  groupBusyId: number | null;
  onToggleGroup: (group: GroupSummary, isMember: boolean) => void;
}) {
  const status = deriveStatus(r);
  const style = STATUS_STYLE[status];
  const isBusy = busy !== null;
  const badgeCount =
    status === "behind" ? (r.behindCount ?? 0) : status === "ahead" ? (r.aheadCount ?? 0) : undefined;

  return (
    <div className="flex flex-col gap-5 p-5">
      <div className="flex flex-col gap-2">
        <div className="flex items-center gap-2">
          <StatusBadge status={status} count={badgeCount} />
          {r.isArchived && (
            <span className="rounded bg-muted px-1.5 py-0.5 text-[10px] font-semibold uppercase text-muted-foreground">
              archived
            </span>
          )}
        </div>
        <div className="font-mono text-lg font-bold">{r.localName}</div>
        {r.description && <p className="text-sm text-muted-foreground">{r.description}</p>}
      </div>

      <div className={cn("rounded-lg border p-4", style.tint, FOCAL_BORDER[status])}>
        <Focal r={r} status={status} busy={busy} run={run} />
      </div>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="outline"
          size="sm"
          disabled={isBusy}
          onClick={() => run("check", () => unwrap(commands.repoCheckNow(r.id)), `Checked ${r.localName}`)}
        >
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

      <GroupsSection
        groups={groups}
        memberIds={memberIds}
        groupBusyId={groupBusyId}
        onToggleGroup={onToggleGroup}
      />

      <section>
        <SectionLabel>Open in</SectionLabel>
        <div className="flex flex-wrap gap-2">
          <OpenButton
            label="Folder"
            icon={FolderOpen}
            disabled={isBusy}
            onClick={() => run("folder", () => unwrap(commands.repoOpenFolder(r.id)), "Opened folder")}
          />
          <OpenButton
            label="Terminal"
            icon={Terminal}
            disabled={isBusy}
            onClick={() => run("terminal", () => unwrap(commands.repoOpenTerminal(r.id)), "Opened terminal")}
          />
          <OpenButton
            label="Editor"
            icon={Pencil}
            disabled={isBusy}
            onClick={() => run("editor", () => unwrap(commands.repoOpenEditor(r.id)), "Opened editor")}
          />
          {r.remoteOriginUrl && (
            <OpenButton
              label="Remote"
              icon={ExternalLink}
              disabled={isBusy}
              onClick={() => run("remote", () => unwrap(commands.repoOpenRemote(r.id)), "Opened remote")}
            />
          )}
        </div>
      </section>

      <section>
        <SectionLabel>Update policy</SectionLabel>
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

      {r.latestReleaseTag && (
        <section>
          <SectionLabel>Latest release</SectionLabel>
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
        <SectionLabel>Where it lives</SectionLabel>
        <dl className="overflow-hidden rounded-md border border-border">
          <KvRow label="Path" value={r.localPath} mono />
          <KvRow label="Remote" value={r.remoteOriginUrl ?? "none"} mono />
          <KvRow label="Branch" value={r.activeBranch ?? r.defaultBranch ?? "unknown"} mono />
          <KvRow label="Head" value={r.headSha ? r.headSha.slice(0, 10) : "unknown"} mono />
          <KvRow label="Upstream" value={r.upstreamBranch ?? "none"} mono />
          <KvRow label="Last checked" value={relativeTime(r.lastCheckedAt)} />
          <KvRow label="Last fetched" value={relativeTime(r.lastFetchedAt)} />
          <KvRow label="Consecutive failures" value={String(r.consecutiveFailures)} />
        </dl>
      </section>
    </div>
  );
}

function Focal({
  r,
  status,
  busy,
  run,
}: {
  r: RepoDetailData;
  status: RepoStatus;
  busy: string | null;
  run: RunFn;
}) {
  const style = STATUS_STYLE[status];
  const isBusy = busy !== null;

  if (status === "behind") {
    return (
      <>
        <div className={cn("text-sm font-bold", style.text)}>{r.behindCount ?? 0} commits behind origin</div>
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
          onClick={() => run("retry", () => unwrap(commands.repoCheckNow(r.id)), `Retried ${r.localName}`)}
        >
          <RefreshCw className={busy === "retry" ? "animate-spin" : undefined} /> Retry check
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

function SectionLabel({ children }: { children: ReactNode }) {
  return (
    <div className="mb-1.5 text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">
      {children}
    </div>
  );
}

function GroupsSection({
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
  return (
    <section>
      <SectionLabel>Groups</SectionLabel>
      {groups.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          No groups yet. Create one from the sidebar to organize this repo.
        </p>
      ) : (
        <div className="flex flex-col gap-1.5">
          {groups.map((g) => {
            const member = memberIds.includes(g.id);
            return (
              <div
                key={g.id}
                className="flex items-center gap-3 rounded-md border border-border px-3 py-2"
              >
                <span
                  className={cn(
                    "size-2.5 shrink-0 rounded-full",
                    g.color === null && "bg-muted-foreground/50",
                  )}
                  style={g.color ? { backgroundColor: g.color } : undefined}
                />
                <span className="min-w-0 flex-1 truncate text-sm font-medium">{g.name}</span>
                <span className="font-mono text-[11px] text-muted-foreground">{g.repoCount}</span>
                <Switch
                  checked={member}
                  disabled={groupBusyId === g.id}
                  onCheckedChange={() => onToggleGroup(g, member)}
                />
              </div>
            );
          })}
        </div>
      )}
    </section>
  );
}

function KvRow({ label, value, mono }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border px-3 py-2 last:border-b-0">
      <dt className="text-xs text-muted-foreground">{label}</dt>
      <dd className={cn("max-w-[62%] truncate text-xs", mono && "font-mono")} title={value}>
        {value}
      </dd>
    </div>
  );
}

function OpenButton({
  label,
  icon: Icon,
  disabled,
  onClick,
}: {
  label: string;
  icon: typeof FolderOpen;
  disabled: boolean;
  onClick: () => void;
}) {
  return (
    <Button variant="secondary" size="sm" disabled={disabled} onClick={onClick}>
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
