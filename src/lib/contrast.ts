/**
 * oklch -> linear sRGB -> WCAG relative luminance -> contrast ratio.
 *
 * A TypeScript port of `_local/design/.../\_generators/contrast.py`, which has
 * been the project's measuring tool for every colour decision but lives in the
 * gitignored `_local/` tree, so CI has never been able to run it. This exists
 * so the numbers can be asserted in the test suite rather than re-measured by
 * hand and pasted into a comment.
 *
 * Three shipped accessibility failures have now been found by measuring a pair
 * that a comment claimed was fine: D-1 (the activity chip), D-14 (the active
 * group row) and D-17 (the Activity "Failed" filter). All three were declared
 * token pairs. None of them were computed before shipping.
 */

export type Oklch = { L: number; C: number; H: number };

/** WCAG AA floors. Large text has a lower one; nothing here uses large text. */
export const AA_TEXT = 4.5;
export const AA_NON_TEXT = 3.0;

export function oklchToLinearSrgb({ L, C, H }: Oklch): [number, number, number] {
  const h = (H * Math.PI) / 180;
  const a = C * Math.cos(h);
  const b = C * Math.sin(h);
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = L - 0.0894841775 * a - 1.291485548 * b;
  const l = l_ ** 3;
  const m = m_ ** 3;
  const s = s_ ** 3;
  return [
    clamp01(4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s),
    clamp01(-1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s),
    clamp01(-0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s),
  ];
}

const clamp01 = (v: number) => Math.min(1, Math.max(0, v));

export function luminance(c: Oklch): number {
  const [r, g, b] = oklchToLinearSrgb(c);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

export function ratioFromLuminance(a: number, b: number): number {
  const hi = Math.max(a, b);
  const lo = Math.min(a, b);
  return (hi + 0.05) / (lo + 0.05);
}

export function contrast(fg: Oklch, bg: Oklch): number {
  return ratioFromLuminance(luminance(fg), luminance(bg));
}

const srgbEncode = (v: number) =>
  v > 0.0031308 ? 1.055 * v ** (1 / 2.4) - 0.055 : 12.92 * v;
const srgbDecode = (v: number) =>
  v > 0.04045 ? ((v + 0.055) / 1.055) ** 2.4 : v / 12.92;

/**
 * Luminance of `fg` painted at `alpha` over `bg`, composited in GAMMA-ENCODED
 * sRGB - the CSS Color 4 default for a simple `color / alpha` value, not
 * linear light. Getting this wrong shifts the answer by enough to turn a fail
 * into a pass, which is exactly the mistake worth not making in a gate.
 */
export function blendedLuminance(fg: Oklch, alpha: number, bg: Oklch): number {
  const f = oklchToLinearSrgb(fg).map(srgbEncode);
  const b = oklchToLinearSrgb(bg).map(srgbEncode);
  const blended = f.map((v, i) => alpha * v + (1 - alpha) * b[i]).map(srgbDecode);
  return 0.2126 * blended[0] + 0.7152 * blended[1] + 0.0722 * blended[2];
}

/** An opaque ink over a translucent wash over an opaque surface. */
export function contrastOverWash(
  ink: Oklch,
  wash: Oklch,
  alpha: number,
  surface: Oklch,
): number {
  return ratioFromLuminance(luminance(ink), blendedLuminance(wash, alpha, surface));
}

/**
 * Pull every `--token: oklch(L C H)` declaration out of one CSS rule block.
 *
 * Deliberately a narrow parser rather than a CSS library: it understands the
 * one shape this project's token block is written in, and a token written any
 * other way is simply absent from the result - which the gate reports as a
 * missing token rather than silently skipping. Alpha-carrying values
 * (`oklch(1 0 0 / 10%)`) are skipped here on purpose; a wash's alpha belongs
 * in the PAIR that uses it, where it can be composited, not in the token table.
 */
export function parseTokens(css: string, selector: string): Map<string, Oklch> {
  // Split on the selector as a plain string rather than building a regex out
  // of it: `:root` and `.dark` both contain regex metacharacters, and an
  // escaping bug here would fail by finding NOTHING, which is the one failure
  // mode a gate must not have.
  const lines = css.split(/\r?\n/);

  // The largest matching block wins. A stylesheet can open the same selector
  // more than once (`:root` again for an override, a theme layer), and the
  // one that matters is whichever actually declares the tokens - taking the
  // first match would silently read an empty block and report every token
  // missing.
  let best = new Map<string, Oklch>();
  for (let i = 0; i < lines.length; i++) {
    // The selector must be alone on its line before the brace, so `:root`
    // does not also match `:root:not([data-theme="light"])`.
    if (lines[i].trim() !== `${selector} {`) continue;
    const body: string[] = [];
    for (let j = i + 1; j < lines.length && lines[j].trim() !== "}"; j++) body.push(lines[j]);
    const found = declarations(body.join("\n"));
    if (found.size > best.size) best = found;
  }

  if (best.size === 0) {
    throw new Error(`No "${selector}" block with oklch tokens in the stylesheet`);
  }
  return best;
}

function declarations(block: string): Map<string, Oklch> {
  const tokens = new Map<string, Oklch>();
  const re = /--([\w-]+):\s*oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)\s*\)/g;
  for (const m of block.matchAll(re)) {
    tokens.set(m[1], { L: Number(m[2]), C: Number(m[3]), H: Number(m[4]) });
  }
  return tokens;
}
