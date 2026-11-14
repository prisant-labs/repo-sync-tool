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
