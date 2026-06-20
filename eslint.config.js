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
    ignores: ["dist", "src-tauri"],
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
