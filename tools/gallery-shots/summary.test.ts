import { describe, expect, it } from "vitest";
import {
  buildSummary,
  classifyShot,
  formatPercent,
  parseShotName,
  shotName,
  type ShotResult,
} from "./summary.js";

describe("shot names", () => {
  it("round-trips a section and theme", () => {
    expect(parseShotName(shotName("screen-shell", "dark"))).toEqual({
      section: "screen-shell",
      theme: "dark",
    });
  });

  it("rejects a file that does not follow the pattern", () => {
    expect(parseShotName("notes.txt")).toBeNull();
    expect(parseShotName("buttons__sepia.png")).toBeNull();
  });
});

describe("classifyShot", () => {
  const size = { width: 10, height: 10 };

  it("reports identical shots as same", () => {
    expect(classifyShot(size, size, 0)).toEqual({ status: "same", changed: 0, perceptual: 0, total: 100 });
  });

  it("reports differing pixels as changed", () => {
    expect(classifyShot(size, size, 7)).toEqual({ status: "changed", changed: 7, perceptual: 7, total: 100 });
  });

  it("treats a difference at or under the noise floor as noise", () => {
    expect(classifyShot(size, size, 1).status).toBe("noise");
    expect(classifyShot(size, size, 4).status).toBe("noise");
    expect(classifyShot(size, size, 5).status).toBe("changed");
  });

  it("flags a uniform palette change that the perceptual count cannot see", () => {
    // All #ffffff vs all #ebebeb: 0 perceptual differences, every pixel differs raw.
    const result = classifyShot(size, size, 100, undefined, 0);
    expect(result.status).toBe("changed");
    expect(result.changed).toBe(100);
    expect(result.perceptual).toBe(0);
  });

  it("applies the noise floor to the raw count", () => {
    expect(classifyShot(size, size, 3, undefined, 0).status).toBe("noise");
    expect(classifyShot(size, size, 5, undefined, 0).status).toBe("changed");
  });

  it("reports a size mismatch instead of a pixel count", () => {
    const result = classifyShot(size, { width: 10, height: 12 }, 99);
    expect(result.status).toBe("size-changed");
    expect(result.changed).toBe(0);
    expect(result.baseSize).toBe("10x10");
    expect(result.headSize).toBe("10x12");
  });

  it("reports a missing side as added or removed", () => {
    expect(classifyShot(null, size).status).toBe("added");
    expect(classifyShot(size, null).status).toBe("removed");
  });

  it("refuses to classify nothing", () => {
    expect(() => classifyShot(null, null)).toThrow();
  });
});

describe("formatPercent", () => {
  it("formats two decimals", () => {
    expect(formatPercent(1, 8)).toBe("12.50%");
  });
  it("does not divide by zero", () => {
    expect(formatPercent(0, 0)).toBe("n/a");
  });
});

describe("buildSummary", () => {
  const row = (over: Partial<ShotResult>): ShotResult => ({
    section: "buttons",
    theme: "light",
    status: "same",
    changed: 0,
    total: 1000,
    ...over,
  });

  it("says so when nothing differs, without a table", () => {
    const md = buildSummary([row({}), row({ theme: "dark" })]);
    expect(md).toContain("No pixel differences in 2 screenshots.");
    expect(md).not.toContain("| Section |");
  });

  it("counts noise without listing it", () => {
    const md = buildSummary([row({ status: "noise", changed: 1 }), row({ section: "x" })]);
    expect(md).toContain("No pixel differences in 2 screenshots.");
    expect(md).toContain("1 more differ by 4 pixels or fewer");
    expect(md).not.toContain("| buttons |");
  });

  it("shows the perceptual count only when it differs from the raw count", () => {
    const md = buildSummary([
      row({ section: "palette", status: "changed", changed: 1000, perceptual: 0 }),
      row({ section: "same-count", status: "changed", changed: 50, perceptual: 50 }),
    ]);
    expect(md).toContain("| palette | light | changed | 1000 (0 perceptual) | 100.00% |");
    expect(md).toContain("| same-count | light | changed | 50 | 5.00% |");
  });

  it("lists only the shots that differ and counts the rest", () => {
    const md = buildSummary([
      row({ section: "cards" }),
      row({ section: "tabs", status: "changed", changed: 50, total: 1000 }),
    ]);
    expect(md).toContain("1 of 2 screenshots differ");
    expect(md).toContain("(1 unchanged)");
    expect(md).toContain("| tabs | light | changed | 50 | 5.00% |");
    expect(md).not.toContain("| cards |");
  });

  it("puts size changes first, then the biggest pixel change first", () => {
    const md = buildSummary([
      row({ section: "a", status: "changed", changed: 5 }),
      row({ section: "b", status: "changed", changed: 500 }),
      row({ section: "c", status: "size-changed", baseSize: "1x1", headSize: "2x2" }),
      row({ section: "d", status: "added" }),
      row({ section: "e", status: "removed" }),
    ]);
    const order = ["| c |", "| b |", "| a |", "| d |", "| e |"].map((s) => md.indexOf(s));
    expect(order.every((i) => i > 0)).toBe(true);
    expect(order).toEqual([...order].sort((x, y) => x - y));
    expect(md).toContain("size changed (1x1 to 2x2)");
  });
});
