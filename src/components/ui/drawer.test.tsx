// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import { Drawer } from "@/components/ui/drawer";

/**
 * Unit coverage for the `Drawer` primitive's `size` variant and accessible-
 * name props (Codex adversarial review of PR #77, findings 1 and 3).
 *
 * Finding 1 (confirmed): the first N4 cut widened this primitive to ~66vw
 * unconditionally, which silently widened every consumer, not only the
 * repo detail panel the ratification named - including the Activity
 * screen's own receipt drawer and the repo detail panel's nested receipt.
 * `size` defaults to `"default"` (the original fixed 480px); only the two
 * detail-panel call sites (`screens/repos.tsx`, `screens/dashboard.tsx`)
 * request `"wide"`. jsdom computes no layout, so this pins the exact
 * Tailwind CLASS TOKENS each size produces, not rendered pixels (`w-[480px]`
 * and `min-w-[480px]` share a substring, so token-level comparison is used
 * throughout rather than `toContain` on the raw string) - real-pixel
 * verification is a Playwright check (see the PR body's real-browser
 * evidence).
 */

afterEach(cleanup);

function noop() {}

function classTokens(el: Element): string[] {
  return el.className.split(/\s+/).filter(Boolean);
}

describe("Drawer size variant", () => {
  it("defaults to the fixed 480px width, not the wide 66vw treatment", () => {
    render(
      <Drawer open onClose={noop}>
        content
      </Drawer>,
    );
    const tokens = classTokens(screen.getByRole("dialog"));
    expect(tokens).toContain("w-[480px]");
    expect(tokens).not.toContain("w-[66vw]");
    expect(tokens).not.toContain("min-w-[480px]");
  });

  it('size="wide" renders the ~66vw treatment with its min-width floor, not the default fixed width', () => {
    render(
      <Drawer open onClose={noop} size="wide">
        content
      </Drawer>,
    );
    const tokens = classTokens(screen.getByRole("dialog"));
    expect(tokens).toContain("w-[66vw]");
    expect(tokens).toContain("min-w-[480px]");
    expect(tokens).not.toContain("w-[480px]");
  });

  it("accepts aria-label for the dialog's accessible name (finding 3)", () => {
    render(
      <Drawer open onClose={noop} aria-label="Example receipt">
        content
      </Drawer>,
    );
    expect(screen.getByRole("dialog", { name: "Example receipt" })).toBeDefined();
  });

  it("accepts aria-labelledby, naming itself from an existing visible heading (finding 3)", () => {
    render(
      <>
        <h2 id="heading-id">Named from a heading</h2>
        <Drawer open onClose={noop} aria-labelledby="heading-id">
          content
        </Drawer>
      </>,
    );
    expect(screen.getByRole("dialog", { name: "Named from a heading" })).toBeDefined();
  });

  it("carries neither aria-label nor aria-labelledby when the caller passes none", () => {
    render(
      <Drawer open onClose={noop}>
        content
      </Drawer>,
    );
    const dialog = screen.getByRole("dialog");
    expect(dialog.hasAttribute("aria-label")).toBe(false);
    expect(dialog.hasAttribute("aria-labelledby")).toBe(false);
  });
});
