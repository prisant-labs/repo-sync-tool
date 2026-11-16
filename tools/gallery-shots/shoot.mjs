// Screenshots every section of the built component gallery, in light and dark.
//
//   node tools/gallery-shots/shoot.mjs <path to gallery.html> <output dir>
//
// What it captures: every section the gallery lists (including "Compare looks"), in the
// default look ("Today", which sets no `data-look`) and the populated data state, once with
// the Dark switch off and once with it on. One PNG per section and theme, named
// `<section-id>__<theme>.png` (see summary.js).
//
// It drives the gallery's own controls (the `data-section` buttons, the "Dark" checkbox and
// the "Populated" button) instead of reading a query string or calling a hook, because the
// built file has neither. The section list is read from the page, so a new section is
// photographed without a change here.
//
// Why the output is deterministic, since the whole point of the check is that two runs on
// the same build give zero changed pixels:
//   - fixed viewport width and a device scale factor of 1, with the browser's locale and
//     time zone pinned;
//   - the clock is frozen (`Date.now()` and `new Date()` return one fixed instant), because
//     the fixtures print relative times such as "2 min ago";
//   - animations, transitions and the text caret are switched off, and reduced motion is on;
//   - the script waits for web fonts, then for the section's markup to stop changing;
//   - the mouse is parked in an empty corner so no hover state is captured.
import { mkdir, rm } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";
import { chromium } from "playwright";
import { shotName } from "./summary.js";

const VIEWPORT_WIDTH = 1440;
const START_HEIGHT = 1000;
// A section taller than this is cut off rather than making a file nobody can open.
const MAX_HEIGHT = 12000;
// Any fixed instant works; this one is a mid-day UTC time so "today" never flips at midnight.
const FROZEN_TIME = new Date("2026-01-15T12:00:00Z");

const FREEZE_CSS = `
  *, *::before, *::after {
    animation: none !important;
    transition: none !important;
    caret-color: transparent !important;
    scroll-behavior: auto !important;
  }
`;

const [galleryPath, outDir] = process.argv.slice(2);
if (!galleryPath || !outDir) {
  console.error("usage: node tools/gallery-shots/shoot.mjs <gallery.html> <output dir>");
  process.exit(2);
}

// Interactive specimens that are CLOSED when a section first renders, so a plain shot of
// the section never shows them. Each entry opens one and asserts it really is visible; the
// result is saved as `<section>--<name>__<theme>.png`. Overlays the gallery mounts open
// (Drawer, Dialog, GroupDialog, AddReposDialog, the activity receipt, the repo drawer) are
// already in the plain shots and are not listed. Add an entry here when a new section has
// a closed-by-default specimen.
const EXTRA_STATES = [
  {
    section: "toast",
    name: "open",
    // Three toasts, one per kind. They are held on screen (see HOLD_TOAST_TIMERS) so one
    // cannot vanish mid-shot.
    async open(page) {
      for (const label of ["Ok toast", "Info toast", "Error toast"]) {
        await page.getByRole("button", { name: label, exact: true }).click();
      }
    },
    target: (page) => page.getByText("Could not reach the remote.", { exact: true }),
    async expectVisible(page) {
      for (const text of ["Your change was saved.", "Something to know.", "Could not reach the remote."]) {
        await page.getByText(text, { exact: true }).waitFor({ state: "visible", timeout: 3000 });
      }
    },
  },
  {
    section: "data-table",
    name: "open",
    // The ColumnsPopover panel.
    async open(page) {
      await page.getByRole("button", { name: /^Columns/ }).click();
    },
    target: (page) => page.getByRole("region", { name: "Choose and reorder columns" }),
    async expectVisible(page) {
      await page.getByRole("region", { name: "Choose and reorder columns" }).waitFor({ state: "visible", timeout: 3000 });
    },
  },
  {
    section: "screen-repos",
    name: "open",
    // The same panel on the real Repos screen.
    async open(page) {
      await page.getByRole("button", { name: /^Columns/ }).click();
    },
    target: (page) => page.getByRole("region", { name: "Choose and reorder columns" }),
    async expectVisible(page) {
      await page.getByRole("region", { name: "Choose and reorder columns" }).waitFor({ state: "visible", timeout: 3000 });
    },
  },
  {
    section: "screen-drawer",
    name: "add-group",
    // The "add this repo to a group" control in the repository drawer.
    async open(page) {
      await page.getByRole("button", { name: "Add this repo to a group" }).click();
    },
    target: (page) => page.getByRole("button", { name: "Add this repo to a group", expanded: true }),
    async expectVisible(page) {
      await page
        .getByRole("button", { name: "Add this repo to a group", expanded: true })
        .waitFor({ state: "visible", timeout: 3000 });
    },
  },
  {
    section: "screen-activity",
    name: "receipt",
    // The first row's receipt drawer on the Activity screen.
    async open(page) {
      await page.getByRole("button", { name: "Open receipt" }).first().click();
    },
    target: (page) => page.locator("#activity-receipt-title"),
    async expectVisible(page) {
      await page.locator("#activity-receipt-title").waitFor({ state: "visible", timeout: 3000 });
    },
  },
];

