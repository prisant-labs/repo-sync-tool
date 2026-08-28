import { AlertTriangle, ArrowDown, ArrowUp, Check, PauseCircle, Unlink, XCircle } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { RepoSummary } from "@/lib/bindings";

/** The 7-state taxonomy, derived on the frontend from raw RepoSummary facts. */
export type RepoStatus =
  | "sync"
  | "ahead"
  | "behind"
  | "dirty"
  | "failed"
  | "paused"
  | "noUpstream";

type StatusFacts = Pick<
  RepoSummary,
  | "isDirty"
  | "enabled"
  | "autoPaused"
  | "lastErrorCode"
  | "aheadCount"
  | "behindCount"
  | "upstreamState"
>;

/**
 * Priority order: paused > failed > dirty > noUpstream > behind > ahead > sync.
 *
 * The wire type carries only raw facts (no `status` field), so this ranking is
 * a frontend policy decision. Re-ranking (e.g. "dirty-and-behind reads as
 * behind") is a one-line change here, never a backend migration.
 *
 * `noUpstream` sits ABOVE behind and ahead deliberately. Those two describe a
 * comparison against an upstream, and when there is no upstream to compare
 * against the backend reports both counts as null, so they cannot fire anyway.
 * Ranking it above them says the same thing in the code rather than relying on
 * a null to keep the lower branches quiet.
 *
 * It sits BELOW dirty and failed because those are things to act on now, and
 * BL-NI-77 was deliberately scoped to stop the badge making a false statement
 * rather than to escalate the repo.
 */
export function deriveStatus(r: StatusFacts): RepoStatus {
  if (!r.enabled || r.autoPaused) return "paused";
  if (r.lastErrorCode) return "failed";
  if (r.isDirty) return "dirty";
  // Only `deleted` and `none` mean there is nothing to sync with. `tracking` is
  // healthy.
  //
  // `null` means NOT YET OBSERVED: a row older than migration 0008 that has not
  // been checked since. It falls through, which on a clean repo lands on "sync".
  // Being straight about that, since it is the reassuring answer this state was
  // added to stop assuming: falling through is not neutral, it is the
  // pre-BL-NI-77 behaviour, chosen because the alternative is worse. Claiming
  // "no upstream" from an absent observation would badge EVERY repo as broken
  // the moment the migration lands and before anything has been re-checked. The
  // window is one check per repo and then it is gone, which a wrong badge on the
  // whole library is not.
  if (r.upstreamState === "deleted" || r.upstreamState === "none") return "noUpstream";
  if ((r.behindCount ?? 0) > 0) return "behind";
  if ((r.aheadCount ?? 0) > 0) return "ahead";
  return "sync";
}

/**
 * One lucide icon per state, so status survives grayscale and color
 * blindness. Shared by `StatusBadge` and any other surface (e.g. the
 * dashboard's "Needs attention" rows) that renders a status without going
 * through `StatusBadge` itself.
 */
export const STATUS_ICON: Record<RepoStatus, LucideIcon> = {
  sync: Check,
  ahead: ArrowUp,
  behind: ArrowDown,
  dirty: AlertTriangle,
  failed: XCircle,
  paused: PauseCircle,
  noUpstream: Unlink,
};

/**
 * Per-status presentation. Class strings are written out in full (never
 * interpolated) so Tailwind's scanner can see them at build time.
 *
 * `chip` is the ratified 4B treatment: status is a FILLED rectangle, so it
 * needs a background and an ink of its own. It is NOT `text` on `tint`. Those
 * two were tuned for different jobs - `text` sits on a neutral surface and is
 * dark enough to be unreadable as a fill, and `tint` is a 12% wash meant to
 * colour a large focal panel, which at chip size reads as a smudge rather than
 * a chip. Every `chip` pair is solved so its ink clears 6.7:1 on its own tint.
 *
 * `text`, `bar` and `tint` all stay: the Focal headline, the lag bar and the
 * Focal panel fill still use them, and none of those is a chip.
 */
export const STATUS_STYLE: Record<
  RepoStatus,
  { label: string; text: string; bar: string; tint: string; chip: string }
