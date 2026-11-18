/**
 * Guard the ownership boundary documented in docs/design/artifact-ownership.md.
 *
 * The browser may read dashboard presentation/transport files or HTTP APIs. It must not reach into
 * canonical CLI artifacts. The static shell must not point at removed v2 payloads.
 */
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const sourceRoot = join(root, "src/v2");
const forbidden = [
  /\.rgctl\/analysis/,
  /parked_relations\.bin/,
  /graph\.snapshot\.bin/,
  /analysis\.pack\.bin/,
  /content_store\.bin/,
];

const files = [];
const walk = (dir) => {
  for (const name of readdirSync(dir)) {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) walk(path);
    else if (/\.(ts|tsx)$/.test(name) && !path.includes("__tests__")) files.push(path);
  }
};
walk(sourceRoot);

for (const file of files) {
  const text = readFileSync(file, "utf8");
  for (const pattern of forbidden) {
    if (pattern.test(text)) {
      console.error(`artifact boundary: ${file} references canonical ${pattern}`);
      process.exit(1);
    }
  }
}

const html = readFileSync(join(root, "dist-v2/index.html"), "utf8");
for (const name of ["dataflow_index.json", "mutations_index.json", "migration_plan.json", "cfg_pdg.archive.bin"]) {
  if (html.includes(name)) {
    console.error(`artifact boundary: static shell references removed v2 payload ${name}`);
    process.exit(1);
  }
}

console.log(`artifact boundary check: ${files.length} v2 source files, shell clean`);
