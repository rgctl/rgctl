import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["src/**/*.test.ts"],
    // v2 has its own config: React, a DOM environment, and the React JSX runtime. Without this exclusion
    // a v2 `.test.ts` would be claimed by this config and run under the Preact shim with no DOM.
    exclude: ["src/v2/**", "node_modules/**"],
  },
});
