import js from "@eslint/js";
import globals from "globals";
import tseslint from "typescript-eslint";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";

// Note: eslint-plugin-react-hooks 7.x still ships its `recommended-latest`
// preset with `plugins` as an array, which ESLint 10 flat config rejects.
// We register the plugin object ourselves and reuse only its rule set.
export default tseslint.config(
  {
    // `target` is the Cargo workspace build directory at the repo root; it holds
    // Tauri codegen assets (non-source `.js` files) that must never be linted.
    // Without this, `eslint .` fails on any machine that has built the Rust side
    // (CI passes only because it lints a fresh checkout with no `target/` yet).
    ignores: ["dist", "src-tauri", "target"],
  },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
    },
    rules: {
      ...reactHooks.configs["recommended-latest"].rules,
    },
  },
  reactRefresh.configs.vite,
);
