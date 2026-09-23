import { useEffect, useState } from "react";
import { getVersion } from "@tauri-apps/api/app";

/**
 * The running app version, read once from Tauri at mount (the real semver
 * from `tauri.conf.json`, not a hand-maintained literal).
 *
 * AC-4 (E-21, composite pin 29, register STG4) asks only that Settings name
 * the running version. Consolidating every read into one hook is a judgement
 * made while building it, NOT something the criterion demanded - corrected
 * after an audit found the earlier wording putting words in the spec's mouth.
 * It is still the right call, for the reason below. Consolidated 2026-09-23
 * from THREE separate copies:
 * `app-shell.tsx`'s sidebar (`useAppVersion`, moved here unchanged), and
 * `screens/settings.tsx`'s "Updates" card, which kept its own `version`
 * state AND overwrote it with `UpdateAvailability.currentVersion` after a
 * manual check - a second read that could never disagree with this one,
 * since both name the same running binary from the same Tauri config, so it
 * added a place for drift rather than any new information. The Settings
 * "About" section (AC-4) is the third consumer.
 *
 * Falls back to `null` while the async call resolves, or if it rejects,
 * following the same mounted-guard idiom as `useAsync` (hooks/use-async.ts).
 */
export function useAppVersion(): string | null {
  const [version, setVersion] = useState<string | null>(null);
  useEffect(() => {
    let active = true;
    getVersion()
      .then((v) => {
        if (active) setVersion(v);
      })
      .catch(() => {
        if (active) setVersion(null);
      });
    return () => {
      active = false;
    };
  }, []);
  return version;
}
