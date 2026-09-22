import { describe, expect, it } from "vitest";

/**
 * STANDARD ENFORCED HERE: the Repos table's column set matches the settled
 * values below, or the departure is recorded here with a reason.
 *
 * WHY THIS EXISTS. On 2026-09-22 the maintainer looked at a bench drawing of
 * this table and said it was "not following the many rules we have established
 * multiple times about certain column widths / styling, label & content
 * alignment + structure + functionality". Checking that took reading three
 * documents, and they did not agree with each other:
 *
 *   - `2026-08-28_iterations/README.md`, which the decision register names as
 *     "the last word on table values", says in prose: fixed widths for every
 *     column except Repository, and a data icon on every column.
 *   - `_generators/gen_lab2.py`, which BUILT the lab page that README
 *     describes, gives Folder a `minmax(150px,240px)` range and gives a
 *     generic data icon to only six of the ten columns.
 *   - `repos.tsx` follows the generator, and says so in a comment that had
 *     been sitting unread next to the Folder column since it shipped.
 *
 * So the question "is this table correct?" had three answers depending on which
 * file you opened, and the only person who noticed was the one person the whole
 * process exists to spare. A prose rule in a document nobody diffs cannot
 * enforce anything. This table can.
 *
 * WHEN THIS FAILS, there are exactly three honest fixes:
 *   1. Put the column back to the settled value - the change was a mistake.
 *   2. Change the value here, in the same commit, with the reason in `why`.
 *      That is what re-deciding looks like; it is cheap and it is visible.
 *   3. Mark the row `contested` with both readings, if the settled record
 *      genuinely disagrees with itself and only the maintainer can resolve it.
 * Loosening the matcher until it goes green is not on the list.
 *
 * Files are read through Vite's `import.meta.glob`, NOT `node:fs`, for the same
 * reason `ui-reachability.test.ts` gives: `tsconfig.json` deliberately denies
 * `src/` any Node types, and widening that boundary for every file in the
 * project to buy one test a convenience is a bad trade.
 */

/** A column's settled presentation, and where that value comes from. */
type SettledColumn = {
  /** The column id as `repos.tsx` declares it. */
  id: string;
  /** The exact `width` string. */
  width: string;
  /** Whether the column passes a generic `icon:` to `DataTable`. */
  icon: boolean;
  /** Where this value was settled, and why it is what it is. */
  why: string;
  /**
   * Set when the settled RECORD disagrees with itself, so this row is a
   * placeholder for a decision rather than a decision. The test still enforces
   * the current value - a contested row is not an unguarded one - but the
   * disagreement is named here so it cannot be lost.
   */
  contested?: string;
};

/**
 * The settled column set, in the ratified ORDER: Repository first and frozen,
 * then Status, Branch, Ahead, Behind, Groups, Folder, Checked, Stars, Forks.
 * (Register ledger B5 and the lab README's settled list. Note this is NOT the
 * lab generator's own COLS order, which puts Folder third; the register's
 * ratified order post-dates it and wins.)
 */
