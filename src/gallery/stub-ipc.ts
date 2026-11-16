import { createHandlers, createWorld, dispatch, type Handlers } from "@/gallery/handlers";
import type { Scenario } from "@/gallery/fixtures";
import { DIAGNOSTICS } from "@/gallery/fixtures";

/**
 * Installs the Tauri IPC stub, so the REAL components render in a plain browser
 * with no Tauri runtime. The commands themselves live in `handlers.ts`.
 *
 * `@tauri-apps/api/core`'s `invoke` reads `window.__TAURI_INTERNALS__.invoke`
 * at CALL time, so installing the stub before any component runs is enough.
 * This module has side effects on purpose: the gallery entry imports it first.
 *
 * The scenario is a module variable read at call time, not React state, because
 * the stub lives outside React. `setScenario` swaps in a fresh in-memory library;
 * the gallery then remounts the section by `key`, and its hooks refetch.
 */

let scenario: Scenario = "populated";
let handlers: Handlers = createHandlers(createWorld(scenario));

export function setScenario(next: Scenario): void {
  scenario = next;
  handlers = createHandlers(createWorld(next));
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
      if (cmd === "plugin:app|version") return DIAGNOSTICS.appVersion;
      // The native folder picker. A gallery has no desktop, so it behaves as if
      // the person cancelled, which is a real answer the dialog already handles.
      if (cmd === "plugin:dialog|open") return null;
      // The loading scenario never resolves a command, so every `AsyncPanel`
      // sits in its spinner branch for as long as it stays up.
      if (scenario === "loading") return new Promise<never>(() => {});
      try {
        return dispatch(handlers, cmd, args);
      } catch (e) {
        // An unstubbed command is a gallery bug, not an app state: say so loudly.
        if (e instanceof Error) console.error(e.message);
        throw e;
      }
    },
  };
}

install();
