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