const SETTLED: SettledColumn[] = [
  {
    id: "repo",
    width: "minmax(180px,240px)",
    icon: false,
    why: "First and frozen. A repository name is the one value whose width is not knowable, so it is a range. The lab draws link glyphs in this cell rather than a generic data icon.",
  },
  {
    id: "status",
    width: "124px",
    icon: false,
    why: "Lab COLS. The cell is a filled status chip carrying its own glyph, so a second generic icon beside it would be noise.",
  },
  {
    id: "branch",
    width: "116px",
    icon: true,
    why: "Lab COLS says 104px; widened to 116px because RR6's longest label ('no commits') wrapped onto two lines inside a fixed 52px row. Caught in a screenshot - nothing in jsdom measures text. A deliberate, recorded departure.",
  },
  {
    id: "ahead",
    width: "64px",
    icon: true,
    why: "Lab COLS. Number column: label left in the header, value right in the cell.",
  },
  {
    id: "behind",
    width: "64px",
    icon: true,
    why: "Lab COLS. Number column, paired with Ahead.",
  },
  {
    id: "groups",
    width: "160px",
    icon: false,
    why: "Lab COLS. The cell draws coloured group dots, its own treatment.",
  },
  {
    id: "folder",
    width: "minmax(150px,240px)",
    icon: false,
    why: "T1 RESOLVED by the maintainer 2026-09-22: the range stays. A file path is the second value whose width is not knowable, and clamping it truncates paths that would have fit. The cost he accepted is that Folder's edge moves with content, so it will not line up with a Folder column in another table. The cell draws its own inline folder glyph rather than the generic data-icon slot.",
    contested:
      "T2 only - the ICON slot, not the width. Register section L.2a. T1 (the width) was resolved 2026-09-22: the range stays. T2 is open because the maintainer asked to SEE both readings before choosing: Reading A, every column opens with the same generic muted icon for a uniform header rhythm; Reading B (what this row enforces), these four already draw a richer mark in the cell - link glyphs, the status chip, group dots, a folder mark - so a second generic icon beside them is noise. Applies equally to `repo`, `status` and `groups`, which is why resolving it changes four rows, not one.",
  },
  {
    id: "checked",
    width: "96px",
    icon: true,
    why: "Lab COLS. Muted metadata cell.",
  },
  {
    id: "stars",
    width: "76px",
    icon: true,
    why: "Lab COLS `(\"stars\",\"Stars\",\"76px\",\"n\",1,1)`. Defaults ON.",
  },
  {
    id: "forks",
    width: "76px",
    icon: true,
    why: "Lab COLS `(\"forks\",\"Forks\",\"76px\",\"n\",1,1)`. Defaults ON.",
  },
];

/**
 * Columns the lab set defines that the table deliberately does NOT render, so a
 * reader cannot mistake their absence for an oversight.
 */
const DELIBERATELY_NOT_RENDERED: Record<string, string> = {
  license: "Lab default OFF (`0`). Column show/hide is itself still deferred, so the lab's own default set decides.",
  size: "Lab default OFF (`0`).",
  vis: "Lab default OFF (`0`), and the field is `visibility` on the wire type.",
  commit: "Lab default OFF (`0`). `lastLocalCommitAt` exists on RepoSummary but no column was ratified for it.",
  prs: "Lab default OFF (`0`).",
  release: "Lab default OFF (`0`).",
};

const SOURCES = import.meta.glob("/src/**/*.{ts,tsx}", {
  query: "?raw",
  import: "default",
  eager: true,
}) as Record<string, string>;

type ParsedColumn = { id: string; width: string; icon: boolean };

/**
 * Pull each column's id, width and whether it declares a generic icon out of
 * the `columns` array literal in `repos.tsx`.
 *
 * Text parsing rather than importing the array, because the array is built
 * inside a `useMemo` that closes over this screen's event handlers - it does not
 * exist outside a render. The alternative, hoisting a static spec out of the
 * component, is a real refactor and a reasonable future move; this test does not
 * require it and should not force it.
 */
function parseColumns(): ParsedColumn[] {
  const raw = SOURCES["/src/screens/repos.tsx"];
  expect(raw, "repos.tsx was not picked up by the glob").toBeTruthy();

  // Normalise line endings before matching. Found the hard way: a tool rewrote
  // this file with CRLF, every `,\n` in the patterns below stopped matching, and
  // the gate reported ZERO columns - which reads as "the array's shape changed"
  // when nothing about the array had changed at all. A gate whose answer depends
  // on line endings is worse than no gate, because its failure looks like a real
  // finding.
  const src = raw.replace(/\r\n/g, "\n");

  const out: ParsedColumn[] = [];
  // Each entry opens with `id: "<name>",` and the block runs to the next one.
  const starts = [...src.matchAll(/\n\s+id: "(\w+)",\n\s+header:/g)];
  for (let i = 0; i < starts.length; i++) {
    const id = starts[i][1];
    const from = starts[i].index ?? 0;
    const to = i + 1 < starts.length ? (starts[i + 1].index ?? src.length) : src.length;
    const block = src.slice(from, to);
    const width = /width: "([^"]+)"/.exec(block)?.[1] ?? "";
    // Only the generic slot counts. An icon rendered inside `cell:` is the
    // column drawing its own thing, which is exactly the distinction the four
    // icon-less columns rely on.
    const beforeCell = block.split("cell:")[0];
    out.push({ id, width, icon: /\n\s+icon: \w/.test(beforeCell) });
  }
  return out;
}

