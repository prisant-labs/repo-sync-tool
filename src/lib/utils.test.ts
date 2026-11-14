import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { cn, FONT_SIZE_TOKENS } from "./utils";

// The font-size keys that `src/index.css` defines, read from the file itself so
// that a size added to the stylesheet cannot be forgotten by `cn()`.
function sizesDefinedInCss(): string[] {
  const css = readFileSync(
    fileURLToPath(new URL("../index.css", import.meta.url)),
    "utf8",
  );
  const keys = new Set<string>();
  for (const match of css.matchAll(/--text-([a-z0-9]+)\s*:/g)) {
    keys.add(match[1]);
  }
  return [...keys].sort();
}

describe("cn() with the custom type sizes", () => {
  it("registers every --text-* size that index.css defines", () => {
    expect([...FONT_SIZE_TOKENS].sort()).toEqual(sizesDefinedInCss());
  });

  // tailwind-merge reads an unknown `text-*` class as a colour, so before the
  // fix it dropped the size whenever a colour class sat beside it.
  for (const size of sizesDefinedInCss()) {
    it(`keeps text-${size} beside a colour class, in both orders`, () => {
      expect(cn(`text-${size} text-muted-foreground`)).toBe(
        `text-${size} text-muted-foreground`,
      );
      expect(cn(`text-muted-foreground text-${size}`)).toBe(
        `text-muted-foreground text-${size}`,
      );
    });
  }

  it("still lets a later size replace an earlier size", () => {
    // This test is ABOUT raw sizes merging with named ones, so it must name them.
    /* eslint-disable local/no-raw-text-size */
    expect(cn("text-xs text-meta")).toBe("text-meta");
    expect(cn("text-meta text-sm")).toBe("text-sm");
    /* eslint-enable local/no-raw-text-size */
  });

  it("still lets a later colour replace an earlier colour", () => {
    expect(cn("text-foreground text-muted-foreground")).toBe(
      "text-muted-foreground",
    );
  });
});
