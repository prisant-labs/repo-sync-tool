import path from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import { viteSingleFile } from "vite-plugin-singlefile";

// The component gallery's own Vite config. It is a second entry (`gallery.html`)
// built into `dist-gallery/`, NEVER `dist/`: Tauri bundles `dist/`, and the
// gallery must not ship inside the app.
//
// `viteSingleFile` inlines every script and stylesheet into the one HTML file,
// so a person can open it by double-clicking: no server, no network, no
// relative asset paths to break when the file is moved or emailed.
// https://github.com/richardtallent/vite-plugin-singlefile
export default defineConfig({
  plugins: [react(), tailwindcss(), viteSingleFile()],
  clearScreen: false,
  base: "./",
  // `public/` holds the app icon; the gallery needs no asset beside its one file.
  publicDir: false,
  resolve: {
    alias: {
      "@": path.resolve(__dirname, "./src"),
    },
  },
  server: {
    // 1422, never 1420: 1420 belongs to the Tauri shell's dev server.
    port: 1422,
    strictPort: true,
  },
  build: {
    outDir: "dist-gallery",
    emptyOutDir: true,
    cssCodeSplit: false,
    rollupOptions: {
      input: path.resolve(__dirname, "gallery.html"),
    },
  },
});
