import { useCallback, useMemo, useState } from "react";
import {
  AlertTriangle,
  ArrowDown,
  Cloud,
  FolderGit2,
  GitPullRequest,
  HardDrive,
  Plus,
  RefreshCw,
  XCircle,
} from "lucide-react";
import type { GroupSummary, RepoSummary, SummaryItem } from "@/lib/bindings";
import { cn } from "@/lib/utils";
import { Button } from "@/components/ui/button";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { AsyncPanel } from "@/components/async-panel";
import { EmptyState, AllClearState } from "@/components/empty-state";
import { Drawer } from "@/components/ui/drawer";
import { RepoDetailPanel, REPO_DETAIL_TITLE_ID } from "@/components/repo-detail";
import { AddReposDialog } from "@/components/add-repos-dialog";
import { PageShell } from "@/components/page-shell";
import { useBackendEvents, useRepoGroupMemberships, useRepoList, useSummaryToday } from "@/hooks/queries";
import { checkFailureMessage, deriveStatus, STATUS_ICON, STATUS_STYLE } from "@/lib/status";

const ALL_FILTER = { enabledOnly: null, hostType: null, query: null };

export function DashboardScreen({
  onOpenRepos,
  activeGroupId,
  groups,
}: {
  onOpenRepos: () => void;
  /**
   * The sidebar's active group filter (`AppShell`, shared with Repos). Passed
   * in (N6, ui-delivery-plan.md ledger B4) so the four tiles can honestly
   * scope to it: see the group-scoping note above `scopedCount` below for
   * which numbers can and cannot follow it.
   */
  activeGroupId: number | null;
  groups: GroupSummary[];
}) {
  const repos = useRepoList(ALL_FILTER);
  const summary = useSummaryToday();
  // Bulk membership read (BL-NI-22's pattern, lifted from repos.tsx) - the
  // only frontend-only ingredient the group-scoping decision below needed.
  const memberships = useRepoGroupMemberships();

  const reposRefetch = repos.refetch;
  const summaryRefetch = summary.refetch;
  const membershipsRefetch = memberships.refetch;
  const refetch = useCallback(() => {
    reposRefetch();
    summaryRefetch();
    membershipsRefetch();
  }, [reposRefetch, summaryRefetch, membershipsRefetch]);
  useBackendEvents(refetch);

  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [addOpen, setAddOpen] = useState(false);

  const noRepos = repos.data !== null && repos.data.length === 0;

  const activeGroup = useMemo(
    () => (activeGroupId === null ? null : (groups.find((g) => g.id === activeGroupId) ?? null)),
    [activeGroupId, groups],
  );
  const membershipMap = memberships.data;
  // A group filter is set but the bulk membership read hasn't resolved (or
  // failed) yet. Every scoped number below must wait for this rather than
  // render a fabricated zero (finding 7 / BL-NI-22's sibling honesty rule,
  // repos.tsx's own `inGroupCount === null` guard) - see the render below.
  const membershipPending = activeGroupId !== null && membershipMap === null;

  const inActiveGroup = useCallback(
    (repoId: number) => activeGroupId === null || (membershipMap?.get(repoId)?.includes(activeGroupId) ?? false),
    [activeGroupId, membershipMap],
  );

  // Look up each attention item's live facts so its icon/color can follow the
  // repo's actual current status (finding 10 / BL-NI-27), rather than always
  // rendering the failed-red glyph. A miss (repo dropped out of the list
  // between fetches) falls back to the prior failed-red treatment.
  const repoById = useMemo(() => {
    const m = new Map<number, RepoSummary>();
    for (const r of repos.data ?? []) m.set(r.id, r);
    return m;
  }, [repos.data]);

  /**
   * GROUP SCOPING (N6, ui-delivery-plan.md ledger B4; M5's ratified "scoped
   * to the selected group" requirement).
   *
   * `summary_today` is one unscoped backend call with no group parameter
   * (`bindings.ts`'s `summaryToday: () => ...`, no argument) - it was never
   * designed to answer "for this group". Three of its four numbers are
   * neverthless honestly scopable WITHOUT a backend change, because
   * `DailySummary.updated` / `.newReleases` / `.attention` each carry a
   * `repoId` per item (`SummaryItem`), so this can INTERSECT the backend's
   * own item list against the membership map already fetched above, rather
   * than re-deriving the underlying business rule client-side. That keeps
   * one authority for "what counts as attention / updated / a new release"
   * (the backend query) while letting the DISPLAY narrow to the active
   * group.
   *
   * The fourth, `noChangeCount`, is a bare integer with no per-repo id list
   * anywhere in the wire type - there is nothing to intersect. Scoping it
   * would need a real backend change (either a `noChange: SummaryItem[]`
   * list or a `group_id` parameter on `summary_today`), which is out of
   * scope for this `src/`-only slice per the shell-crate chokepoint. It
   * stays a GLOBAL count everywhere, and the one place it is shown (the
   * "Under watch" hint) says so explicitly rather than silently sitting
   * under a scoped headline number - seeing "40 checked, no change" under a
   * headline of "5" (a 5-repo group) would read as incoherent otherwise.
   *
   * WORDING (Codex adversarial review of PR #79, finding 1, confirmed):
   * `noChangeCount` does NOT mean "currently in sync." Reading
   * `summary_today`'s `classify_row` (`summary.rs`): a successful "check" or
   * "fetch" action ALWAYS lands in the no-change bucket regardless of the
   * resulting ahead/behind counts, and a "skipped" activity row does too. A
   * repo that was fetched today, remains clean, but is now 40 commits
   * behind is counted here - it is not "in sync," it is behind and simply
   * was not touched today. The label below says only what is true:
   * "checked, no change," never "in sync." The same reasoning applies to
   * the All-clear callout beneath the tiles: the attention population
   * deliberately excludes a clean-but-behind repo
   * (`attention_excludes_a_repo_that_is_only_behind`), so an attention-free
   * group can still contain repos that are meaningfully behind, and the
   * callout must not claim "in sync" for them either - it states only the
   * backend's own predicate, "No dirty or failed repositories."
   *
   * SCOPE LABELING (Codex adversarial review of PR #79, finding 3,
   * confirmed): every scoped number below is keyed on `activeGroupId`, not
   * on `activeGroup` (the resolved `GroupSummary` object) - `activeGroup`
   * can be `null` while `activeGroupId` is still set, whenever `groups`
   * (an independent async read, `AppShell`'s `useGroups`) has not yet
   * caught up (still loading, or momentarily stale). Gating the scope
   * caption, the "(all repos)" qualifier, or the All-clear wording on
   * `activeGroup` instead would let the numbers stay scoped while every
   * visible indicator saying so silently vanished - scoped numbers with
   * unscoped labeling, exactly the class of defect this file exists to
   * avoid. Every label below therefore branches on `activeGroupId !== null`
   * first, and only prefers `activeGroup`'s name when it happens to be
   * resolved, falling back to a degraded but still honest "a group" /
   * "this group" phrasing otherwise.
   */
  const scopedCount = useCallback(
    (items: SummaryItem[]) => (activeGroupId === null ? items.length : items.filter((it) => inActiveGroup(it.repoId)).length),
    [activeGroupId, inActiveGroup],
  );

  const underWatchCount = useMemo(() => {
    if (repos.data === null) return null;
    return activeGroupId === null ? repos.data.length : repos.data.filter((r) => inActiveGroup(r.id)).length;
  }, [repos.data, activeGroupId, inActiveGroup]);

  return (
    <PageShell
      title="Dashboard"
      actions={
        <>
          <Button variant="outline" size="sm" onClick={refetch}>
            <RefreshCw /> Refresh
          </Button>
          <Button size="sm" onClick={() => setAddOpen(true)}>
            <Plus /> Add repos
          </Button>
        </>
      }
    >

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
          {(s) => {
            // See `membershipPending` above: a group filter is active but its
            // membership map has not resolved. Route through the membership
            // read's own loading/error presentation rather than render any
            // scoped number - repos.tsx's identical guard for the same reason.
            if (membershipPending) {
              return (
                <AsyncPanel state={memberships}>
                  {/* Unreachable: this only renders while membershipMap is
                      null, and AsyncPanel only calls children once
                      state.data is non-null (membershipPending would then
                      be false and this branch would not be taken). */}
                  {() => null}
                </AsyncPanel>
              );
            }

            const attentionItems = activeGroupId === null ? s.attention : s.attention.filter((it) => inActiveGroup(it.repoId));

            return (
              <div className="flex flex-col gap-3">
                {activeGroupId !== null && (
                  <div className="flex items-center gap-1.5 text-xs font-medium text-muted-foreground">
                    {activeGroup ? (
                      <>
                        <span
                          className={cn(
                            "size-2 shrink-0 rounded-full",
                            activeGroup.color === null && "bg-muted-foreground/50",
                          )}
                          style={activeGroup.color ? { backgroundColor: activeGroup.color } : undefined}
                        />
                        Scoped to {activeGroup.name}
                      </>
                    ) : (
                      // The group list has not caught up with `activeGroupId` yet
                      // (still loading, or momentarily stale) - the numbers below
                      // are still correctly scoped (they key on `activeGroupId`
                      // alone), so this degraded indicator says so rather than
                      // silently disappearing and leaving scoped numbers looking
                      // like global ones (Codex review finding 3, confirmed).
                      "Scoped to a group"
                    )}
                  </div>
                )}
                <div className="grid grid-cols-2 gap-3 md:grid-cols-4">
                  {/*
                    Only "Under watch" is wired: it is a plain navigation to
                    Repos, which - freshly mounted (Repos fully unmounts when
                    not the active view, so its own status chip resets to
                    "all") and sharing this same `activeGroupId` app state -
                    shows EXACTLY this tile's population, scoped or not. See
                    the PR body for why the other three tiles are NOT wired.
                  */}
                  <Tile
                    label="Under watch"
                    value={underWatchCount ?? "-"}
                    hint={
                      activeGroupId !== null
                        ? `${s.noChangeCount} checked, no change (all repos)`
                        : `${s.noChangeCount} checked, no change`
                    }
                    onClick={onOpenRepos}
                  />
                  {/*
                    The hint names the ACTUAL rule, which is `last_error_code
                    IS NOT NULL OR is_dirty = 1` (summary.rs). It previously
                    said "dirty, failed, behind" and behind was never part of
                    it.

                    Behind is deliberately excluded rather than missing. The
                    default `update_mode` is `fetch_only`, which updates
                    remote refs and never touches the working tree, so behind
                    is the designed steady state of a watched library rather
                    than an anomaly - folding it in here would make this
                    number permanently non-zero and stop it meaning "act on
                    this". Behind has its own violet badge on the Repos
                    screen, which is where a drifting repo is meant to be
                    read (and now, its own reason pill on the row below when
                    it rides alongside an actual attention cause).
                  */}
                  <Tile
                    label="Need attention"
                    value={attentionItems.length}
                    hint="dirty or failed"
                    alert={attentionItems.length > 0}
                  />
                  <Tile label="Updated today" value={scopedCount(s.updated)} hint="fast-forwarded, clean" />
                  <Tile label="New releases" value={scopedCount(s.newReleases)} hint="upstream tags" />
                </div>

                <Card>
                  <CardHeader>
                    <CardTitle>Needs attention</CardTitle>
                    <button onClick={onOpenRepos} className="ml-auto text-xs font-medium text-primary">
                      Open Repos
                    </button>
                  </CardHeader>
                  {attentionItems.length === 0 ? (
                    <CardContent>
                      <AllClearState
                        title="All clear"
                        description={
                          activeGroupId === null
                            ? "No dirty or failed repositories."
                            : activeGroup
                              ? `No dirty or failed repositories in ${activeGroup.name}.`
                              : "No dirty or failed repositories in this group."
                        }
                      />
                    </CardContent>
                  ) : (
                    <ul>
                      {attentionItems.map((item) => (
                        <AttentionRow
                          key={item.repoId}
                          item={item}
                          repo={repoById.get(item.repoId)}
                          onOpen={() => setSelectedId(item.repoId)}
                        />
                      ))}
                    </ul>
                  )}
                </Card>
              </div>
            );
          }}
        </AsyncPanel>
      )}

      <Drawer
        open={selectedId !== null}
        onClose={() => setSelectedId(null)}
        size="wide"
        aria-labelledby={REPO_DETAIL_TITLE_ID}
      >
        {selectedId !== null && (
          <RepoDetailPanel id={selectedId} onChanged={refetch} onClose={() => setSelectedId(null)} />
        )}
      </Drawer>

      <AddReposDialog open={addOpen} onClose={() => setAddOpen(false)} onAdded={refetch} />
    </PageShell>
  );
}

