import { fixturesFor, type Scenario } from "@/gallery/fixtures";

/**
 * Stub for the Tauri IPC layer, so the REAL components render in a plain
 * browser with no Tauri runtime.
 *
 * `@tauri-apps/api/core`'s `invoke` reads `window.__TAURI_INTERNALS__.invoke`
 * at CALL time, so installing the stub before any component runs is enough.
 * This module has side effects on purpose: the gallery entry imports it first.
 *
 * Handlers are keyed by the wire command name (snake_case), the same string the
 * generated bindings pass to `invoke`. A handler resolves to the raw payload;
 * the generated `typedError` wrapper turns that into `{ status: "ok" }`.
 *
 * The scenario is a module variable read at call time, not React state, because
 * the stub lives outside React. The gallery sets it, then remounts the screen
 * by `key`, and the remounted hooks refetch under the new scenario.
 */

let scenario: Scenario = "populated";

export function setScenario(next: Scenario): void {
  scenario = next;
}

type Handler = (args: Record<string, unknown> | undefined) => unknown;

function handlers(): Record<string, Handler> {
  const f = fixturesFor(scenario);
  return {
    repo_list: () => f.repos,
    repo_get: (a) => f.detailFor(typeof a?.id === "number" ? a.id : 0),
    settings_get: () => f.settings,
    group_list: () => f.groups,
    groups_for_repo: (a) => f.groupsForRepo(typeof a?.repoId === "number" ? a.repoId : 0),
    activity_list: () => f.activity,
    repo_group_memberships: () => f.memberships,
    db_recovery_notice: () => f.dbRecovery,
    summary_today: () => f.summary,
    repo_scan_parent: (a) => f.scan(typeof a?.path === "string" ? a.path : null),
    diagnostics_get: () => f.diagnostics,
    app_check_for_update: () => f.updateAvailability,
    // No-op successes: nothing in the gallery persists a change, so an action
    // button answers with a plausible result and the screen carries on.
    repo_add_path: () => 99,
    "plugin:app|version": () => f.diagnostics.appVersion,
  };
}

/** Installs the stub. Called once at module load, before any component renders. */
function install(): void {
  const w = window as unknown as Record<string, unknown>;

  // `@tauri-apps/api/event`'s unlisten reaches for a SEPARATE global; without it
  // every component that subscribes to a backend event throws on unmount.
  w.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    unregisterListener: () => Promise.resolve(),
  };

  w.__TAURI_INTERNALS__ = {
    transformCallback: (cb: unknown) => {
      const id = Math.floor(Math.random() * 1e9);
      w[`_cb_${id}`] = cb;
      return id;
    },
    unregisterCallback: () => {},
    convertFileSrc: (p: string) => p,
    invoke: async (cmd: string, args?: Record<string, unknown>) => {
      // Event-plugin calls (listen / unlisten) must resolve or a subscribing
      // component never finishes mounting.
      if (cmd.startsWith("plugin:event|")) return 0;
      const handler = handlers()[cmd];
      if (handler) {
        // The loading scenario never resolves a stubbed command, so every
        // `AsyncPanel` sits in its spinner branch for as long as it stays up.
        if (scenario === "loading") return new Promise<never>(() => {});
        return handler(args);
      }
      // Anything unstubbed is a no-op rather than a rejection: a gallery that
      // explodes on an unrelated command teaches nothing.
      console.warn("[gallery] unstubbed command:", cmd, args);
      return null;
    },
  };
}

install();
