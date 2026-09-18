/// <reference types="node" />
import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import {
  AA_NON_TEXT,
  AA_TEXT,
  contrast,
  contrastOverWash,
  parseTokens,
  type Oklch,
} from "@/lib/contrast";

/**
 * The colour gate.
 *
 * Three shipped accessibility failures have been found in three sessions, all
 * the same shape: a token pair that a code comment reasoned about and nobody
 * computed. D-1 (the activity receipt chip), D-14 (the active group row) and
 * D-17 (the Activity "Failed" filter) each passed every test in this suite and
 * every review, because nothing here knew what a colour was.
 *
 * WHAT THIS CATCHES: a token whose value moves so that a pair it participates
 * in drops under its floor. That is the actual failure mode all three had -
 * two were pairs that were wrong from the day they were written, and the third
 * got worse when a background changed underneath it.
 *
 * WHAT THIS CANNOT CATCH, and it matters: the pair list below is written by
 * hand. A NEW pair introduced in a component is invisible to this file until
 * someone adds it here. This is a registry of what we have decided to keep
 * honest, not a scan of what the app actually paints - jsdom computes no
 * colours, so a real scan would need a browser. Adding a `text-*` on a new
 * `bg-*` means adding a row here in the same change.
 *
 * Ratios are computed from `src/index.css` itself rather than from constants,
 * so editing a token is what makes this fail. Alpha is composited in
 * gamma-encoded sRGB, the way CSS does it.
 */

/**
 * Read from disk rather than through Vite's `?raw`. A `?raw` import of a CSS
 * file still goes through the Tailwind pipeline, so what comes back is the
 * PROCESSED stylesheet, not the token source - the `:root` block this gate
 * exists to read is not in it. `@types/node` is already present (vite and
 * vitest pull it in), referenced above so this one file can see it without
 * widening the app's own tsconfig.
 */
const CSS = readFileSync(new URL("../index.css", import.meta.url), "utf8");
const THEMES = {
  light: parseTokens(CSS, ":root"),
  dark: parseTokens(CSS, ".dark"),
} as const;

type Pair = {
  what: string;
  ink: string;
  on: string;
  /** A translucent wash of this token, painted over `on`, under the ink. */
  wash?: { token: string; alpha: number };
  /** Text unless stated: an icon, bar, dot or fill gets the 3:1 floor. */
  floor?: number;
};

/**
 * Every pair that ships, with where it lives. Keep the `where` accurate: it is
 * what makes a failure here actionable rather than a puzzle.
 */
const PAIRS: Pair[] = [
  // --- body and surfaces ---------------------------------------------------
  { what: "body text on the page", ink: "foreground", on: "background" },
  { what: "body text on a card", ink: "foreground", on: "card" },
  { what: "muted text on the page", ink: "muted-foreground", on: "background" },
  { what: "muted text on a card", ink: "muted-foreground", on: "card" },
  { what: "muted text in a well", ink: "muted-foreground", on: "muted" },
  { what: "sidebar text", ink: "sidebar-foreground", on: "sidebar" },

  // --- the accent ----------------------------------------------------------
  // `--primary` is tuned to sit BEHIND white text, so it is only ever a fill.
  // `--primary-ink` is the same hue solved for use AS text (D-1, D-14).
  { what: "white on a primary button", ink: "primary-foreground", on: "primary" },
  { what: "accent text on the page", ink: "primary-ink", on: "background" },
  { what: "accent text on a card", ink: "primary-ink", on: "card" },
  { what: "accent text on the sidebar", ink: "primary-ink", on: "sidebar" },
  {
    what: "the engaged group row (D-14)",
    ink: "primary-ink",
    on: "sidebar",
    wash: { token: "primary", alpha: 0.1 },
  },
  {
    what: "an engaged filter chip (C1)",
    ink: "primary-ink",
    on: "background",
    wash: { token: "primary", alpha: 0.15 },
  },

  // --- the sidebar set (SB3, SB4, L4) --------------------------------------
  { what: "the add-repo button's label (L4)", ink: "background", on: "foreground" },
  {
    what: "the add-repo button's fill against the sidebar (L4)",
    ink: "foreground",
    on: "sidebar",
    floor: AA_NON_TEXT,
  },
  {
    what: "the attention dot against the sidebar (SB3)",
    ink: "status-dirty",
    on: "sidebar",
    floor: AA_NON_TEXT,
  },
  { what: "the active nav item's label", ink: "foreground", on: "sidebar-accent" },

  // --- status inks, as TEXT on every surface they land on ------------------
  // The tone on a filter chip (C1) put these on `--muted`, which is how D-17
  // was found. `--destructive` is deliberately absent: it has no consumer.
  ...(
    [
      "status-sync",
      "status-behind",
      "status-dirty",
      "status-failed",
      "status-paused",
      "status-no-upstream",
    ] as const
  ).flatMap((ink) =>
    (["background", "card", "muted"] as const).map((on) => ({
      what: `${ink} as text on ${on}`,
      ink,
      on,
    })),
  ),

  // --- status chips: the ink/tint pairs StatusBadge actually paints --------
  ...(
    ["sync", "ahead", "behind", "dirty", "failed", "paused", "no-upstream"] as const
  ).map((s) => ({
    what: `the ${s} status chip`,
    ink: `status-${s}-ink`,
    on: `status-${s}-tint`,
  })),
];

