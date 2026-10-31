import js from "@eslint/js";
import { defineConfig, globalIgnores } from "eslint/config";
import svelte from "eslint-plugin-svelte";
import tseslint from "typescript-eslint";

export default defineConfig(
  globalIgnores(["dist/", "src-tauri/"]),
  js.configs.recommended,
  tseslint.configs.recommended,
  svelte.configs.recommended,
  {
    files: ["**/*.svelte"],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
    rules: { "no-undef": "off" },
  },
);
