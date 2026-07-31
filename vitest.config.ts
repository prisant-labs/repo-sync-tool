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
    // Node, not jsdom. The first test set is pure logic (status derivation, time
    // conversion) and needs no DOM, so pulling in a DOM implementation would cost
    // startup time and a dependency for nothing. When the first component test
    // arrives it should add jsdom (or happy-dom) plus testing-library and switch
    // this to an environment-per-file annotation, not a blanket change.
    environment: "node",
    include: ["src/**/*.test.ts", "src/**/*.test.tsx"],
    // Fail rather than silently pass when a run matches no files. A test suite
    // that quietly matches nothing looks identical to one that passes.
    passWithNoTests: false,
  },
});