function resolve(theme: keyof typeof THEMES, token: string): Oklch {
  const value = THEMES[theme].get(token);
  if (value === undefined) {
    throw new Error(
      `--${token} is not an opaque oklch token in the "${theme}" block of src/index.css`,
    );
  }
  return value;
}

describe("every shipped colour pair clears its WCAG floor", () => {
  for (const theme of ["light", "dark"] as const) {
    describe(theme, () => {
      for (const pair of PAIRS) {
        const floor = pair.floor ?? AA_TEXT;
        it(`${pair.what}: ${pair.ink} on ${pair.on}`, () => {
          const ink = resolve(theme, pair.ink);
          const surface = resolve(theme, pair.on);
          const ratio = pair.wash
            ? contrastOverWash(
                ink,
                resolve(theme, pair.wash.token),
                pair.wash.alpha,
                surface,
              )
            : contrast(ink, surface);

          // The received value is in the message on failure, so a broken pair
          // reports its actual ratio rather than just "false".
          expect(
            Number(ratio.toFixed(2)),
            `${pair.what} measures ${ratio.toFixed(2)}:1 in ${theme}, under the ${floor}:1 floor`,
          ).toBeGreaterThanOrEqual(floor);
        });
      }
    });
  }
});

describe("the measuring code itself", () => {
  // Without these the gate could pass by being wrong in a consistent
  // direction, which is worse than not having it.
  it("agrees with the WCAG definition at the extremes", () => {
    const white: Oklch = { L: 1, C: 0, H: 0 };
    const black: Oklch = { L: 0, C: 0, H: 0 };

    expect(contrast(white, black)).toBeCloseTo(21, 1);
    expect(contrast(white, white)).toBeCloseTo(1, 5);
  });

  it("reproduces the three ratios that caught real defects", () => {
    // D-14: accent text on the /10 wash over the light sidebar was 4.27:1
    // with `--primary`, which is why `--primary-ink` exists. Pinning the
    // REJECTED value proves the gate would have failed the old code.
    const light = THEMES.light;
    const failed = contrastOverWash(
      light.get("primary")!,
      light.get("primary")!,
      0.1,
      light.get("sidebar")!,
    );
    expect(failed).toBeLessThan(AA_TEXT);
    expect(Number(failed.toFixed(2))).toBe(4.27);

    // D-17: `--destructive` as text on the page, 4.34:1, under the floor.
    // The token is still declared, so this stays measurable; if anyone
    // adopts it for text again, the number is on the record here.
    const destructive = contrast(light.get("destructive")!, light.get("background")!);
    expect(Number(destructive.toFixed(2))).toBe(4.34);
  });

  it("composites alpha in gamma-encoded sRGB, not linear light", () => {
    // A 50% white wash over black lands near the sRGB midpoint (~0.216
    // relative luminance), not at 0.5. Compositing in linear light instead
    // would put it at 0.5 and quietly inflate every washed ratio.
    const light = THEMES.light;
    const halfOverBlack = contrastOverWash(
      light.get("foreground")!,
      { L: 1, C: 0, H: 0 },
      0.5,
      { L: 0, C: 0, H: 0 },
    );
    expect(halfOverBlack).toBeGreaterThan(1);
    expect(halfOverBlack).toBeLessThan(5);
  });

  it("reads both theme blocks out of the real stylesheet", () => {
    // A parser that silently returned nothing would make every pair above
    // throw rather than pass, but this says so directly.
    expect(THEMES.light.size).toBeGreaterThan(30);
    expect(THEMES.dark.size).toBeGreaterThan(30);
    expect(THEMES.light.get("background")).toEqual({ L: 0.968, C: 0, H: 0 });
    expect(THEMES.dark.get("background")).toEqual({ L: 0.145, C: 0, H: 0 });
  });
});