> = {
  sync: { label: "In sync", text: "text-status-sync", bar: "bg-status-sync", tint: "bg-status-sync/12", chip: "bg-status-sync-tint text-status-sync-ink" },
  ahead: { label: "Ahead", text: "text-status-sync", bar: "bg-status-sync", tint: "bg-status-sync/12", chip: "bg-status-ahead-tint text-status-ahead-ink" },
  behind: { label: "Behind", text: "text-status-behind", bar: "bg-status-behind", tint: "bg-status-behind/12", chip: "bg-status-behind-tint text-status-behind-ink" },
  dirty: { label: "Dirty", text: "text-status-dirty", bar: "bg-status-dirty", tint: "bg-status-dirty/12", chip: "bg-status-dirty-tint text-status-dirty-ink" },
  failed: { label: "Failed", text: "text-status-failed", bar: "bg-status-failed", tint: "bg-status-failed/12", chip: "bg-status-failed-tint text-status-failed-ink" },
  paused: { label: "Paused", text: "text-status-paused", bar: "bg-status-paused", tint: "bg-status-paused/12", chip: "bg-status-paused-tint text-status-paused-ink" },
  // Its own token rather than reusing `paused`'s near-neutral. The two look
  // similar in weight on purpose (neither should shout), but they mean opposite
  // things about who is responsible: paused is "you turned this off", noUpstream
  // is "this cannot run and you probably did not know". Sharing a color would
  // invite reading the second as the first and moving on.
  noUpstream: {
    label: "No upstream",
    text: "text-status-no-upstream",
    bar: "bg-status-no-upstream",
    tint: "bg-status-no-upstream/12",
    chip: "bg-status-no-upstream-tint text-status-no-upstream-ink",
  },
};

/** A human "behind by N" style lag label from the raw counts + derived status. */
export function lagLabel(r: StatusFacts): string {
  const status = deriveStatus(r);
  if (status === "behind") return `${r.behindCount ?? 0} behind`;
  if (status === "ahead") return `${r.aheadCount ?? 0} ahead, clean`;
  if (status === "dirty") return "uncommitted, skipped";
  if (status === "failed") return "check failed";
  if (status === "paused") return "watching paused";
  // Names the cause, not the symptom. "current" would be the literal reading of
  // ahead=0/behind=0, and it is exactly the false reassurance BL-NI-77 filed:
  // the counts are zero because there is nothing to compare against.
  if (status === "noUpstream") return "upstream gone, nothing to sync";
  return "current";
}

/**
 * Rough 0..1 magnitude for the lag bar, saturating around 50 commits behind.
 * The backend counts are exact; this is only the visual scaling.
 */
export function lagMagnitude(r: StatusFacts): number {
  const status = deriveStatus(r);
  if (status === "behind") return Math.min(1, (r.behindCount ?? 0) / 50);
  if (status === "dirty") return 0.4;
  if (status === "ahead") return 0.08;
  return 0.04;
}

/** Unix-seconds to a short relative label. The backend stores integer epoch seconds. */
export function relativeTime(epochSeconds: number | null): string {
  if (epochSeconds === null) return "never";
  const deltaSec = Math.max(0, Date.now() / 1000 - epochSeconds);
  if (deltaSec < 45) return "just now";
  const min = Math.round(deltaSec / 60);
  if (min < 60) return `${min}m ago`;
  const hr = Math.round(min / 60);
  if (hr < 24) return `${hr}h ago`;
  const days = Math.round(hr / 24);
  return `${days}d ago`;
}

/**
 * A human sentence for a failed check's typed reason code.
 *
 * A check that failed now RESOLVES rather than rejecting (BL-NI-04), so the
 * screen decides what to say about it instead of falling back to whatever the
 * error carried. The codes are the frozen operational set the backend produces
 * on a non-zero fetch: `git.auth_failed`, `net.offline`, and the catch-all
 * `git.fetch_failed`.
 *
 * Every branch points at the Activity receipt, because that is where the actual
 * git output now lives, and "check Activity" is a next step the user can take
 * where "fetch failed" alone is not.
 *
 * The `null` and unrecognized branches say the same thing on purpose. A code the
 * frontend has not been taught is still a failure, and inventing a specific
 * explanation for it would be worse than admitting the generic one: the receipt
 * has the truth either way.
 */
export function checkFailureMessage(reason: string | null): string {
  switch (reason) {
    case "git.auth_failed":
      return "Authentication failed. RepoSync uses the credentials your system git already has, so check that they are still valid for this remote. The full output is in Activity.";
    case "net.offline":
      return "Could not reach the remote. Check the network connection, then try again. The full output is in Activity.";
    default:
      return "The fetch failed. Select this repository's newest entry in Activity for the exact command and git's own output.";
  }
}
