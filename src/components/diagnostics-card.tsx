import { useState } from "react";
import type { ReactNode } from "react";
import { AlertTriangle, ClipboardCopy, FolderOpen, RefreshCw } from "lucide-react";
import { commands } from "@/lib/bindings";
import type { Diagnostics } from "@/lib/bindings";
import { IpcError, unwrap } from "@/lib/ipc";
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";
import { Button } from "@/components/ui/button";
import { AsyncPanel } from "@/components/async-panel";
import { useDiagnostics } from "@/hooks/queries";
import { formatBytes, formatDiagnosticsReport } from "@/lib/diagnostics";
import { useToast } from "@/hooks/use-toast";

/**
 * Settings -> Diagnostics.
 *
 * The problem this exists to solve: RepoSync builds with
 * `windows_subsystem = "windows"`, so a release build has no console and every
 * diagnostic goes to a file in a directory the user has no reason to know
 * about. Until this card, reaching that log meant knowing to paste
 * `%LOCALAPPDATA%\RepoSync\logs` into an address bar.
 *
 * It also surfaces the one class of failure the app is otherwise silent about:
 * a scheduled check whose outcome could not be persisted (BL-NI-14). That
 * degrades invisibly on purpose - the repo stays due and retries - so a
 * non-zero count here is the only place a user could ever notice it.
 */
export function DiagnosticsCard() {
  const diagnostics = useDiagnostics();

  return (
    <Card>
      <CardHeader className="justify-between">
        <div>
          <CardTitle>Diagnostics</CardTitle>
          <p className="text-xs text-foreground/70">
            Where RepoSync keeps its files, and what it has been doing.
          </p>
        </div>
        <Button
          variant="ghost"
          size="sm"
          onClick={diagnostics.refetch}
          aria-label="Refresh diagnostics"
        >
          <RefreshCw /> Refresh
        </Button>
      </CardHeader>
      <CardContent className="p-0">
        <AsyncPanel state={diagnostics}>{(d) => <DiagnosticsBody d={d} />}</AsyncPanel>
      </CardContent>
    </Card>
  );
}

function DiagnosticsBody({ d }: { d: Diagnostics }) {
  const toast = useToast();
  const [opening, setOpening] = useState(false);

  async function openLogs() {
    setOpening(true);
    try {
      await unwrap(commands.diagnosticsOpenLogDir());
    } catch (e) {
      toast("error", "Could not open the log folder", e instanceof IpcError ? e.message : String(e));
    } finally {
      setOpening(false);
    }
  }

  async function copyDetails() {
    try {
      await navigator.clipboard.writeText(formatDiagnosticsReport(d));
      toast("ok", "Diagnostics copied", "Paste it into a bug report.");
    } catch {
      // Reported rather than swallowed: a silent no-op on a copy button reads
      // as a broken button, and the user is mid-way through filing a report.
      toast("error", "Could not copy", "Your system blocked clipboard access.");
    }
  }

  return (
    <div className="flex flex-col">
      <Warnings d={d} />

      <Row label="Log folder" hint="Everything RepoSync records goes here.">
        <Button variant="secondary" size="sm" disabled={opening} onClick={() => void openLogs()}>
          <FolderOpen /> Open logs
        </Button>
      </Row>
      <PathRow value={d.logDir} />

      <Row
        label="Logging"
        hint={
          d.loggingActive
            ? "Level and retention currently in force."
            : "RepoSync could not open a log file, so nothing is being recorded."
        }
      >
        {d.loggingActive ? (
          <Mono>
            {(d.logLevel ?? "?").toUpperCase()} - {d.logMaxFiles} days, {formatBytes(d.logMaxBytes)}{" "}
            max
          </Mono>
        ) : (
          <Mono tone="warn">not running</Mono>
        )}
      </Row>
      <Row label="Log files on disk" hint="Counted now, not remembered.">
        <Mono>
          {d.logFileCount} {d.logFileCount === 1 ? "file" : "files"}, {formatBytes(d.logBytes)}
        </Mono>
      </Row>

      <Row label="Data folder" hint="Settings, the repo registry, and the activity history.">
        <Mono>{d.onedriveRooted ? "OneDrive-synced" : "local"}</Mono>
      </Row>
      <PathRow value={d.dataDir} />
      <Row label="Database" hint="The SQLite file inside the data folder.">
        <span />
      </Row>
      <PathRow value={d.dbPath} />

      <Row label="Git" hint="The executable RepoSync runs for every fetch and pull.">
        {d.gitAvailable ? (
          <Mono>{d.gitVersion ?? "found"}</Mono>
        ) : (
          <Mono tone="warn">{d.gitVersion ? `${d.gitVersion} (too old)` : "not found"}</Mono>
        )}
      </Row>
      {d.gitPath && <PathRow value={d.gitPath} />}

      <Row
        label="Scheduled checks since launch"
        hint="Cycles run, repos checked, and outcomes that could not be saved."
      >
        <Mono tone={d.schedulerOutcomePersistFailures > 0 ? "warn" : undefined}>
          {d.schedulerCycles} / {d.schedulerReposChecked} / {d.schedulerOutcomePersistFailures}
        </Mono>
      </Row>

      <Row label="RepoSync version" hint="The build you are running.">
        <Mono>{d.appVersion}</Mono>
      </Row>

      <div className="flex items-center justify-between gap-4 px-4 py-3">
        <p className="text-xs text-foreground/70">
          Copying includes the folder paths above, which contain your Windows account name.
        </p>
        <Button variant="secondary" size="sm" onClick={() => void copyDetails()}>
          <ClipboardCopy /> Copy details
        </Button>
      </div>
    </div>
  );
}

