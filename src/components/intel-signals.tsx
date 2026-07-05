import { GitPullRequest, Package } from "lucide-react";
import { cn } from "@/lib/utils";

/**
 * Branch and PR intelligence signal chips (E-17): a compact open-PR count and the
 * latest release tag for the repo row.
 *
 * Rendered in the DESIGN.md SIGNAL REGISTER (the magenta `status-release` token),
 * NEVER the status-taxonomy colors, honoring the Status-Owns-Saturation rule so PR
 * and release info can never masquerade as sync status (E-17 AC9). A `null`
 * `openPrCount` is an un-refreshed / non-GitHub / private-inaccessible repo (a clean
 * unknown) and simply renders no PR chip - it is never shown as a fabricated "0".
 */
export function IntelSignals({
  latestReleaseTag,
  openPrCount,
  className,
}: {
  latestReleaseTag: string | null;
  openPrCount: number | null;
  className?: string;
}) {
  const hasRelease = latestReleaseTag !== null && latestReleaseTag !== "";
  const hasPrs = openPrCount !== null && openPrCount > 0;
  if (!hasRelease && !hasPrs) return null;

  return (
    <div className={cn("flex flex-wrap items-center gap-x-3 gap-y-1", className)}>
      {hasRelease && (
        <span
          className="inline-flex items-center gap-1 font-mono text-[11px] font-medium text-status-release"
          title={`Latest release ${latestReleaseTag}`}
        >
          <Package className="size-3 shrink-0" />
          {latestReleaseTag}
        </span>
      )}
      {hasPrs && (
        <span
          className="inline-flex items-center gap-1 font-mono text-[11px] font-medium text-status-release"
          title={`${openPrCount} open pull request${openPrCount === 1 ? "" : "s"}`}
        >
          <GitPullRequest className="size-3 shrink-0" />
          {openPrCount} {openPrCount === 1 ? "PR" : "PRs"}
        </span>
      )}
    </div>
  );
}