/**
 * The M5 "filter tile" shape (ratified 08-27, `_local/gui/2026-08-27_iterations/
 * 2026-08-27_10_metrics-variations.html`): a bordered card that is a `<button>`
 * with a hover state when it has an honest click destination, or a plain,
 * visually IDENTICAL `<div>` when it does not - never a button that looks
 * clickable and silently does nothing (or worse, lands somewhere that lies
 * about what the tile counts). See the per-tile wiring decisions in the PR
 * body.
 */
function Tile({
  label,
  value,
  hint,
  alert,
  onClick,
}: {
  label: string;
  value: number | string;
  hint: string;
  alert?: boolean;
  onClick?: () => void;
}) {
  const Wrapper = onClick ? "button" : "div";
  return (
    <Wrapper
      type={onClick ? "button" : undefined}
      onClick={onClick}
      className={cn(
        "rounded-xl border border-border bg-card p-4 text-left shadow-sm",
        onClick &&
          "cursor-pointer transition-colors hover:border-muted-foreground/50 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
      )}
    >
      <div className="text-[11px] font-semibold uppercase tracking-wider text-muted-foreground">{label}</div>
      <div className={cn("mt-1.5 font-mono text-3xl font-bold tracking-tight", alert && "text-status-failed")}>
        {value}
      </div>
      <div className="mt-1.5 text-xs text-muted-foreground">{hint}</div>
    </Wrapper>
  );
}