// Toasts clear themselves after 4200 ms on a real timer, which would race the screenshot.
// This holds every 4200 ms timer instead of scheduling it, and `window.__releaseToastTimers`
// runs the held callbacks afterwards so the stack is empty before the next section. The
// app's own code is untouched; only this page, inside this script, sees the wrapper.
const HOLD_TOAST_TIMERS = `
  (() => {
    const real = window.setTimeout.bind(window);
    const held = [];
    window.setTimeout = (fn, delay, ...args) => {
      if (delay === 4200) {
        held.push(() => fn(...args));
        return 0;
      }
      return real(fn, delay, ...args);
    };
    window.__releaseToastTimers = () => held.splice(0).forEach((run) => run());
  })();
`;

// The element every picture is cut to: the section's own content, below the gallery's
// title. The nav, the control bar and the page header are outside it on purpose. A change
// to the gallery's own chrome (a new nav entry, a new switch) must not show up as a change
// to every section; only a change to what the section renders should.
const CONTENT = '[data-testid="gallery-main"] > div:last-child';

/**
 * Grows the viewport until the whole section is on screen, settles, and saves one PNG of
 * the section's content only. `contained` is an optional locator (a toast, an open panel)
 * that the cut is widened to include, in case it spills past the section's own box.
 */
async function capture(page, file, contained) {
  await page.setViewportSize({ width: VIEWPORT_WIDTH, height: START_HEIGHT });
  await waitForSettled(page);
  const overflow = await page.$eval('[data-testid="gallery-main"]', (m) => m.scrollHeight - m.clientHeight);
  if (overflow > 0) {
    // Rounded up to a whole 100 px, so a taller control bar (a few pixels of chrome) cannot
    // change the viewport height a section is laid out in.
    const grow = Math.ceil(overflow / 100) * 100;
    await page.setViewportSize({
      width: VIEWPORT_WIDTH,
      height: Math.min(START_HEIGHT + grow, MAX_HEIGHT),
    });
    await waitForSettled(page);
  }
  // The cut is the section's content box, widened to take in an opened specimen that
  // spills past it (a popover opening below the last cell, say), so nothing opened is lost.
  const box = await page.locator(CONTENT).boundingBox();
  if (!box) throw new Error("the section has no content to photograph");
  let clip = box;
  if (contained) {
    const inner = await contained.boundingBox();
    if (!inner) throw new Error("the opened specimen has no box");
    const left = Math.min(box.x, inner.x);
    const top = Math.min(box.y, inner.y);
    const right = Math.max(box.x + box.width, inner.x + inner.width);
    const bottom = Math.max(box.y + box.height, inner.y + inner.height);
    clip = { x: left, y: top, width: right - left, height: bottom - top };
  }
  // Whole pixels, so the picture size does not depend on a fractional layout position.
  const x = Math.floor(clip.x);
  const y = Math.floor(clip.y);
  await page.screenshot({
    path: file,
    animations: "disabled",
    clip: { x, y, width: Math.ceil(clip.x + clip.width) - x, height: Math.ceil(clip.y + clip.height) - y },
  });
}

