// @vitest-environment jsdom
import { useState } from "react";
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Tabs, TabList, TabPanel } from "@/components/ui/tabs";

/**
 * Unit coverage for the hand-rolled ARIA tabs primitive (D2: jp decided
 * against Radix for N4, 2026-08-31). Asserts the ARIA tree shape directly -
 * role=tablist/tab/tabpanel, aria-selected, roving tabindex, the tab/panel
 * `aria-controls`/`aria-labelledby` linkage - rather than any detail of
 * `repo-detail.tsx`'s own usage, which is covered separately in
 * `repo-detail.test.tsx`.
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
  it("renders one tablist, three tabs, and exactly one tabpanel for the active tab", () => {
    render(<Harness />);

    const tablist = screen.getByRole("tablist", { name: "Example sections" });
    expect(tablist).toBeDefined();

    const tabs = screen.getAllByRole("tab");
    expect(tabs).toHaveLength(3);

    const panels = screen.getAllByRole("tabpanel");
    expect(panels).toHaveLength(1);
    expect(screen.getByText("Alpha content")).toBeDefined();
    expect(screen.queryByText("Beta content")).toBeNull();
    expect(screen.queryByText("Gamma content")).toBeNull();
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

  it("links the active tab and its panel via aria-controls / aria-labelledby / id", () => {
    render(<Harness initial="c" />);

    const gamma = screen.getByRole("tab", { name: "Gamma" });
    const panel = screen.getByRole("tabpanel");

    expect(gamma.getAttribute("aria-controls")).toBe(panel.id);
    expect(panel.getAttribute("aria-labelledby")).toBe(gamma.id);

    // Inactive tabs name no panel: the id they would point at does not exist
    // in the DOM (TabPanel unmounts inactive content).
    const alpha = screen.getByRole("tab", { name: "Alpha" });
    expect(alpha.getAttribute("aria-controls")).toBeNull();
  });

  it("Right/Left/Home/End move focus and activate (automatic activation), wrapping at both ends", async () => {
    render(<Harness />);
    const user = userEvent.setup();

    const alpha = screen.getByRole("tab", { name: "Alpha" });
    alpha.focus();

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
    expect(screen.getByText("Gamma content")).toBeDefined();
  });
});
