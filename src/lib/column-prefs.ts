/**
 * Column visibility/order persistence for a `DataTable` caller (E-21 AC-16).
 *
 * DECISION FLAGGED FOR VETO: this is `localStorage`, not a backend setting.
 * No backend field exists for "which columns are visible and in what
 * order" and adding one is the `src-tauri` chokepoint - out of scope for
 * this frontend-only change per AGENTS.md. It is also a pure display
 * preference, not data that needs to sync anywhere or survive a reinstall,
 * which is the usual case FOR `localStorage` over a Settings field. Still,
 * it is the first piece of UI state this app persists outside the backend
 * rather than through `Settings`, so the maintainer gets the chance to say
 * otherwise before a second screen copies the pattern.
 *
 * Deliberately NOT here: the sort column/direction. The criterion is
 * explicit that sort resets every session, so it lives in a plain
 * `useState` on the screen and never touches storage at all.
 */

export type ColumnPrefs = {
  /** Every known column id, in the user's chosen display order. */
  order: string[];
  /** `id -> false` for hidden; absent means visible. */
  visibility: Record<string, boolean>;
};

/**
 * Reads a versioned `localStorage` key, falling back to `defaultOrder` (all
 * visible, in that order) on ANY problem: no value saved yet, a value that
 * is not JSON, a value whose shape does not match `ColumnPrefs`, or
 * `localStorage` itself throwing (quota, private browsing, disabled). The
 * try/catch is the point of this function - a corrupt or missing value must
 * never crash the screen, only fall back.
 *
 * `knownIds` (normally the same array as `defaultOrder`, kept separate so a
 * caller cannot accidentally couple "what order do we load if nothing was
 * saved" to "what ids currently exist") filters the stored order: an id the
 * table no longer renders is dropped silently, and any known id the stored
 * order is missing - a column shipped after the value was saved - is
 * appended at the end, so a column can never go missing from the table
 * because of a stale stored order.
 */
export function readColumnPrefs(
  key: string,
  knownIds: readonly string[],
  defaultOrder: readonly string[],
): ColumnPrefs {
  const fallback: ColumnPrefs = { order: [...defaultOrder], visibility: {} };
  try {
    const raw = localStorage.getItem(key);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Partial<ColumnPrefs> | null;
    if (parsed === null || typeof parsed !== "object") return fallback;

    const known = new Set(knownIds);
    const storedOrder = Array.isArray(parsed.order) ? parsed.order : [];
    const order = storedOrder.filter((id): id is string => typeof id === "string" && known.has(id));
    const missing = defaultOrder.filter((id) => !order.includes(id));

    const visibility: Record<string, boolean> = {};
    if (parsed.visibility && typeof parsed.visibility === "object") {
      for (const id of known) {
        const v = (parsed.visibility as Record<string, unknown>)[id];
        if (typeof v === "boolean") visibility[id] = v;
      }
    }

    return { order: [...order, ...missing], visibility };
  } catch {
    return fallback;
  }
}

/** Writes a `ColumnPrefs` value, swallowing any `localStorage` failure - the
 *  in-memory state the screen already holds still works for this session;
 *  only persistence across a restart is lost. */
export function writeColumnPrefs(key: string, prefs: ColumnPrefs): void {
  try {
    localStorage.setItem(key, JSON.stringify(prefs));
  } catch {
    // See the doc comment above: persistence is best-effort.
  }
}
