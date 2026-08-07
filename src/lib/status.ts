import { AlertTriangle, ArrowDown, ArrowUp, Check, PauseCircle, XCircle } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { RepoSummary } from "@/lib/bindings";

/** The 7-state taxonomy, derived on the frontend from raw RepoSummary facts. */
export type RepoStatus = "sync" | "ahead" | "behind" | "dirty" | "failed" | "paused";

type StatusFacts = Pick<
  RepoSummary,
  "isDirty" | "enabled" | "autoPaused" | "lastErrorCode" | "aheadCount" | "behindCount"
>;

/**
 * Priority order: paused > failed > dirty > behind > ahead > sync.
 *
 * The wire type carries only raw facts (no `status` field), so this ranking is
 * a frontend policy decision. Re-ranking (e.g. "dirty-and-behind reads as
 * behind") is a one-line change here, never a backend migration.
 */
export function deriveStatus(r: StatusFacts): RepoStatus {
  if (!r.enabled || r.autoPaused) return "paused";
  if (r.lastErrorCode) return "failed";
  if (r.isDirty) return "dirty";
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
};

/**
 * Per-status presentation. Class strings are written out in full (never
 * interpolated) so Tailwind's scanner can see them at build time.
 */
export const STATUS_STYLE: Record<
  RepoStatus,
  { label: string; text: string; bar: string; tint: string }
> = {
  sync: { label: "In sync", text: "text-status-sync", bar: "bg-status-sync", tint: "bg-status-sync/12" },
  ahead: { label: "Ahead", text: "text-status-sync", bar: "bg-status-sync", tint: "bg-status-sync/12" },
  behind: { label: "Behind", text: "text-status-behind", bar: "bg-status-behind", tint: "bg-status-behind/12" },
  dirty: { label: "Dirty", text: "text-status-dirty", bar: "bg-status-dirty", tint: "bg-status-dirty/12" },
  failed: { label: "Failed", text: "text-status-failed", bar: "bg-status-failed", tint: "bg-status-failed/12" },
  paused: { label: "Paused", text: "text-status-paused", bar: "bg-status-paused", tint: "bg-status-paused/12" },
};

/** A human "behind by N" style lag label from the raw counts + derived status. */
export function lagLabel(r: StatusFacts): string {
  const status = deriveStatus(r);
  if (status === "behind") return `${r.behindCount ?? 0} behind`;
  if (status === "ahead") return `${r.aheadCount ?? 0} ahead, clean`;
  if (status === "dirty") return "uncommitted, skipped";
  if (status === "failed") return "check failed";
  if (status === "paused") return "watching paused";
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
