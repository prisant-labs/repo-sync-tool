import { readFileSync, readdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { CALLED_COMMANDS, WIRE_ARGS, createHandlers, createWorld, dispatch, toWireName } from "@/gallery/handlers";
import type { ActivityFilter } from "@/lib/bindings";

const SRC = fileURLToPath(new URL("..", import.meta.url));

function sourceFiles(): string[] {
  return readdirSync(SRC, { recursive: true, encoding: "utf8" })
    .filter((f) => /\.tsx?$/.test(f) && !/\.test\.tsx?$/.test(f) && !f.endsWith("bindings.ts"))
    .map((f) => `${SRC}/${f}`);
}

const NO_FILTER: ActivityFilter = { repoId: null, groupId: null, actionType: null, status: null, limit: null };

function fresh(scenario: "populated" | "empty" = "populated") {
  return createHandlers(createWorld(scenario));
}

describe("the gallery IPC stub", () => {
  it("has a handler for every command the app source can call", () => {
    const called = new Set<string>();
    for (const file of sourceFiles()) {
      for (const m of readFileSync(file, "utf8").matchAll(/\bcommands\.([a-zA-Z]+)/g)) called.add(m[1]);
    }
    const missing = [...called].filter((c) => !(CALLED_COMMANDS as readonly string[]).includes(c));
    expect(missing).toEqual([]);
  });

  it("names each command's wire arguments exactly as the generated bindings send them", () => {
    const bindings = readFileSync(`${SRC}/lib/bindings.ts`, "utf8");
    const wire = new Map<string, { name: string; args: string[] }>();
    for (const m of bindings.matchAll(/^\t(\w+): \(.*?__TAURI_INVOKE(?:<[^>]*>)?\("(\w+)"(?:, \{ ([^}]*) \})?\)/gm)) {
      wire.set(m[1], { name: m[2], args: m[3] ? m[3].split(",").map((a) => a.trim()) : [] });
    }
    for (const command of CALLED_COMMANDS) {
      const real = wire.get(command);
      expect(real, `${command} not found in bindings.ts`).toBeDefined();
      expect(toWireName(command)).toBe(real!.name);
      expect([...WIRE_ARGS[command]]).toEqual(real!.args);
    }
  });

  it("rejects an unknown command, naming it, instead of resolving", () => {
    expect(() => dispatch(fresh(), "repo_teleport", {})).toThrow(/repo_teleport/);
  });

  it("answers a check-now call with a result that is not a failure", () => {
    const result = dispatch(fresh(), "repo_check_now", { id: 1 }) as { failed: boolean };
    expect(result.failed).toBe(false);
  });

  it("reports a failed check for a repository that has a recorded error", () => {
    const result = dispatch(fresh(), "repo_check_now", { id: 5 }) as { failed: boolean; reason: string | null };
    expect(result.failed).toBe(true);
    expect(result.reason).toBe("net.offline");
  });

  describe("activity_list", () => {
    it("applies the repository filter", () => {
      const rows = fresh().activityList({ ...NO_FILTER, repoId: 3 });
      expect(rows.length).toBeGreaterThan(0);
      expect(rows.every((r) => r.repoId === 3)).toBe(true);
    });

    it("applies the action and outcome filters together, and can come back empty", () => {
      const h = fresh();
      const failedUpdates = h.activityList({ ...NO_FILTER, actionType: "update", status: "failed" });
      expect(failedUpdates.map((r) => r.id)).toEqual([6]);
      expect(h.activityList({ ...NO_FILTER, actionType: "check", repoId: 6, status: "failed" })).toEqual([]);
    });

    it("scopes to a group through membership", () => {
      const h = fresh();
      const inPersonal = h.activityList({ ...NO_FILTER, groupId: 2 });
      expect(inPersonal.length).toBeGreaterThan(0);
      expect(inPersonal.every((r) => r.repoId === 4 || r.repoId === 5)).toBe(true);
    });

    it("returns newest first and honours the limit", () => {
      const rows = fresh().activityList({ ...NO_FILTER, limit: 3 });
      expect(rows).toHaveLength(3);
      expect(rows[0].timestamp).toBeGreaterThanOrEqual(rows[1].timestamp);
      expect(rows[1].timestamp).toBeGreaterThanOrEqual(rows[2].timestamp);
    });

    it("is empty in the empty scenario", () => {
      expect(fresh("empty").activityList(NO_FILTER)).toEqual([]);
    });
  });

  describe("summary_today", () => {
    it("scopes every count and list to the group", () => {
      const h = fresh();
      const all = h.summaryToday(null);
      const personal = h.summaryToday(2);
      expect(all.attentionCount).toBe(2);
      expect(personal.attentionCount).toBe(2);
      expect(personal.updatedCount).toBe(0);
      expect(personal.releasesCount).toBe(0);
      const work = h.summaryToday(1);
      expect(work.attentionCount).toBe(0);
      expect(work.updatedCount).toBe(1);
    });

    it("counts only enabled, in-scope repositories that had nothing happen", () => {
      expect(fresh().summaryToday(null).noChangeCount).toBe(4);
    });
  });

  it("shows a mutation on the next read", () => {
    const h = fresh();
    h.repoSetEnabled(1, false);
    expect(h.repoList({ enabledOnly: true, hostType: null, query: null }).some((r) => r.id === 1)).toBe(false);
    h.groupAssign(1, 2);
    expect(h.groupsForRepo(1)).toEqual([1, 2]);
    expect(h.groupList().find((g) => g.id === 2)?.repoCount).toBe(3);
  });
});
