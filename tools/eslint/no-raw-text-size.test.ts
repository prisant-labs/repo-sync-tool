import { RuleTester } from "eslint";
import tseslint from "typescript-eslint";
import { afterAll, describe, it } from "vitest";
import rule from "./no-raw-text-size.js";

// RuleTester calls the globals `describe` and `it`; point them at vitest.
RuleTester.describe = describe;
RuleTester.it = it;
// `afterAll` is read by RuleTester at runtime but missing from its typings.
(RuleTester as unknown as { afterAll: typeof afterAll }).afterAll = afterAll;

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
    // Colours, alignment and wrapping utilities that share the `text-` prefix.
    { code: `const a = "text-[#333] text-[color:var(--x)] text-[oklch(0.5_0_0)]";` },
    { code: `const a = "text-[rgb(0_0_0)] text-[var(--color-foo)] md:text-[#333]";` },
    { code: `const a = "text-muted-foreground/80 text-center text-balance text-ellipsis";` },
    { code: `const a = "md:hover:text-muted-foreground/80 hover:text-center";` },
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
      code: `const a = "text-base text-lg";`,
      errors: [
        { messageId: "raw", data: { cls: "text-base", hint: "use text-heading" } },
        { messageId: "raw", data: { cls: "text-lg", hint: "use text-title" } },
      ],
    },
    {
      code: `const a = "text-2xl text-[13px]";`,
      errors: [{ messageId: "raw" }, { messageId: "raw" }],
    },
    // Forms Tailwind still turns into a font-size, each bare and behind variants.
    ...[
      "!text-sm",
      "text-sm!",
      "text-sm/6",
      "!text-sm/6",
      "text-[length:11px]",
      "text-[.75rem]",
      "text-[0.75rem]",
      "text-[1.5em]",
      "text-[110%]",
      "text-[calc(1rem+2px)]",
      "text-[clamp(1rem,2vw,2rem)]",
      "text-[11px]/6",
      "text-[11px]!",
    ].flatMap((cls) => [cls, `md:hover:${cls}`]).map((cls) => ({
      code: `const a = "flex ${cls}";`,
      errors: [{ messageId: "raw" as const }],
    })),
    {
      code: `const a = "md:hover:!text-sm";`,
      errors: [{ messageId: "raw", data: { cls: "md:hover:!text-sm", hint: "use text-body" } }],
    },
    {
      code: `const a = "text-sm/6 text-[11px]/6";`,
      errors: [
        { messageId: "raw", data: { cls: "text-sm/6", hint: "use text-body" } },
        { messageId: "raw", data: { cls: "text-[11px]/6", hint: "use text-meta" } },
      ],
    },
  ],
});
