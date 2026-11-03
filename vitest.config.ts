import path from "node:path";
import { defineConfig } from "vitest/config";

// Vitest lives in its own config rather than a `test` key on vite.config.ts, so the
// app's build config stays free of test-only concerns and neither file has to be
// read by tooling that only cares about the other.
//
// The `@` alias is duplicated from vite.config.ts deliberately: the two configs are
// loaded independently, so importing one into the other to share it would pull the
// React and Tailwind plugins into every test run for no benefit.
export default defineConfig({
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  test: {
    // `node` stays the default so the pure-logic tests in `src/lib` keep proving
    // they need no DOM at all. Component tests opt in per file with a
    // `// @vitest-environment jsdom` docblock, which keeps the environment
    // visible at the top of the file that needs it rather than in config the
    // reader has to go looking for.
    environment: "node",
    // `src/test/mock-ipc.ts` stubs generated commands with `vi.spyOn`. Without
    // this, a stub survives into the next test and the failure depends on test
    // order, which is the hardest kind of test failure to read.
    //
    // This setting used to sit in a `test` block in vite.config.ts, where it
    // never took effect: Vitest reads this file instead whenever it exists. A
    // probe proved it on 2026-11-02 - a spy installed in one test was still
    // stubbed in the next.
    restoreMocks: true,
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    // Fail rather than silently pass when a run matches no files. A test suite
    // that quietly matches nothing looks identical to one that passes.
    passWithNoTests: false,
  },
});
