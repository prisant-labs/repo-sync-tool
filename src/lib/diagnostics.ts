import type { Diagnostics } from "@/lib/bindings";

/**
 * Formatting for the Settings -> Diagnostics card.
 *
 * Split out of the component so both functions can be tested directly. That
 * matters more than usual here: the report string is what a user pastes into a
 * bug report, so a field silently dropping out of it is a defect nobody notices
 * until an issue arrives missing the one detail that would have explained it.
 */

/**
 * Bytes as a short human string. `null` - which is what the log budget fields
 * carry when logging failed to start - renders as a dash rather than "0 B",
 * because zero is a measurement and this is an absence.
 */
export function formatBytes(bytes: number | null): string {
  if (bytes === null) return "-";
  if (bytes < 1024) return `${bytes} B`;
  const mib = bytes / (1024 * 1024);
  if (mib >= 1) return `${mib % 1 === 0 ? mib : mib.toFixed(1)} MB`;
  return `${(bytes / 1024).toFixed(0)} KB`;
}

/**
 * The plain-text form for a bug report. Field order matches the card, so
 * someone reading the paste and someone reading the screen are looking at the
 * same thing in the same order.
 *
 * The three conditions worth acting on - logging not running, a OneDrive-rooted
 * data directory, a database recovered at startup - are marked in upper case
 * inline rather than appended as a separate section, so they survive being
 * quoted, truncated, or read quickly.
 */
export function formatDiagnosticsReport(d: Diagnostics): string {
  const git = `${d.gitAvailable ? "ok" : "unavailable"}${d.gitVersion ? ` ${d.gitVersion}` : ""}${
    d.gitPath ? ` (${d.gitPath})` : ""
  }`;
  const logging = d.loggingActive
    ? `${d.logLevel ?? "?"}, ${d.logMaxFiles} days, ${formatBytes(d.logMaxBytes)} max`
    : "DID NOT START";
  // "started but nothing on disk" is the only live evidence available that
  // writes are failing, so it is marked rather than left for a reader to infer
  // from a zero. `logging: started` alone would be a claim the app cannot back.
  const files = !d.logDirReadable
    ? "log files: FOLDER UNREADABLE"
    : `log files: ${d.logFileCount} (${formatBytes(d.logBytes)})${
        d.loggingActive && d.logFileCount === 0 ? " [NOTHING WRITTEN]" : ""
      }`;

  return [
    `RepoSync ${d.appVersion}`,
    `git: ${git}`,
    `logging: ${logging}`,
    files,
    `log dir: ${d.logDir}`,
    `data dir: ${d.dataDir}${d.onedriveRooted ? " [ONEDRIVE-SYNCED]" : ""}`,
    `db: ${d.dbPath}${d.dbRecovered ? " [RECOVERED AT STARTUP]" : ""}`,
    `scheduler: ${d.schedulerCycles} cycles, ${d.schedulerReposChecked} repos checked, ` +
      `${d.schedulerOutcomePersistFailures} outcome writes failed`,
  ].join("\n");
}