describe("the Repos table matches its settled column values", () => {
  it("parses the column set at all, so a shape change cannot make this vacuous", () => {
    const parsed = parseColumns();
    // Guards the matcher itself. If `repos.tsx` restructures its column array,
    // this fires instead of the suite silently proving nothing about zero
    // columns - the failure mode that let `bench-audit.py` report success over
    // six files while eleven existed.
    expect(
      parsed.length,
      "the column parser found a different number of columns than the settled set. " +
        "Either a column was added or removed (update SETTLED below, with a reason), " +
        "or the array's shape changed and this parser needs updating.",
    ).toBe(SETTLED.length);
  });

  it("renders the settled columns, in the ratified order", () => {
    expect(parseColumns().map((c) => c.id)).toEqual(SETTLED.map((c) => c.id));
  });

  it.each(SETTLED)("$id is $width", ({ id, width, why }) => {
    const col = parseColumns().find((c) => c.id === id);
    expect(col, `column ${id} is no longer rendered`).toBeDefined();
    expect(
      col?.width,
      `${id}'s width departs from the settled value.\n  settled: ${width}\n  why:     ${why}\n\n` +
        `Put it back, or change the value in SETTLED in this same commit with the new reason.`,
    ).toBe(width);
  });

  it.each(SETTLED)("$id " + "icon slot", ({ id, icon, why }) => {
    const col = parseColumns().find((c) => c.id === id);
    expect(col, `column ${id} is no longer rendered`).toBeDefined();
    expect(
      col?.icon,
      `${id} ${icon ? "should carry" : "should NOT carry"} a generic data icon.\n  why: ${why}\n\n` +
        `The four columns without one draw their own richer glyph in the cell instead ` +
        `(link marks, the status chip, a folder mark, group dots). Adding a generic icon ` +
        `beside those is the change this guards against.`,
    ).toBe(icon);
  });

  it("names every contested value, so a disagreement in the record cannot be lost", () => {
    // A contested row is NOT an unguarded one: the assertions above still hold it
    // to its current value. This only proves the disagreement stays visible until
    // someone resolves it, rather than decaying into "the way it has always been".
    const contested = SETTLED.filter((c) => c.contested);
    for (const c of contested) {
      expect(c.contested, `${c.id} is marked contested with no explanation`).toMatch(/Reading A/);
    }
    // If this number goes DOWN, a decision was made and should have been recorded
    // in the register too. If it goes UP, a new disagreement was found. Either way
    // the change should be deliberate.
    expect(
      contested.map((c) => c.id),
      "the set of contested column values changed. Update register section L.2a in the same change.",
    ).toEqual(["folder"]);
  });

  it("carries no stale entries in the not-rendered list", () => {
    // An allowlist that outlives its reason reads as a deliberate decision when
    // it is actually a leftover - the same rule `ui-reachability.test.ts` applies
    // to its own.
    const rendered = new Set(parseColumns().map((c) => c.id));
    for (const id of Object.keys(DELIBERATELY_NOT_RENDERED)) {
      expect(
        rendered.has(id),
        `${id} is listed as deliberately not rendered, but the table now renders it. ` +
          `Move it into SETTLED with its settled width and icon, and delete the entry.`,
      ).toBe(false);
    }
  });
});
