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
    // `_local` is the gitignored design tree: benches, generators and
    // prototypes, none of it shipped and none of it written to this config's
    // rules. Without it here `pnpm lint` fails repo-wide for anyone who has
    // that tree on disk, dying on a bench script with `return` outside a
    // function - a failure that says nothing about `src/` and that CI never
    // sees, because `_local` is not committed.
    // Both spellings on purpose. Windows resolves the directory
    // case-insensitively but eslint's ignore matching is case-SENSITIVE, and
    // `.gitignore` line 28 spells it `_LOCAL/` while the working tree spells
    // it `_local` - so a single entry silently misses on one of them.
    ignores: ["dist", "src-tauri", "target", "_local", "_LOCAL"],
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