/**
 * One reason a row is showing. `category` is the row's PROVENANCE glyph
 * grouping (see `AttentionRow`) - "local" and "remote" get a `HardDrive`/
 * `Cloud` marker, "neutral" gets none, because a guessed provenance is worse
 * than admitting the code is not one this file has been taught (see
 * `classifyErrorCode`).
 */
interface Reason {
  category: "local" | "remote" | "neutral";
  icon: typeof AlertTriangle;
  label: string;
  tone: string;
  title?: string;
}

/**
 * Classify a `lastErrorCode` by provenance, from the STABLE WIRE CODE alone -
 * never guessed from context (Codex adversarial review of PR #79, finding 2,
 * confirmed). Verified against every write path that persists
 * `repo_local_state.last_error_code`:
 *
 * - REMOTE (network / fetch / auth): `git.auth_failed`, `net.offline`,
 *   `git.fetch_failed` - the only three codes a soft fetch failure can
 *   produce (`repo.rs`'s `check_now_inner`, step 5's `why` match) or the
 *   scheduler ever writes (`policy::status_error_code`, matched from
 *   `scheduler.rs`'s `DbOutcomeWriter::record`).
 * - LOCAL (filesystem / local git mechanics, never touches the network):
 *   `git.not_a_repo` and `git.not_found`, written by `check_now`'s
 *   `record_hard_failure_code` (`repo.rs`) when the working directory is
 *   gone, is no longer a valid git repository, or the git executable itself
 *   cannot be found. `check_now`'s own doc comment names three failure
 *   modes - "git missing, the path gone, the directory no longer a working
 *   tree" - and the last two collapse onto the single `git.not_a_repo` code:
 *   `git/inspect.rs`'s `inspect` maps EVERY `Repository::open` failure to
 *   `AppError::NotARepo` regardless of whether the path is missing or merely
 *   not a repository, so this file cannot (and does not need to) tell those
 *   two apart any further than the backend already does. Also local:
 *   `git.ff_not_possible` and `git.command_failed`, written by
 *   `update_now`'s fast-forward failure classifier (`repo.rs`, the
 *   `PullClass` match) for a local git-mechanics problem - a diverged branch
 *   or a non-fetch git invocation failure - that never touched the network
 *   either.
 * - Anything else renders NEUTRAL: a taxonomy entry this list has not been
 *   taught, or one that should never reach this field at all. No provenance
 *   glyph, rather than a guessed one.
 */
