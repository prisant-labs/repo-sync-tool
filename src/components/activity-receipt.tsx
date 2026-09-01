import { ClipboardCopy, X } from "lucide-react";
import type { ActivityRecord } from "@/lib/bindings";
import { Button } from "@/components/ui/button";
import { useToast } from "@/hooks/use-toast";
import { relativeTime } from "@/lib/status";
import { absoluteTime, formatReceipt } from "@/lib/activity";
import { cn } from "@/lib/utils";

/**
 * The id of this component's own repo-name heading, so any `Drawer` that
 * renders it can name itself via `aria-labelledby` from the same visible
 * text a sighted user reads (Codex adversarial review, finding 3, confirmed
 * - the receipt drawer had no accessible name). Both call sites
 * (`screens/activity.tsx`'s own receipt, and `repo-detail.tsx`'s nested
 * receipt on the Activity tab) reuse this one constant; only one
 * `ActivityReceipt` is ever mounted at a time (the two live in mutually
 * exclusive screens/panels), so the shared id is safe.
 */
export const ACTIVITY_RECEIPT_TITLE_ID = "activity-receipt-title";

/**
 * The receipt for one activity row: what RepoSync ran, what git said back, and
 * how it was classified.
 *
 * The activity log has always captured `raw_command` / `raw_stdout` /
 * `raw_stderr`, and `activity_list` has always returned them, but nothing
 * rendered them - so the audit trail existed and was unreadable. This is the
 * surface that makes "why did that repo not update" answerable without opening
 * the SQLite file.
 *
 * The three streams are shown VERBATIM (as stored). They are already cleaned:
 * `redact::redact_secrets` runs at capture time in `run_git`, before the 16 KiB
 * size cap, so what is in the database is what is safe to show. Doing anything
 * further here would only make this one view disagree with the log file and the
 * error toast, which read the same bytes.
 */
export function ActivityReceipt({
  record,
  repoName,
  onClose,
}: {
  record: ActivityRecord;
  repoName: string | null;
  onClose: () => void;
}) {
  const toast = useToast();

  async function copy() {
    try {
      await navigator.clipboard.writeText(formatReceipt(record, repoName));
      toast("ok", "Receipt copied");
    } catch {
      toast("error", "Could not copy", "Your system blocked clipboard access.");
    }
  }

  return (
    <>
      <div className="flex items-start justify-between gap-3 border-b border-border px-5 py-4">
        <div className="min-w-0">
          <div className="flex items-center gap-2">
            <span className="inline-flex w-fit rounded-md bg-muted px-2 py-0.5 font-mono text-[11px] font-semibold">
              {record.actionType}
            </span>
            <StatusChip status={record.status} />
          </div>
          <h3 id={ACTIVITY_RECEIPT_TITLE_ID} className="mt-1.5 truncate text-base font-semibold">
            {repoName ?? "Unknown repo"}
          </h3>
          <p className="text-xs text-foreground/70">
            {absoluteTime(record.timestamp)} - {relativeTime(record.timestamp)}
          </p>
        </div>
        <Button variant="ghost" size="icon" onClick={onClose} aria-label="Close receipt">
          <X />
        </Button>
      </div>

      <div className="flex-1 overflow-y-auto">
        {record.summary && (
          <p className="border-b border-border px-5 py-3 text-sm text-foreground">
            {record.summary}
          </p>
        )}

        <dl className="grid grid-cols-2 gap-x-4 gap-y-2.5 border-b border-border px-5 py-3">
          <Fact label="Reason" value={record.reasonCode} mono />
          <Fact label="Commits" value={record.commitRange} mono />
          <Fact
            label="Exit code"
            value={record.exitCode === null ? null : String(record.exitCode)}
            mono
            tone={record.exitCode !== null && record.exitCode !== 0 ? "bad" : undefined}
          />
          <Fact
            label="Duration"
            value={record.durationMs === null ? null : `${record.durationMs} ms`}
            mono
          />
        </dl>

        <Stream label="Command" value={record.rawCommand} />
        <Stream label="Output" value={record.rawStdout} />
        <Stream label="Errors" value={record.rawStderr} tone="bad" />
      </div>

      {/*
        The wording here is load-bearing and was deliberately weakened after an
        adversarial review. It previously read "Credentials are stripped from
        captured git output" full stop, which claims more than the redactor
        delivers: URL credentials are removed structurally, but the token-prefix
        list is short and best-effort by design, and the command line always
        contains the repository's full local path. Sitting an unqualified
        reassurance next to a Copy button is the worst place in the app to
        overclaim, because the user acts on it immediately and irreversibly.
      */}
      <div className="flex items-center justify-between gap-3 border-t border-border px-5 py-3">
        <p className="text-xs text-foreground/80">
          Before sharing: credentials embedded in URLs are removed and common token formats are
          stripped where recognized, but this includes the repository&apos;s full local path and any
          secret in an unfamiliar format.
        </p>
        <Button variant="secondary" size="sm" onClick={() => void copy()}>
          <ClipboardCopy /> Copy
        </Button>
      </div>
    </>
  );
}

/**
 * One captured stream.
 *
 * `null` and `""` are rendered DIFFERENTLY on purpose. A `null` means RepoSync
 * never captured that stream for this action - a policy decision that skipped
 * without running git, say - while an empty string means git ran and printed
 * nothing. Collapsing them into one "no output" would erase exactly the
 * distinction someone opens this panel to make.
 */
function Stream({ label, value, tone }: { label: string; value: string | null; tone?: "bad" }) {
  return (
    <section className="border-b border-border px-5 py-3">
      <h4 className="mb-1.5 text-xs font-semibold uppercase tracking-wide text-foreground/70">
        {label}
      </h4>
      {value === null ? (
        <p className="text-xs italic text-foreground/70">not captured for this action</p>
      ) : value === "" ? (
        <p className="text-xs italic text-foreground/70">empty (git printed nothing)</p>
      ) : (
        <pre
          className={cn(
            "max-h-64 overflow-auto whitespace-pre-wrap break-all rounded-md border border-border bg-muted/50 p-2.5 font-mono text-[11px] leading-relaxed",
            tone === "bad" ? "text-status-failed" : "text-foreground/90",
          )}
        >
          {value}
        </pre>
      )}
    </section>
  );
}

function Fact({
  label,
  value,
  mono,
  tone,
}: {
  label: string;
  value: string | null;
  mono?: boolean;
  tone?: "bad";
}) {
  return (
    <div className="min-w-0">
      <dt className="text-xs text-foreground/70">{label}</dt>
      <dd
        className={cn(
          "truncate text-sm",
          mono && "font-mono text-xs",
          tone === "bad" ? "font-semibold text-status-failed" : "text-foreground",
        )}
      >
        {value ?? "-"}
      </dd>
    </div>
  );
}

/** The activity row's own `status` string, which is not the repo status taxonomy. */
function StatusChip({ status }: { status: string }) {
  const bad = status === "failed" || status === "error";
  return (
    <span
      className={cn(
        "inline-flex w-fit rounded-md px-2 py-0.5 font-mono text-[11px] font-semibold",
        bad ? "bg-status-failed/15 text-status-failed" : "bg-status-sync/15 text-status-sync",
      )}
    >
      {status}
    </span>
  );
}
