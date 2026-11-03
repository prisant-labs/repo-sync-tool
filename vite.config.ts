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
  // No `test` block here. Vitest reads `vitest.config.ts` and ignores this file
  // whenever that one exists, so test settings placed here never take effect.
});