const REMOTE_ERROR_CODES = new Set(["git.auth_failed", "net.offline", "git.fetch_failed"]);
const LOCAL_ERROR_CODES = new Set(["git.not_a_repo", "git.not_found", "git.ff_not_possible", "git.command_failed"]);

function classifyErrorCode(code: string): "local" | "remote" | "neutral" {
  if (REMOTE_ERROR_CODES.has(code)) return "remote";
  if (LOCAL_ERROR_CODES.has(code)) return "local";
  return "neutral";
}

/**
 * Short human labels for the failure codes this row can show (mirrors
 * `checkFailureMessage`'s own switch, `lib/status.ts`, for the three remote
 * ones). The row needs a WORD, not a full sentence, which rides the `title`
 * tooltip instead - the same short-label-plus-tooltip idiom the Folder
 * column and Latest release section already use elsewhere in the app.
 */
function failureLabel(code: string): string {
  switch (code) {
    case "git.auth_failed":
      return "Auth failed";
    case "net.offline":
      return "Offline";
    case "git.not_a_repo":
      return "Not a repository";
    case "git.not_found":
      return "Git not found";
    case "git.ff_not_possible":
      return "Can't fast-forward";
    case "git.command_failed":
      return "Git command failed";
    default:
      return "Check failed";
  }
}

