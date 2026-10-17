/**
 * v2 dashboard build — React.
 *
 * Deliberately a *separate* config from `vite.config.ts` rather than one config with a mode switch.
 * v1 needs `@preact/preset-vite`, which aliases `react` → `preact/compat`; v2 is real React and that
 * alias would rewrite React into the shim. Exactly one of the two configs may carry the alias, so the
 * alias lives in the file it belongs to and neither config has a branch a reader has to evaluate.
 *
 * Output goes to `dist-v2`, never `dist`: `dist` is what `--with-dashboard` serves today, and
 * overwriting it would red-line all 18 dashboard test files at once instead of letting them stay
 * meaningful while v2 is built.
 */
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
import tailwind from "@tailwindcss/vite";

/**
 * The embedded bundle must contain `index.html`: `crates/rgctl-dashboard/src/bundle.rs` looks for that
 * exact name to decide a bundle is complete (lines 23 and 27) and rewrites it to inject the manifest
 * (line 103). The v2 entry is therefore excluded from `dist-v2` under a different name and re-emitted
 * as `index.html`.
 *
 * It is an explicit plugin rather than a source file called `index.html` because v1 already owns that
 * name in this directory, and two entries cannot both be it.
 */
function renameV2Entry() {
  return {
    name: "rgctl-v2-entry-name",
    enforce: "post" as const,
    generateBundle(_options: unknown, bundle: Record<string, { fileName: string }>) {
      const entry = bundle["index.v2.html"];
      if (!entry) return;
      delete bundle["index.v2.html"];
      entry.fileName = "index.html";
      bundle["index.html"] = entry;
    },
  };
}

export default defineConfig({
  plugins: [react(), tailwind(), renameV2Entry()],
  /**
   * `jsxImportSource` is set here as well as in `tsconfig.v2.json`, and this is not redundancy.
   *
   * `dashboard/tsconfig.json` sets `"jsxImportSource": "preact"` and includes `src`, so any `.tsx` under
   * `src/` — including `src/v2/` — compiles its JSX through **Preact's** jsx-runtime unless told
   * otherwise. The result is not a build error: it is Preact vnodes handed to real React, which throws
   * `Minified React error #31` (objects are not valid as a child) at runtime, naming a key list full of
   * Preact's `__k`/`__b`/`__v` internals.
   *
   * Which tsconfig a bundler picks up for a file that two configs both claim is a resolution detail, so
   * this is stated here rather than left to precedence. It cost an hour to find.
   */
  esbuild: { jsx: "automatic", jsxImportSource: "react" },
  // Relative asset paths: the bundle is embedded and served from `.rgctl/dashboard/`, and must also
  // work from `file://`.
  base: "./",
  build: {
    outDir: "dist-v2",
    assetsDir: "assets",
    emptyOutDir: true,
    rollupOptions: {
      // Explicit entry, because v1's `index.html` would otherwise be the one Vite picks up.
      input: "index.v2.html",
      output: {
        manualChunks(id) {
          if (id.includes("node_modules")) {
            if (id.includes("@tanstack")) return "tanstack";
            if (id.includes("@radix-ui")) return "radix";
            if (id.includes("/react") || id.includes("/react-dom") || id.includes("/scheduler")) {
              return "react";
            }
          }
        },
      },
    },
  },
});
