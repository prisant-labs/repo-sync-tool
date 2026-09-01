// @vitest-environment jsdom
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tabs, TabList, TabPanel } from "@/components/ui/tabs";

/**
 * Unit coverage for the hand-rolled ARIA tabs primitive (D2: jp decided
 * against Radix for N4, 2026-08-31). Asserts the ARIA tree shape directly -
 * role=tablist/tab/tabpanel, aria-selected, roving tabindex, the tab/panel
 * `aria-controls`/`aria-labelledby` linkage, and the automatic-activation
 * model (selection follows focus, by any path focus arrives, not only this
 * component's own arrow-key handler) - rather than any detail of
 * `repo-detail.tsx`'s own usage, which is covered separately in
 * `repo-detail.test.tsx`.
 *
 * All three `TabPanel`s stay MOUNTED regardless of which is active (only
 * the `hidden` attribute toggles) - see `ui/tabs.tsx`'s file doc comment.
 * `getByText`/`queryByText` do NOT filter hidden content, so tests that care
 * about visibility use role queries (`getAllByRole("tabpanel")` excludes
 * hidden panels by default; pass `{ hidden: true }` to include them) or
 * assert the `hidden` attribute directly - never `queryByText`.
 */

afterEach(cleanup);

const TABS = [
  { value: "a", label: "Alpha" },
  { value: "b", label: "Beta" },
  { value: "c", label: "Gamma" },
];

function Harness({ initial = "a" }: { initial?: string }) {
  const [value, setValue] = useState(initial);
  return (
    <Tabs value={value} onValueChange={setValue}>
      <TabList aria-label="Example sections" tabs={TABS} />
      <TabPanel value="a">Alpha content</TabPanel>
      <TabPanel value="b">Beta content</TabPanel>
      <TabPanel value="c">Gamma content</TabPanel>
    </Tabs>
  );
}

