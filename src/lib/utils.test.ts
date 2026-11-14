import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { cn, FONT_SIZE_TOKENS, themeFontSizeNames } from "./utils";

function sizesDefinedInCss(): string[] {
  const css = readFileSync(
    fileURLToPath(new URL("../index.css", import.meta.url)),
    "utf8",
  );
  return themeFontSizeNames(css);
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
    expect(cn("text-xs text-meta")).toBe("text-meta");
    expect(cn("text-meta text-sm")).toBe("text-sm");
  });

  it("still lets a later colour replace an earlier colour", () => {
    expect(cn("text-foreground text-muted-foreground")).toBe(
      "text-muted-foreground",
    );
  });
});

describe("themeFontSizeNames()", () => {
  const base = "@theme { --text-meta: 0.6875rem; --text-page: 1.5rem; }";

  it("reads the live declarations", () => {
    expect(themeFontSizeNames(base)).toEqual(["meta", "page"]);
  });

  it("does not count a commented-out declaration", () => {
    const css = "@theme { /* --text-meta: 0.6875rem; */ --text-page: 1.5rem; }";
    expect(themeFontSizeNames(css)).toEqual(["page"]);
  });

  it("reports an unregistered hyphenated token", () => {
    const css = base.replace("}", "--text-table-heading: 0.75rem; }");
    const found = themeFontSizeNames(css);
    expect(found).toContain("table-heading");
    expect(found.filter((n) => !FONT_SIZE_TOKENS.includes(n as never))).toEqual([
      "table-heading",
    ]);
  });

  it("does not count modifier keys as sizes", () => {
    const css =
      "@theme { --text-meta: 1rem; --text-meta--line-height: 1.2; " +
      "--text-meta--font-weight: 500; --text-meta--letter-spacing: 0.02em; }";
    expect(themeFontSizeNames(css)).toEqual(["meta"]);
  });

  it("ignores declarations outside an @theme block", () => {
    expect(themeFontSizeNames(":root { --text-meta: 1rem; }")).toEqual([]);
  });
});