/**
 * Full-sentence tooltip for a LOCAL or NEUTRAL failure code. Deliberately
 * separate from `checkFailureMessage` (`lib/status.ts`), which is scoped by
 * its own doc comment to "the frozen operational set the backend produces on
 * a non-zero fetch" - reusing it here would tell the user "the fetch failed"
 * for a failure that never reached the network, the same class of defect
 * this function exists to avoid.
 */
function localFailureMessage(code: string): string {
  switch (code) {
    case "git.not_a_repo":
      return "RepoSync could not open this folder as a git repository. It may have been moved, renamed, or had its .git directory removed.";
    case "git.not_found":
      return "RepoSync could not find the git executable. Check the git executable path in Settings.";
    case "git.ff_not_possible":
      return "The local branch has diverged from its upstream and cannot fast-forward automatically. See the repo's detail panel.";
    case "git.command_failed":
      return "A local git command failed. See Activity for the exact command and output.";
    default:
      return "See Activity for details.";
  }
}

/**
 * WHY a repo is on the attention list (N6 Part B, coverage-matrix.md section
 * 5 REDESIGN #13; jp's 08-26 ask): a local cause (uncommitted changes), a
 * remote one (a failed fetch, being behind, open PRs), or - for a
 * `lastErrorCode` this file has not been taught, or an unresolved repo
 * lookup - a neutral one with no provenance glyph, rather than a guessed
 * one (`classifyErrorCode`). `is_dirty` and `last_error_code` are the same
 * `repo_local_state` columns the backend's own attention query reads
 * (`summary.rs`), so this reproduces exactly which cause(s) apply for a
 * repo already known to be on the list - it does not decide LIST
 * MEMBERSHIP, only narrates it.
 *
 * `behindCount` and `openPrCount` are not attention causes on their own (a
 * repo that is only behind is deliberately excluded, see `summary.rs`'s
 * `attention_excludes_a_repo_that_is_only_behind`), but they are relevant
 * REMOTE context once a repo is already on the list for another reason, so
 * they render as additional pills.
 */
