// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it } from "vitest";
import { readColumnPrefs, writeColumnPrefs } from "@/lib/column-prefs";

/**
 * This file opts into jsdom despite holding no components and no rendered
 * DOM, which looks like it breaks AGENTS.md's "pure-logic tests in `src/lib`
 * stay in `node`" rule. It does not: that rule is about proving the LOGIC
 * needs no DOM, and this logic's whole job is talking to `localStorage`, a
 * browser global `node` does not provide at all - there is nothing to prove
 * by running it without one. jsdom supplies a real `localStorage` backed by
 * an in-memory store, which is what every test below actually exercises.
 */

const KEY = "test.column-prefs.v1";
const DEFAULT_ORDER = ["a", "b", "c"];

beforeEach(() => {
  localStorage.clear();
});

afterEach(() => {
  localStorage.clear();
});

describe("readColumnPrefs", () => {
  it("falls back to the default order, all visible, when nothing is stored", () => {
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER)).toEqual({
      order: DEFAULT_ORDER,
      visibility: {},
    });
  });

  it("falls back to the default when the stored value is not valid JSON", () => {
    localStorage.setItem(KEY, "{not json");
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER)).toEqual({
      order: DEFAULT_ORDER,
      visibility: {},
    });
  });

  it("falls back to the default when the stored value is valid JSON but the wrong shape", () => {
    localStorage.setItem(KEY, JSON.stringify("just a string"));
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER)).toEqual({
      order: DEFAULT_ORDER,
      visibility: {},
    });
  });

  it("round-trips a stored order and visibility map", () => {
    writeColumnPrefs(KEY, { order: ["c", "a", "b"], visibility: { b: false } });
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER)).toEqual({
      order: ["c", "a", "b"],
      visibility: { b: false },
    });
  });

  it("ignores a stored column id that no longer exists, without dropping the ids that do", () => {
    writeColumnPrefs(KEY, { order: ["b", "ghost-column", "a", "c"], visibility: {} });
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER).order).toEqual(["b", "a", "c"]);
  });

  it("appends a known id missing from the stored order, rather than dropping it from the table", () => {
    // Simulates a column shipped after the value was saved: "c" is known
    // today but the stored order predates it.
    writeColumnPrefs(KEY, { order: ["b", "a"], visibility: {} });
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER).order).toEqual(["b", "a", "c"]);
  });

  it("keeps each stored id once, first occurrence winning, so a duplicated id cannot add a column", () => {
    // The Codex review's case: a malformed saved order that repeats an id.
    // Before the fix this read back as four entries for three columns.
    writeColumnPrefs(KEY, { order: ["b", "c", "b", "a", "c"], visibility: {} });
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER).order).toEqual(["b", "c", "a"]);
  });

  it("drops a visibility entry for an id that no longer exists", () => {
    writeColumnPrefs(KEY, { order: DEFAULT_ORDER, visibility: { "ghost-column": false, b: false } });
    expect(readColumnPrefs(KEY, DEFAULT_ORDER, DEFAULT_ORDER).visibility).toEqual({ b: false });
  });
});
