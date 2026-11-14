// Local ESLint rule: forbid raw Tailwind font sizes, so the named ramp in
// `src/index.css` (`text-body`, `text-cell`, `text-meta`, `text-micro`, ...) is
// the only way to size text. It reads string `Literal` and `TemplateElement`
// nodes only, so a comment that mentions `text-sm` is never reported.

const RAW_SIZE = /^text-(xs|sm|base|lg|\d*xl)$|^text-\[\d/;

const REPLACEMENT = {
  "text-sm": "text-body",
  "text-xs": "text-cell",
  "text-[11px]": "text-meta",
  "text-[10px]": "text-micro",
  "text-base": "text-heading",
  "text-lg": "text-title",
};

// Drop variant prefixes (`md:hover:`), but only colons outside square brackets,
// so an arbitrary variant such as `[&:not(a)]:text-xs` splits correctly.
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
        const utility = stripVariants(token);
        if (RAW_SIZE.test(utility)) {
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