function attentionReasons(repo: RepoSummary | undefined, fallbackDetail: string | null): Reason[] {
  const reasons: Reason[] = [];
  if (repo?.isDirty) {
    reasons.push({ category: "local", icon: AlertTriangle, label: "Uncommitted changes", tone: STATUS_STYLE.dirty.text });
  }
  if (repo?.lastErrorCode) {
    const code = repo.lastErrorCode;
    const category = classifyErrorCode(code);
    reasons.push({
      category,
      icon: XCircle,
      label: failureLabel(code),
      tone: STATUS_STYLE.failed.text,
      // `checkFailureMessage` is scoped to the three REMOTE codes only (its
      // own doc comment); a local or neutral code gets its own sentence
      // instead, never the fetch-specific one.
      title: category === "remote" ? checkFailureMessage(code) : localFailureMessage(code),
    });
  }
  // The repo dropped out of `repos.data` between fetches (the lookup
  // missed): fall back to the backend's own detail string rather than
  // showing nothing, but this is an UNRESOLVED lookup, not a known cause -
  // it renders neutral (no local/remote glyph) rather than guessing "remote"
  // the way this branch used to (Codex review finding 2, confirmed).
  if (reasons.length === 0 && fallbackDetail) {
    reasons.push({ category: "neutral", icon: XCircle, label: fallbackDetail, tone: STATUS_STYLE.failed.text });
  }
  if (repo && (repo.behindCount ?? 0) > 0) {
    reasons.push({
      category: "remote",
      icon: ArrowDown,
      label: `${repo.behindCount} behind`,
      tone: STATUS_STYLE.behind.text,
    });
  }
  if (repo && (repo.openPrCount ?? 0) > 0) {
    // E-17 AC9 (Status-Owns-Saturation): PR/release context renders in the
    // dedicated magenta `status-release` token, never a taxonomy status
    // colour, so it can never be mistaken for sync status.
    reasons.push({
      category: "remote",
      icon: GitPullRequest,
      label: `${repo.openPrCount} open PR${repo.openPrCount === 1 ? "" : "s"}`,
      tone: "text-status-release",
    });
  }
  return reasons;
}

function ReasonPill({ icon: Icon, label, tone, title }: Reason) {
  return (
    <span className={cn("inline-flex items-center gap-1 text-xs font-medium", tone)} title={title}>
      <Icon aria-hidden className="size-3 shrink-0" />
      {label}
    </span>
  );
}

function AttentionRow({
  item,
  repo,
  onOpen,
}: {
  item: SummaryItem;
  repo: RepoSummary | undefined;
  onOpen: () => void;
}) {
  const status = repo ? deriveStatus(repo) : "failed";
  const Icon = STATUS_ICON[status];
  const reasons = attentionReasons(repo, item.detail);
  const local = reasons.filter((r) => r.category === "local");
  const remote = reasons.filter((r) => r.category === "remote");
  const neutral = reasons.filter((r) => r.category === "neutral");

  return (
    <li>
      <button
        type="button"
        onClick={onOpen}
        className="flex w-full items-start gap-3 border-b border-border px-4 py-3 text-left last:border-b-0 hover:bg-muted/40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset"
      >
        <Icon className={cn("mt-0.5 size-4 shrink-0", STATUS_STYLE[status].text)} />
        <div className="min-w-0 flex-1">
          <div className="truncate font-mono text-sm font-semibold">{item.localName}</div>
          {reasons.length > 0 && (
            <div className="mt-1 flex flex-wrap items-center gap-x-4 gap-y-1">
              {/*
                Two glyphs mark PROVENANCE (local disk versus the remote), each
                shown once per row and only when a reason of that kind is
                present; the reasons themselves stay in the established
                status-coloured icon+word vocabulary (`STATUS_ICON`,
                `lib/status.ts` section 9's "one lucide icon per state"
                constraint - HardDrive/Cloud are never substituted for it, only
                added alongside it). A NEUTRAL reason (an error code this file
                has not been taught, or an unresolved repo lookup) gets
                NEITHER glyph - a guessed provenance is worse than admitting
                the code is unclassified (Codex review finding 2, confirmed).
              */}
              {local.length > 0 && (
                <span className="inline-flex items-center gap-1.5">
                  <HardDrive aria-hidden className="size-3 shrink-0 text-muted-foreground" />
                  {local.map((r) => (
                    <ReasonPill key={r.label} {...r} />
                  ))}
                </span>
              )}
              {remote.length > 0 && (
                <span className="inline-flex items-center gap-1.5">
                  <Cloud aria-hidden className="size-3 shrink-0 text-muted-foreground" />
                  {remote.map((r) => (
                    <ReasonPill key={r.label} {...r} />
                  ))}
                </span>
              )}
              {neutral.length > 0 && (
                <span className="inline-flex items-center gap-1.5">
                  {neutral.map((r) => (
                    <ReasonPill key={r.label} {...r} />
                  ))}
                </span>
              )}
            </div>
          )}
        </div>
      </button>
    </li>
  );
}
