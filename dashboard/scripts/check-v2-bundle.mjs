/**
 * Guard the v2 critical path.
 *
 * The dashboard must paint from the static skeleton and only load React/TanStack/CSS before the first
 * room. A regression that imports Radix, the palette, Understand, or Three.js into the shell should fail
 * here instead of quietly shipping a slower dashboard.
 */
import { readFileSync, statSync } from "node:fs";
import { gzipSync } from "node:zlib";
import { join } from "node:path";

const root = new URL("..", import.meta.url).pathname;
const dist = join(root, "dist-v2");
const html = readFileSync(join(dist, "index.html"), "utf8");

const fail = (message) => {
  console.error(`v2 bundle check: ${message}`);
  process.exit(1);
};

if (!html.includes("boot-shell")) fail("index.html is missing the pre-baked shell skeleton");

const entry = html.match(/<script[^>]+src="\.\/assets\/([^"]+)"/)?.[1];
if (!entry) fail("could not find the entry script");

const preloads = [...html.matchAll(/href="\.\/assets\/([^"]+)"/g)].map((m) => m[1]);
const forbidden = /(radix|Palette|Understand|three|OrbitControls|Taint|Architecture|Migration)/i;
for (const file of [entry, ...preloads]) {
  if (forbidden.test(file)) fail(`critical chunk pulled in early: ${file}`);
}

const critical = [entry, ...preloads].reduce((sum, file) => {
  const path = join(dist, "assets", file);
  return sum + gzipSync(readFileSync(path), { level: 9 }).byteLength;
}, 0);

const budget = 140 * 1024;
if (critical > budget) {
  fail(`critical gzip ${(critical / 1024).toFixed(1)} KiB exceeds ${(budget / 1024).toFixed(0)} KiB`);
}

console.log(
  `v2 bundle check: entry=${entry}, critical=${(critical / 1024).toFixed(1)} KiB, preloads=${preloads.length}`,
);