describe("Tabs primitive ARIA tree", () => {
  it("renders one tablist and three tabs; all three panels stay mounted but only the active one is visible", () => {
    render(<Harness />);

    const tablist = screen.getByRole("tablist", { name: "Example sections" });
    expect(tablist).toBeDefined();

    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(3);

    // Default role queries exclude hidden elements from the a11y tree.
    const visiblePanels = screen.getAllByRole("tabpanel");
    expect(visiblePanels).toHaveLength(1);
    expect(visiblePanels[0].textContent).toBe("Alpha content");

    // `{ hidden: true }` proves the inactive two are still mounted, not
    // unmounted - the load-bearing contract for finding 2a (a stable
    // aria-controls target on every tab) and per-panel scroll persistence.
    const allPanels = screen.getAllByRole("tabpanel", { hidden: true });
    expect(allPanels).toHaveLength(3);
    const betaPanel = allPanels.find((p) => p.textContent === "Beta content");
    const gammaPanel = allPanels.find((p) => p.textContent === "Gamma content");
    expect(betaPanel?.hasAttribute("hidden")).toBe(true);
    expect(gammaPanel?.hasAttribute("hidden")).toBe(true);
  });

  it("marks exactly the active tab aria-selected, with roving tabindex", () => {
    render(<Harness initial="b" />);

    const alpha = screen.getByRole("tab", { name: "Alpha" });
    const beta = screen.getByRole("tab", { name: "Beta" });
    const gamma = screen.getByRole("tab", { name: "Gamma" });

    expect(beta.getAttribute("aria-selected")).toBe("true");
    expect(alpha.getAttribute("aria-selected")).toBe("false");
    expect(gamma.getAttribute("aria-selected")).toBe("false");

    expect(beta.getAttribute("tabindex")).toBe("0");
    expect(alpha.getAttribute("tabindex")).toBe("-1");
    expect(gamma.getAttribute("tabindex")).toBe("-1");
  });

  it("gives every tab - active or not - an aria-controls that resolves to a real, mounted panel", () => {
    render(<Harness initial="c" />);

    const gamma = screen.getByRole("tab", { name: "Gamma" });
    const alpha = screen.getByRole("tab", { name: "Alpha" });
    const beta = screen.getByRole("tab", { name: "Beta" });
    const activePanel = screen.getByRole("tabpanel");

    expect(gamma.getAttribute("aria-controls")).toBe(activePanel.id);
    expect(activePanel.getAttribute("aria-labelledby")).toBe(gamma.id);

    // The W3C tabs pattern requires aria-controls on every tab, not only
    // the active one (Codex adversarial review, finding 2a, confirmed).
    // Since TabPanel keeps every panel mounted, an inactive tab's
    // aria-controls id resolves to a real (hidden) element too.
    for (const tab of [alpha, beta, gamma]) {
      const controlsId = tab.getAttribute("aria-controls");
      expect(controlsId).toBeTruthy();
      const panel = document.getElementById(controlsId!);
      expect(panel).not.toBeNull();
      expect(panel?.getAttribute("role")).toBe("tabpanel");
    }
  });

  it("Right/Left/Home/End move focus and activate (automatic activation), wrapping at both ends", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    const alpha = screen.getByRole("tab", { name: "Alpha" });
    act(() => alpha.focus());

    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Beta" }).getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Beta" }));

    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Alpha" }).getAttribute("aria-selected")).toBe("true");

    // Wraps backward past the first tab.
    await user.keyboard("{ArrowLeft}");
    expect(screen.getByRole("tab", { name: "Gamma" }).getAttribute("aria-selected")).toBe("true");

    await user.keyboard("{Home}");
    expect(screen.getByRole("tab", { name: "Alpha" }).getAttribute("aria-selected")).toBe("true");

    await user.keyboard("{End}");
    expect(screen.getByRole("tab", { name: "Gamma" }).getAttribute("aria-selected")).toBe("true");
  });

  it("clicking a tab activates it", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    await user.click(screen.getByRole("tab", { name: "Gamma" }));

    expect(screen.getByRole("tab", { name: "Gamma" }).getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel").textContent).toBe("Gamma content");
  });

  it("selection follows focus by ANY path, not only this component's own arrow-key handler (Codex adversarial review, finding 2b)", () => {
    render(<Harness initial="a" />);

    const beta = screen.getByRole("tab", { name: "Beta" });
    expect(beta.getAttribute("aria-selected")).toBe("false");

    // A tab can legally receive PROGRAMMATIC focus even while inactive
    // (roving tabindex only removes it from sequential Tab navigation, not
    // from being a valid `.focus()` target) - e.g. a screen reader's
    // browse-mode cursor, or any future caller that never goes through
    // this component's own `activate` helper. Simulate exactly that: focus
    // Beta directly, with no click and no arrow key.
    act(() => beta.focus());

    // Automatic activation means this alone must select it - there must be
    // no window where a tab is focused but a DIFFERENT tab is still
    // marked selected.
    expect(beta.getAttribute("aria-selected")).toBe("true");
    expect(screen.getByRole("tabpanel").textContent).toBe("Beta content");
  });

  it("arrow-key math advances from the FOCUSED tab, not a stale 'selected' value (Codex adversarial review, finding 2b)", async () => {
    render(<Harness initial="a" />);
    const user = userEvent.setup();

    // Same divergence setup as above, then drive a key from it.
    const beta = screen.getByRole("tab", { name: "Beta" });
    act(() => beta.focus());
    expect(beta.getAttribute("aria-selected")).toBe("true");

    // If the arrow math read a stale "selected" value instead of
    // `document.activeElement`/`e.target`, this would be a no-op or
    // advance from the wrong tab. Sourced from focus, ArrowRight from Beta
    // must land on Gamma.
    await user.keyboard("{ArrowRight}");
    expect(screen.getByRole("tab", { name: "Gamma" }).getAttribute("aria-selected")).toBe("true");
    expect(document.activeElement).toBe(screen.getByRole("tab", { name: "Gamma" }));
  });
});
