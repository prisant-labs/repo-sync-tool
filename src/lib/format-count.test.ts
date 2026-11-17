import { describe, expect, it } from "vitest";
import { countTitle, formatCount } from "./format-count";

describe("formatCount", () => {
  it.each([
    [0, "0"],
    [1, "1"],
    [999, "999"],
    [1000, "1k+"],
    [1999, "1k+"],
    [9999, "9k+"],
    [10000, "10k+"],
    [98767, "98k+"],
    [999999, "999k+"],
    [1_000_000, "1M+"],
    [123_456_789, "123M+"],
  ])("formats %i as %s", (n, expected) => {
    expect(formatCount(n)).toBe(expected);
  });
});

describe("countTitle", () => {
  it("gives the exact number when abbreviated", () => {
    expect(countTitle(98767, "behind")).toBe("98,767 commits behind");
    expect(countTitle(1000, "ahead")).toBe("1,000 commits ahead");
  });
  it("gives nothing when the count is shown exactly", () => {
    expect(countTitle(999, "behind")).toBeUndefined();
    expect(countTitle(0, "ahead")).toBeUndefined();
  });
});
