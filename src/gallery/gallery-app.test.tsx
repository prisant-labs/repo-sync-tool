// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import "@/gallery/stub-ipc";
import { GalleryApp } from "@/gallery/gallery-app";

/**
 * Theme ownership. `AppShell` embeds its own `useTheme`, which sets `.dark` on
 * <html> with no cleanup, so a Settings toggle inside the "App shell" section
 * used to change the page without telling the gallery. The Compare section then
 * drew Today and Look A dark and Look B (`:not(.dark)`) light. The gallery must
 * adopt the page's theme so every column agrees.
 */
afterEach(() => {
  cleanup();
  document.documentElement.classList.remove("dark");
});

function columns() {
  return ["today", "graphite", "slate"].map(
    (id) => document.querySelector<HTMLElement>(`[data-compare="${id}"]`)!,
  );
}

describe("gallery theme ownership", () => {
  it("all three Compare columns follow the Dark switch", async () => {
    const user = userEvent.setup();
    render(<GalleryApp />);
    await user.click(screen.getByRole("button", { name: "Compare looks" }));
    expect(columns().map((c) => c.classList.contains("dark"))).toEqual([false, false, false]);
    await user.click(screen.getByRole("checkbox", { name: "Dark" }));
    expect(columns().map((c) => c.classList.contains("dark"))).toEqual([true, true, true]);
  });

  it("a Settings toggle inside the embedded App shell carries into Compare", async () => {
    const user = userEvent.setup();
    render(<GalleryApp />);
    expect((screen.getByRole("checkbox", { name: "Dark" }) as HTMLInputElement).checked).toBe(false);

    await user.click(screen.getByRole("button", { name: "App shell (whole app)" }));
    // The shell's own nav, then the real Settings theme switch.
    const shellNav = (await screen.findAllByRole("button", { name: /^Settings/ })).find((b) => !b.hasAttribute("data-section"))!;
    await user.click(shellNav);
    const themeSwitch = await screen.findByRole("switch", { name: "Dark theme" });
    await act(async () => {
      await user.click(themeSwitch);
    });
    expect(document.documentElement.classList.contains("dark")).toBe(true);

    await user.click(screen.getByRole("button", { name: "Compare looks" }));
    await waitFor(() =>
      expect(columns().map((c) => c.classList.contains("dark"))).toEqual([true, true, true]),
    );
    expect((screen.getByRole("checkbox", { name: "Dark" }) as HTMLInputElement).checked).toBe(true);
    // Every column carries the SAME theme on the same element as its look.
    for (const c of columns()) expect(c.classList.contains("dark")).toBe(true);
  });
});
