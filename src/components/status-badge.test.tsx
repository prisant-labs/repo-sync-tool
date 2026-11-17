// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { StatusBadge } from "./status-badge";

afterEach(cleanup);

describe("StatusBadge count", () => {
  it("abbreviates a very large behind count and keeps the exact number in the title", () => {
    render(<StatusBadge status="behind" count={98767} />);
    const chip = screen.getByText("98k+ behind");
    expect(chip.getAttribute("title")).toBe("98,767 commits behind");
    expect(screen.queryByText(/98767/)).toBeNull();
  });

  it("abbreviates a very large ahead count the same way", () => {
    render(<StatusBadge status="ahead" count={12345} />);
    expect(screen.getByText("12k+ ahead").getAttribute("title")).toBe("12,345 commits ahead");
  });

  it("leaves a small count unchanged and adds no title", () => {
    render(<StatusBadge status="behind" count={999} />);
    expect(screen.getByText("999 behind").hasAttribute("title")).toBe(false);
  });
});
