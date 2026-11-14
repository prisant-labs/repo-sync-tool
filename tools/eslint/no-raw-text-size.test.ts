import { RuleTester } from "eslint";
import tseslint from "typescript-eslint";
import { afterAll, describe, it } from "vitest";
import rule from "./no-raw-text-size.js";

// RuleTester calls the globals `describe` and `it`; point them at vitest.
RuleTester.describe = describe;
RuleTester.it = it;
RuleTester.afterAll = afterAll;

const tester = new RuleTester({
  languageOptions: {
    parser: tseslint.parser,
    parserOptions: { ecmaFeatures: { jsx: true } },
  },
});

tester.run("no-raw-text-size", rule, {
  valid: [
    { code: `const a = "text-muted-foreground";` },
    { code: `const a = "text-body text-cell text-meta text-micro text-label text-page";` },
    { code: `const a = "text-[color:var(--x)] text-center text-foreground";` },
    { code: `const a = "md:hover:text-body";` },
    // A comment that names a raw size is documentation, not usage.
    { code: `// text-sm was the old size\nconst a = "font-medium"; /* text-xs too */` },
    { code: "const a = `flex ${x} text-cell`;" },
  ],
  invalid: [
    {
      code: `const a = "flex text-sm font-medium";`,
      errors: [{ messageId: "raw", data: { cls: "text-sm", hint: "use text-body" } }],
    },
    {
      code: `const a = "md:hover:text-sm";`,
      errors: [{ messageId: "raw", data: { cls: "md:hover:text-sm", hint: "use text-body" } }],
    },
    {
      code: "const a = `flex ${x} text-xs`;",
      errors: [{ messageId: "raw", data: { cls: "text-xs", hint: "use text-cell" } }],
    },
    {
      code: `const v = cva("inline-flex", { variants: { size: { sm: "h-8 text-xs", lg: "text-[11px]" } } });`,
      errors: [
        { messageId: "raw", data: { cls: "text-xs", hint: "use text-cell" } },
        { messageId: "raw", data: { cls: "text-[11px]", hint: "use text-meta" } },
      ],
    },
    {
      code: `const a = <div className="text-[10px] dark:text-lg" />;`,
      errors: [
        { messageId: "raw", data: { cls: "text-[10px]", hint: "use text-micro" } },
        { messageId: "raw" },
      ],
    },
    {
      code: `const a = "text-base text-2xl text-[13px]";`,
      errors: [{ messageId: "raw" }, { messageId: "raw" }, { messageId: "raw" }],
    },
  ],
});
