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

        // The gallery's main pane scrolls inside a screen-high shell, so grow the
        // viewport until the whole section is visible in one picture.
        const extra = await page.$eval('[data-testid="gallery-main"]', (m) => m.scrollHeight - m.clientHeight);
        if (extra > 0) {
          await page.setViewportSize({
            width: VIEWPORT_WIDTH,
            height: Math.min(START_HEIGHT + extra, MAX_HEIGHT),
          });
          await waitForSettled(page);
        }

        // A section that mounts its own theme root (the app shell) could undo the switch.
        // Refuse to write a "dark" picture that is really light.
        const isDark = await page.evaluate(() => document.documentElement.classList.contains("dark"));
        if (isDark !== wantDark) {
          failures.push(`${id} (${theme}): the page ended up ${isDark ? "dark" : "light"}`);
          continue;
        }
        await page.screenshot({ path: path.join(outDir, shotName(id, theme)), animations: "disabled" });
      }
    }
    await page.getByLabel("Dark", { exact: true }).setChecked(false);
    console.log(`wrote ${sectionIds.length * 2} screenshots to ${outDir}`);
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
