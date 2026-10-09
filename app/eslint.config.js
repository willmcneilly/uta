import js from "@eslint/js";
import comments from "@eslint-community/eslint-plugin-eslint-comments";
import { defineConfig } from "eslint/config";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import globals from "globals";
import tseslint from "typescript-eslint";
import uta from "./scripts/lint/noRawColour.ts";

export default defineConfig(
  { ignores: ["dist", "src-tauri", "scripts/lint/fixtures"] },
  {
    files: ["**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat["recommended-latest"],
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
  },
  // No colour written directly in the code: see scripts/lint/noRawColour.ts.
  {
    files: ["**/*.{ts,tsx}"],
    ignores: ["src/design/tokens.ts"],
    plugins: { uta },
    rules: { "uta/no-raw-colour": "error" },
  },
  // Every escape has to say why, after `--`.
  {
    files: ["**/*.{ts,tsx}"],
    plugins: { "@eslint-community/eslint-comments": comments },
    rules: { "@eslint-community/eslint-comments/require-description": "error" },
  },
);
