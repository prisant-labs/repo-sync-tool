import { clsx, type ClassValue } from "clsx";
import { extendTailwindMerge } from "tailwind-merge";

// Every `--text-<name>` size that `src/index.css` defines for the app's type
// ramp. tailwind-merge does not read the stylesheet, and it treats an unknown
// `text-*` class as a text COLOUR. Without this list,
// `cn("text-meta text-muted-foreground")` returned only
// `"text-muted-foreground"` and the size was silently dropped. Registering the
// names in tailwind-merge's `text` theme scale makes it treat them as font
// sizes, so they merge with `text-xs` and friends and never with a colour.
// `utils.test.ts` fails when this list and the stylesheet disagree.
export const FONT_SIZE_TOKENS = [
  "page",
  "identity",
  "body",
  "cell",
  "meta",
  "micro",
  "heading",
  "title",
  "label",
] as const;

const twMerge = extendTailwindMerge({
  extend: {
    theme: {
      text: [...FONT_SIZE_TOKENS],
    },
  },
});

export function cn(...inputs: ClassValue[]) {
  return twMerge(clsx(inputs));
}

// The font-size names declared in the `@theme` blocks of a stylesheet. Pure, so
// the drift test can feed it altered CSS. It strips comments first (a
// commented-out declaration is not live), reads only inside `@theme { }` and
// `@theme inline { }` blocks, accepts hyphenated names such as `table-heading`,
// and skips modifier keys, which have the shape `--text-<name>--<modifier>`
// (`--line-height`, `--font-weight`, `--letter-spacing`).
export function themeFontSizeNames(css: string): string[] {
  const stripped = css.replace(/\/\*[\s\S]*?\*\//g, "");
  const names = new Set<string>();
  for (const open of stripped.matchAll(/@theme\b[^{]*\{/g)) {
    let depth = 1;
    let i = open.index + open[0].length;
    const start = i;
    while (i < stripped.length && depth > 0) {
      if (stripped[i] === "{") depth += 1;
      else if (stripped[i] === "}") depth -= 1;
      i += 1;
    }
    const body = stripped.slice(start, i - 1);
    for (const decl of body.matchAll(/--text-([a-z0-9-]+?)\s*:/g)) {
      if (!decl[1].includes("--")) names.add(decl[1]);
    }
  }
  return [...names].sort();
}
