// Compares two directories of gallery screenshots and prints a Markdown report.
//
//   node tools/gallery-shots/compare.mjs <base dir> <head dir> <diff dir>
//
// Output: the Markdown table on stdout (CI appends it to $GITHUB_STEP_SUMMARY), plus a diff
// PNG in <diff dir> for every shot whose pixels changed. Exit code is 0 whenever the
// comparison itself ran, even if pixels differ: the check reports, it does not gate. A
// missing directory or an unreadable PNG is a real error and exits 1.
import { mkdir, readdir, readFile, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import pixelmatch from "pixelmatch";
import { PNG } from "pngjs";
import { buildSummary, classifyShot, parseShotName } from "./summary.js";

// pixelmatch's default threshold (0.1) ignores sub-visible colour noise from anti-aliasing.
// Identical builds still differ by exactly zero pixels; this only keeps real changes honest.
const MATCH_OPTIONS = { threshold: 0.1 };

const [baseDir, headDir, diffDir] = process.argv.slice(2);
if (!baseDir || !headDir || !diffDir) {
  console.error("usage: node tools/gallery-shots/compare.mjs <base dir> <head dir> <diff dir>");
  process.exit(2);
}

async function listShots(dir) {
  const names = await readdir(dir);
  return new Set(names.filter((n) => parseShotName(n)));
}

async function readPng(dir, name) {
  return PNG.sync.read(await readFile(path.join(dir, name)));
}

async function main() {
  await rm(diffDir, { recursive: true, force: true });
  await mkdir(diffDir, { recursive: true });

  const baseShots = await listShots(baseDir);
  const headShots = await listShots(headDir);
  const names = [...new Set([...baseShots, ...headShots])].sort();

  const results = [];
  for (const name of names) {
    const { section, theme } = parseShotName(name);
    const base = baseShots.has(name) ? await readPng(baseDir, name) : null;
    const head = headShots.has(name) ? await readPng(headDir, name) : null;

    let changed = 0;
    if (base && head && base.width === head.width && base.height === head.height) {
      const diff = new PNG({ width: head.width, height: head.height });
      changed = pixelmatch(base.data, head.data, diff.data, head.width, head.height, MATCH_OPTIONS);
      if (changed > 0) await writeFile(path.join(diffDir, name), PNG.sync.write(diff));
    }
    results.push({
      section,
      theme,
      ...classifyShot(
        base && { width: base.width, height: base.height },
        head && { width: head.width, height: head.height },
        changed,
      ),
    });
  }

  process.stdout.write(buildSummary(results));
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
