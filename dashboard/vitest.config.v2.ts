/**
 * Vitest for the v2 app — React, with a DOM.
 *
 * Two things here are not defaults, and both are the same trap the bundler config documents:
 *
 * 1. `jsxImportSource: "react"`. `tsconfig.json` sets `"jsxImportSource": "preact"` and includes `src`, so
 *    without this the test transform compiles v2's JSX through Preact's jsx-runtime and hands Preact vnodes
 *    to real React. That is `Minified React error #31` at runtime, not a build error — it cost an hour in
 *    the bundler and would cost another here.
 * 2. `include` is scoped to `src/v2`. v1's config claims every `*.test.ts` under `src`, so the two configs
 *    must partition the tree — otherwise a test file is run by whichever config happens to claim it, under
 *    the wrong framework, with the wrong JSX runtime, and no DOM.
 */
import { defineConfig } from "vitest/config";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  esbuild: { jsx: "automatic", jsxImportSource: "react" },
  test: {
    include: ["src/v2/**/*.test.{ts,tsx}"],
    environment: "jsdom",
    globals: true,
    restoreMocks: true,
  },
});