/**
 * The things worth acting on, pulled to the top instead of left for the reader
 * to spot in a list of otherwise identical rows.
 */
function Warnings({ d }: { d: Diagnostics }) {
  const items: string[] = [];
  if (!d.loggingActive) {
    items.push(
      "Logging is not running, so nothing is being recorded. Check that the log folder below is writable.",
    );
  }
  if (d.onedriveRooted) {
    items.push(
      "Your data folder is inside a OneDrive-synced tree. A sync agent can corrupt the database while RepoSync is writing to it.",
    );
  }
  if (d.schedulerOutcomePersistFailures > 0) {
    items.push(
      `${d.schedulerOutcomePersistFailures} scheduled check outcome(s) could not be saved. Those repos retried, but the log has the reason (search for scheduler.outcome_persist_failed).`,
    );
  }
  if (d.dbRecovered) {
    items.push(
      "The database could not be migrated at startup and was moved aside. RepoSync started with a fresh one.",
    );
  }
  if (items.length === 0) return null;

  return (
    <div className="flex flex-col gap-2 border-b border-border bg-status-failed/10 px-4 py-3">
      {items.map((text) => (
        <div key={text} className="flex items-start gap-2">
          <AlertTriangle className="mt-0.5 size-4 shrink-0 text-status-failed" aria-hidden />
          <p className="text-sm text-foreground">{text}</p>
        </div>
      ))}
    </div>
  );
}

function Row({ label, hint, children }: { label: string; hint?: string; children: ReactNode }) {
  return (
    <div className="flex items-center justify-between gap-4 border-b border-border px-4 pb-1.5 pt-3">
      <div className="min-w-0">
        <div className="text-sm font-medium">{label}</div>
        {hint && <div className="text-xs text-foreground/70">{hint}</div>}
      </div>
      <div className="flex shrink-0 items-center gap-2">{children}</div>
    </div>
  );
}

/**
 * A path on its own full-width line. Paths are long and the point of showing
 * them is that they can be read and selected, so they get the width rather than
 * being truncated into a right-hand column.
 */
function PathRow({ value }: { value: string }) {
  return (
    <div className="border-b border-border bg-muted/40 px-4 py-2">
      <code className="block break-all font-mono text-[11px] text-foreground/90">{value}</code>
    </div>
  );
}

function Mono({ children, tone }: { children: ReactNode; tone?: "warn" }) {
  return (
    <span
      className={
        tone === "warn"
          ? "font-mono text-xs font-semibold text-status-failed"
          : "font-mono text-xs font-semibold text-foreground"
      }
    >
      {children}
    </span>
  );
}
