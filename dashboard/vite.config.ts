import { defineConfig } from "vite";
import preact from "@preact/preset-vite";

export default defineConfig({
  plugins: [preact()],
  base: "./",
  build: {
    outDir: "dist",
    assetsDir: "assets",
    emptyOutDir: true,
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (id.includes("node_modules")) {
            if (id.includes("@codemirror") || id.includes("/codemirror/")) {
              return "codemirror";
            }
            if (id.includes("graphology-layout-forceatlas2")) {
              return "graph-layout";
            }
            if (id.includes("/sigma/")) {
              return "sigma";
            }
          }
        },
      },
    },
  },
  worker: {
    format: "es",
  },
});
