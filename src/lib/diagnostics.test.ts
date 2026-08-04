import { describe, expect, it } from "vitest";
import type { Diagnostics } from "@/lib/bindings";
import { formatBytes, formatDiagnosticsReport } from "@/lib/diagnostics";

function diagnostics(overrides: Partial<Diagnostics> = {}): Diagnostics {
  return {
    appVersion: "0.9.0",
    dataDir: "C:\\Users\\someone\\AppData\\Local\\RepoSync",
    dbPath: "C:\\Users\\someone\\AppData\\Local\\RepoSync\\reposync.db",
    logDir: "C:\\Users\\someone\\AppData\\Local\\RepoSync\\logs",
    loggingActive: true,
    logLevel: "info",
    logMaxFiles: 14,
    logMaxBytes: 32 * 1024 * 1024,
    logDirReadable: true,
    logFileCount: 3,
    logBytes: 1024,
    onedriveRooted: false,
    gitPath: "C:\\Program Files\\Git\\cmd\\git.exe",
    gitVersion: "2.40.1",
    gitResolved: true,
    gitMeetsFloor: true,
    schedulerCycles: 12,
    schedulerReposChecked: 30,
    schedulerOutcomePersistFailures: 0,
    dbRecovered: false,
    ...overrides,
  };
}

describe("formatBytes", () => {
  it("renders an absent budget as a dash, not as zero", () => {
    // `null` is what the log budget fields carry when logging failed to start.
    // "0 B" would be a measurement; this is an absence, and the difference is
    // the whole reason someone is reading the card.
    expect(formatBytes(null)).toBe("-");
    expect(formatBytes(0)).toBe("0 B");
  });

  it("scales through bytes, kilobytes, and megabytes", () => {
    expect(formatBytes(512)).toBe("512 B");
    expect(formatBytes(2048)).toBe("2 KB");
    expect(formatBytes(32 * 1024 * 1024)).toBe("32 MB");
    expect(formatBytes(1024 * 1024 * 1.5)).toBe("1.5 MB");
  });
});

describe("formatDiagnosticsReport", () => {
  it("includes every field the card shows", () => {
    const text = formatDiagnosticsReport(diagnostics());
    expect(text).toContain("RepoSync 0.9.0");
    expect(text).toContain("git: ok 2.40.1");
    expect(text).toContain("logging: info, 14 days, 32 MB max");
    expect(text).toContain("log files: 3 (1 KB)");
    expect(text).toContain("\\RepoSync\\logs");
    expect(text).toContain("scheduler: 12 cycles, 30 repos checked, 0 outcome writes failed");
  });

  /**
   * The three conditions worth acting on are marked in upper case INLINE rather
   * than appended as a trailing section, so they survive being quoted,
   * truncated, or skimmed in an issue thread.
   */
  it("marks a logger that never started loudly", () => {
    const text = formatDiagnosticsReport(
      diagnostics({ loggingActive: false, logLevel: null, logMaxFiles: null, logMaxBytes: null }),
    );
    expect(text).toContain("logging: DID NOT START");
    // The directory is still reported: knowing where logs WOULD have gone is
    // what makes "is it a permissions problem" answerable.
    expect(text).toContain("log dir: ");
  });

  /**
   * `loggingActive` only proves the subscriber installed at startup -
   * `tracing_appender` writes on a worker thread and surfaces no later errors,
   * so it stays true while a full disk silently swallows every event. The one
   * live check available is whether anything actually landed, and a report that
   * said "logging: info" beside a bare "0" would read as healthy.
   */
  it("marks a logger that started but wrote nothing", () => {
    const text = formatDiagnosticsReport(diagnostics({ logFileCount: 0, logBytes: 0 }));
    expect(text).toContain("[NOTHING WRITTEN]");
  });

  it("does not cry wolf when the logger never started in the first place", () => {
    // Zero files is expected, not suspicious, when logging never started - the
    // preceding line already says so, and two alarms for one fact is noise.
    const text = formatDiagnosticsReport(
      diagnostics({ loggingActive: false, logFileCount: 0, logBytes: 0 }),
    );
    expect(text).not.toContain("[NOTHING WRITTEN]");
  });

  it("reports an unreadable log folder as its own state, not as empty", () => {
    const text = formatDiagnosticsReport(diagnostics({ logDirReadable: false, logFileCount: 0 }));
    expect(text).toContain("log files: FOLDER UNREADABLE");
  });

  it("marks a OneDrive-rooted data directory and a recovered database", () => {
    const text = formatDiagnosticsReport(diagnostics({ onedriveRooted: true, dbRecovered: true }));
    expect(text).toContain("[ONEDRIVE-SYNCED]");
    expect(text).toContain("[RECOVERED AT STARTUP]");
  });

  /**
   * A git below the >= 2.30 floor and a git that is missing entirely are
   * DIFFERENT states, and only the second one stops RepoSync running git at all
   * (E-03 AC7: a below-floor git is "usable but flagged"). Reporting both as
   * "unavailable" would send a reader chasing a missing binary that is sitting
   * right there at the path printed on the same line.
   */
  it("distinguishes a below-floor git from a missing one", () => {
    const belowFloor = formatDiagnosticsReport(
      diagnostics({ gitResolved: true, gitMeetsFloor: false, gitVersion: "2.28.0" }),
    );
    expect(belowFloor).toContain("BELOW 2.30 FLOOR 2.28.0");
    expect(belowFloor).toContain("git.exe");

    const missing = formatDiagnosticsReport(
      diagnostics({ gitResolved: false, gitMeetsFloor: false, gitVersion: null, gitPath: null }),
    );
    expect(missing).toContain("git: NOT FOUND");
    expect(missing).not.toContain("BELOW");
  });

  /** BL-NI-14 reaching the surface: a silent retry storm has to be countable. */
  it("carries the scheduler outcome-persist failure count", () => {
    const text = formatDiagnosticsReport(diagnostics({ schedulerOutcomePersistFailures: 4 }));
    expect(text).toContain("4 outcome writes failed");
  });
});
