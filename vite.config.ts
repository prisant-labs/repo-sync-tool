/// <reference types="vitest/config" />
import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";

// Tauri convention: keep Vite quiet so Tauri's own output stays readable,
// and pin the dev server to a fixed port the Tauri shell expects.
// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), tailwindcss()],
  // Prevent Vite from clearing the screen so Tauri logs remain visible.
  clearScreen: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    port: 1420,
    strictPort: true,
  },
  test: {
    // `node` stays the default so the pure-logic tests in `src/lib` keep proving
    // they need no DOM at all. Component tests opt in per file with a
    // `// @vitest-environment jsdom` docblock, which keeps the environment
    // visible at the top of the file that needs it rather than in config the
    // reader has to go looking for.
    environment: "node",
    // `src/test/mock-ipc.ts` stubs generated commands with `vi.spyOn`. Without
    // this, a stub survives into the next test file and the failure depends on
    // file order, which is the hardest kind of test failure to read.
    restoreMocks: true,
  },
});