/** Waits until the main pane's markup is identical across several checks in a row. */
async function waitForSettled(page) {
  await page.evaluate(() => document.fonts.ready);
  let previous = "";
  let stableChecks = 0;
  for (let i = 0; i < 100 && stableChecks < 4; i += 1) {
    const current = await page.evaluate(
      () => document.querySelector('[data-testid="gallery-main"]')?.innerHTML ?? "",
    );
    stableChecks = current === previous ? stableChecks + 1 : 0;
    previous = current;
    await page.waitForTimeout(75);
  }
  if (stableChecks < 4) throw new Error("the section never stopped changing");
}

async function main() {
  await rm(outDir, { recursive: true, force: true });
  await mkdir(outDir, { recursive: true });

  const browser = await chromium.launch();
  const failures = [];
  try {
    const context = await browser.newContext({
      viewport: { width: VIEWPORT_WIDTH, height: START_HEIGHT },
      deviceScaleFactor: 1,
      colorScheme: "light",
      reducedMotion: "reduce",
      locale: "en-US",
      timezoneId: "UTC",
    });
    // Freeze Date.now() / new Date() but leave timers running, so the app still renders.
    await context.clock.setFixedTime(FROZEN_TIME);
    await context.addInitScript(HOLD_TOAST_TIMERS);
    const page = await context.newPage();
    page.on("pageerror", (error) => failures.push(`page error: ${error.message}`));

    await page.goto(pathToFileURL(path.resolve(galleryPath)).href);
    await page.addStyleTag({ content: FREEZE_CSS });
    await page.getByRole("button", { name: "Populated", exact: true }).click();

    const sectionIds = await page.$$eval("button[data-section]", (buttons) =>
      buttons.map((b) => b.getAttribute("data-section")),
    );
    if (sectionIds.length === 0) throw new Error("the gallery lists no sections");

    for (const theme of ["light", "dark"]) {
      const wantDark = theme === "dark";
      for (const id of sectionIds) {
        await page.setViewportSize({ width: VIEWPORT_WIDTH, height: START_HEIGHT });
        await page.click(`button[data-section="${id}"]`);
        await page.getByLabel("Dark", { exact: true }).setChecked(wantDark);
        await page.mouse.move(VIEWPORT_WIDTH - 2, 2);
        await page.evaluate(() => document.activeElement?.blur());
        await waitForSettled(page);

        // A section that mounts its own theme root (the app shell) could undo the switch.
        // Refuse to write a "dark" picture that is really light.
        const isDark = await page.evaluate(() => document.documentElement.classList.contains("dark"));
        if (isDark !== wantDark) {
          failures.push(`${id} (${theme}): the page ended up ${isDark ? "dark" : "light"}`);
          continue;
        }
        await capture(page, path.join(outDir, shotName(id, theme)));

        for (const extra of EXTRA_STATES.filter((e) => e.section === id)) {
          const label = `${id}--${extra.name} (${theme})`;
          try {
            await extra.open(page);
            await extra.expectVisible(page);
            await page.mouse.move(VIEWPORT_WIDTH - 2, 2);
            await capture(page, path.join(outDir, shotName(`${id}--${extra.name}`, theme)), extra.target(page));
          } catch (error) {
            failures.push(`${label}: ${error.message.split("\n")[0]}`);
          }
          // Let go of any held toast timers so the stack is empty for the next section.
          await page.evaluate(() => window.__releaseToastTimers?.());
        }
      }
    }
    await page.getByLabel("Dark", { exact: true }).setChecked(false);
    console.log(`wrote ${(sectionIds.length + EXTRA_STATES.length) * 2} screenshots to ${outDir}`);
  } finally {
    await browser.close();
  }

  if (failures.length > 0) {
    for (const failure of failures) console.error(failure);
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
