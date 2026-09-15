import { describe, expect, it } from "vitest";

/**
 * STANDARD ENFORCED HERE: a capability the backend exposes is either reachable
 * from the interface, or it is on the list below with a reason.
 *
 * This is the machine-checked half of the rule the coverage matrix states in
 * prose - "designs forward, but must lose nothing". Prose cannot catch a
 * redesign that quietly drops a button, because the dropped button leaves no
 * trace: the command still compiles, every other test still passes, and the
 * only evidence is a control that is no longer on screen. It has nearly
 * happened twice. The 2026-09-14 header-row redesign dropped Check now,
 * Refresh metadata and Editor, and was caught by one person reading carefully
 * rather than by anything automatic.
 *
 * WHEN THIS FAILS, there are exactly three honest fixes:
 *   1. Wire the command up - the redesign dropped something it should not have.
 *   2. Add it below with a reason - it is deliberately unreachable.
 *   3. Delete the command from the backend - nothing needs it.
 * Widening the matcher until the test goes green is not on the list.
 *
 * Files are read through Vite's `import.meta.glob`, NOT `node:fs`, on purpose:
 * `tsconfig.json` deliberately gives `src/` no Node types, so a component
 * cannot reach for the filesystem by accident. Importing `node:fs` here would
 * have meant widening that boundary for every file in the project to buy one
 * test a convenience.
 */

/** Commands with no caller in the interface, each with the reason it is allowed. */
const DELIBERATELY_UNREACHABLE: Record<string, string> = {
  groupRename:
    "Superseded by groupUpdate (name + colour in one atomic edit), which is what the UI calls. " +
    "Kept registered for E-06 additive-compatibility: the IPC contract does not remove commands. " +
    "See the comment where it is registered in src-tauri/src/lib.rs.",
  summaryWeek:
    "A V1.1 stub. The backend returns not_implemented() and there is nothing to render, so " +
    "reaching it from the UI would mean a button that always errors.",
};

/** Every non-test source file under `src/`, as raw text, keyed by path. */
const SOURCES = import.meta.glob("/src/**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

/** `bindings.ts` is the declaration site, not a call site. Tests prove nothing about a USER's reach. */
function callSites(): string {
  return Object.entries(SOURCES)
    .filter(([path]) => !/\.test\.tsx?$/.test(path) && !path.endsWith("/bindings.ts"))
    .map(([, text]) => text)
    .join("\n");
}

function exposedCommands(): string[] {
  const bindings = SOURCES["/src/lib/bindings.ts"];
  expect(bindings, "bindings.ts was not picked up by the glob").toBeTruthy();
  return [...bindings.matchAll(/^\s*(\w+):\s*\([^)]*\)\s*=>\s*typedError/gm)].map((m) => m[1]);
}

describe("every backend capability is reachable from the interface", () => {
  it("has a caller in src/, or a recorded reason it does not", () => {
    const exposed = exposedCommands();
    // Guards the matcher itself: a bindings regeneration that changed shape
    // would otherwise make this test vacuously pass by finding no commands.
    expect(exposed.length).toBeGreaterThan(30);

    const code = callSites();
    const undeclared = exposed.filter(
      (c) => !code.includes(`commands.${c}`) && !(c in DELIBERATELY_UNREACHABLE),
    );

    expect(
      undeclared,
      `These backend commands have no caller anywhere in the interface:\n` +
        undeclared.map((c) => `  - ${c}`).join("\n") +
        `\n\nEither wire each one up, or add it to DELIBERATELY_UNREACHABLE with the reason. ` +
        `If a redesign just moved buttons around, this is the control it dropped.`,
    ).toEqual([]);
  });

  it("carries no stale entries in the allowlist", () => {
    // An allowlist that outlives its reason is worse than no allowlist: it
    // reads as a deliberate decision when it is actually a leftover.
    const exposed = exposedCommands();
    const code = callSites();

    for (const name of Object.keys(DELIBERATELY_UNREACHABLE)) {
      expect(exposed, `${name} is allowlisted but no longer exists in bindings.ts`).toContain(name);
      expect(
        code.includes(`commands.${name}`),
        `${name} is allowlisted as unreachable, but the interface now calls it. Remove the entry.`,
      ).toBe(false);
    }
  });
});
