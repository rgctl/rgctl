import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

const EXPECTED_SECTIONS = [
  "Graph visualization",
  "Function inventory",
  "CFG / PDG analysis",
  "Dataflow",
  "Program slicing",
  "Blast radius",
  "Taint analysis",
  "Migration planner",
  "GQL reference",
];

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.getByRole("button", { name: "Query Guide", exact: true }).click();
await page.waitForSelector(".guide-view", { timeout: 15000 });

const found = [];
for (const title of EXPECTED_SECTIONS) {
  const count = await page.locator(".guide-view .card-header", { hasText: title }).count();
  if (count > 0) found.push(title);
}

const prereq = await page.locator(".guide-view", { hasText: "rbuilder discover ." }).count();
const blastDepth = await page.locator(".guide-view", { hasText: "--depth 5" }).count();

console.log(JSON.stringify({ found: found.length, expected: EXPECTED_SECTIONS.length, prereq, blastDepth }, null, 2));

await browser.close();
process.exit(
  found.length === EXPECTED_SECTIONS.length && prereq >= 1 && blastDepth >= 1 ? 0 : 1,
);
