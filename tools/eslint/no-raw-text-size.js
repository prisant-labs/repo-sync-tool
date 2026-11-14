// Local ESLint rule: forbid raw Tailwind font sizes, so the named ramp in
// `src/index.css` (`text-body`, `text-cell`, `text-meta`, `text-micro`, ...) is
// the only way to size text. It reads string `Literal` and `TemplateElement`
// nodes only, so a comment that mentions `text-sm` is never reported.

const NAMED_SIZE = /^text-(xs|sm|base|lg|\d*xl)$/;

// A CSS length: digits or a leading dot, then a unit. `text-[#333]` and
// `text-[oklch(...)]` are colours and never match.
const LENGTH = /^[+-]?(\d+\.?\d*|\.\d+)(px|rem|em|%|vw|vh|vmin|vmax|ch|ex|lh|rlh|pt|pc|cm|mm|in|svw|svh|lvw|lvh|dvw|dvh|cqw|cqh)$/;
const MATH = /^(calc|clamp|min|max)\(/;
const COLOUR = /^(#|rgb|hsl|hwb|lab|lch|oklab|oklch|color\(|color:|color-mix|var\(--color)/;

const REPLACEMENT = {
  "text-sm": "text-body",
  "text-xs": "text-cell",
  "text-[11px]": "text-meta",
  "text-[10px]": "text-micro",
  "text-base": "text-heading",
  "text-lg": "text-title",
};

// The part of a class after the last top-level `:`. Colons inside square
// brackets (`[&:not(a)]:`, `text-[length:11px]`) are not variant separators.
function stripVariants(token) {
  let depth = 0;
  let cut = 0;
  for (let i = 0; i < token.length; i++) {
    const ch = token[i];
    if (ch === "[") depth++;
    else if (ch === "]") depth--;
    else if (ch === ":" && depth === 0) cut = i + 1;
  }
  return token.slice(cut);
}

// `text-sm/6` -> `text-sm`; a slash inside brackets (`calc(1/2)`) is kept.
function stripModifier(utility) {
  let depth = 0;
  for (let i = 0; i < utility.length; i++) {
    const ch = utility[i];
    if (ch === "[" || ch === "(") depth++;
    else if (ch === "]" || ch === ")") depth--;
    else if (ch === "/" && depth === 0) return utility.slice(0, i);
  }
  return utility;
}

function isArbitrarySize(utility) {
  const m = /^text-\[(.+)\]$/.exec(utility);
  if (!m) return false;
  const v = m[1];
  if (COLOUR.test(v)) return false;
  if (v.startsWith("length:")) return true;
  return LENGTH.test(v) || (MATH.test(v) && !v.includes("--color"));
}

// The utility a class applies, or "" when it is not a raw font size.
function rawSize(token) {
  let u = stripVariants(token);
  u = u.replace(/^!/, "").replace(/!$/, "");
  u = stripModifier(u);
  return NAMED_SIZE.test(u) || isArbitrarySize(u) ? u : "";
}

function hint(utility) {
  const named = REPLACEMENT[utility];
  if (named) return `use ${named}`;
  return "use a named size: text-body (14px), text-cell (12px), text-meta (11px), text-heading (16px), text-title (18px), text-label or text-page; see the type ramp in src/index.css";
}

/** @type {import("eslint").Rule.RuleModule} */
export default {
  meta: {
    type: "problem",
    schema: [],
    messages: {
      raw: "Raw font size `{{cls}}`: {{hint}}.",
    },
  },
  create(context) {
    function check(node, text) {
      for (const token of text.split(/\s+/)) {
        if (!token) continue;
        const utility = rawSize(token);
        if (utility) {
          context.report({
            node,
            messageId: "raw",
            data: { cls: token, hint: hint(utility) },
          });
        }
      }
    }
    return {
      Literal(node) {
        if (typeof node.value === "string") check(node, node.value);
      },
      TemplateElement(node) {
        check(node, node.value.cooked ?? node.value.raw);
      },
    };
  },
};
