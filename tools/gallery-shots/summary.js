// The pure parts of the gallery screenshot check: naming a shot, and turning a list of
// per-shot comparison results into the Markdown table the CI job writes to the step summary.
// Nothing here touches the disk or a browser, so it is unit-tested directly
// (summary.test.ts). The file-reading parts live in compare.mjs.

/** One screenshot file is named `<section-id>__<theme>.png`, for example `buttons__dark.png`. */
export function shotName(sectionId, theme) {
  return `${sectionId}__${theme}.png`;
}

/** The reverse of `shotName`. Returns null for a file that does not follow the pattern. */
export function parseShotName(fileName) {
  const match = /^(.+)__(light|dark)\.png$/.exec(fileName);
  return match ? { section: match[1], theme: match[2] } : null;
}

/**
 * A shot with this many changed pixels or fewer is reported as "noise", not "changed".
 * Two builds of identical source were seen to differ by a single pixel in a few dark shots
 * on the Linux runner (anti-aliasing at a rounded corner or a shadow edge, which the GPU-less
 * renderer does not always place identically). Any real change to an element, even a one
 * pixel border, is far larger than a handful of pixels, so a small floor hides the noise
 * without hiding a change. The diff image is still written for a noisy shot.
 */
export const NOISE_FLOOR = 4;

/**
 * Decide one shot's status from what was found on each side.
 *   base, head: `{ width, height }` for a side that has the shot, or null when it does not.
 *   changed: the number of pixels pixelmatch reported different (ignored unless sizes match).
 * Returns `{ status, changed, total }`. A size mismatch is its own status because the
 * pixels cannot be compared one-to-one, and calling it "changed pixels: 0" would hide it.
 */
export function classifyShot(base, head, changed = 0, noiseFloor = NOISE_FLOOR) {
  if (!base && !head) throw new Error("classifyShot needs at least one side");
  if (!base) return { status: "added", changed: 0, total: head.width * head.height };
  if (!head) return { status: "removed", changed: 0, total: base.width * base.height };
  if (base.width !== head.width || base.height !== head.height) {
    return {
      status: "size-changed",
      changed: 0,
      total: head.width * head.height,
      baseSize: `${base.width}x${base.height}`,
      headSize: `${head.width}x${head.height}`,
    };
  }
  const total = head.width * head.height;
  if (changed === 0) return { status: "same", changed, total };
  return { status: changed <= noiseFloor ? "noise" : "changed", changed, total };
}

/** Percent of pixels changed, with two decimals, or "n/a" when there is nothing to compare. */
export function formatPercent(changed, total) {
  if (!total) return "n/a";
  return `${((changed / total) * 100).toFixed(2)}%`;
}

const STATUS_ORDER = { "size-changed": 0, changed: 1, added: 2, removed: 3, noise: 4, same: 5 };

/**
 * Build the Markdown report.
 *   results: `[{ section, theme, status, changed, total, baseSize?, headSize? }]`
 * Unchanged shots are counted but not listed, so a pull request that changes one section
 * produces a short table instead of a hundred rows. Rows with differences come first,
 * the biggest change first within each kind.
 */
export function buildSummary(results) {
  const unchanged = results.filter((r) => r.status === "same").length;
  const noise = results.filter((r) => r.status === "noise").length;
  const noiseNames = results.filter((r) => r.status === "noise").map((r) => `${r.section} ${r.theme}`);
  const noiseNote =
    noise > 0
      ? ` ${noise} more differ by ${NOISE_FLOOR} pixels or fewer and are treated as noise (${noiseNames.join(", ")}).`
      : "";
  const interesting = results
    .filter((r) => r.status !== "same" && r.status !== "noise")
    .sort(
      (a, b) =>
        STATUS_ORDER[a.status] - STATUS_ORDER[b.status] ||
        b.changed - a.changed ||
        a.section.localeCompare(b.section) ||
        a.theme.localeCompare(b.theme),
    );

  const lines = ["## Gallery screenshot check", ""];
  if (interesting.length === 0) {
    lines.push(`No pixel differences in ${results.length} screenshots.${noiseNote}`);
    return lines.join("\n") + "\n";
  }

  lines.push(
    `${interesting.length} of ${results.length} screenshots differ from the base branch ` +
      `(${unchanged} unchanged).${noiseNote} This check reports only; it never fails a pull request on pixels.`,
    "",
    "| Section | Theme | Result | Changed pixels | Percent |",
    "|---|---|---|---|---|",
  );
  for (const r of interesting) {
    let result;
    let pixels = "-";
    let percent = "-";
    if (r.status === "changed") {
      result = "changed";
      pixels = String(r.changed);
      percent = formatPercent(r.changed, r.total);
    } else if (r.status === "size-changed") {
      result = `size changed (${r.baseSize} to ${r.headSize})`;
    } else {
      result = r.status;
    }
    lines.push(`| ${r.section} | ${r.theme} | ${result} | ${pixels} | ${percent} |`);
  }
  lines.push("", "Base, head and diff images are in the `gallery-screenshots` artifact.");
  return lines.join("\n") + "\n";
}
