import { chromium } from "playwright";
import fs from "node:fs";
import path from "node:path";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";
const OUT_DIR = process.env.MIGRATION_SCREENSHOT_DIR ?? ".playwright-mcp";

async function roadmapRows(page) {
  return page.locator(".migration-view tbody tr").evaluateAll((rows) =>
    rows.map((row) => {
      const cells = [...row.querySelectorAll("td")].map((td) => td.textContent?.trim() ?? "");
      return {
        step: cells[0] ?? "",
        schedule: cells[1] ?? "",
        rank: cells[2] ?? "",
        community: cells[3] ?? "",
        priority: Number(cells[4] ?? "0"),
        avgPr: cells[5] ?? "",
        maxBlast: cells[7] ?? "",
      };
    }),
  );
}

async function sigmaMetrics(page) {
  return page.evaluate(() => {
    const host = document.querySelector(".migration-graph-panel .sigma-host");
    if (!host) return { found: false };
    const rect = host.getBoundingClientRect();
    const canvas = host.querySelector("canvas");
    return {
      found: true,
      hostW: rect.width,
      hostH: rect.height,
      canvasW: canvas?.width ?? 0,
      canvasH: canvas?.height ?? 0,
    };
  });
}

fs.mkdirSync(OUT_DIR, { recursive: true });

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

await page.goto(BASE, { waitUntil: "networkidle", timeout: 120000 });
await page.waitForSelector(".rb-app", { timeout: 60000 });
await page.waitForTimeout(2000);

const tabCount = await page.getByRole("button", { name: "Migration", exact: true }).count();
if (tabCount === 0) {
  const tabs = await page.locator(".rb-main-tabs .nav-link").evaluateAll((els) =>
    els.map((e) => e.textContent?.trim() ?? ""),
  );
  console.log(JSON.stringify({ error: "Migration tab missing", tabs }, null, 2));
  await page.screenshot({ path: path.join(OUT_DIR, "gbuilder-no-migration-tab.png"), fullPage: true });
  await browser.close();
  process.exit(1);
}

await page.getByRole("button", { name: "Migration", exact: true }).click();
await page.waitForSelector(".migration-view", { timeout: 30000 });
await page.waitForSelector(".migration-view tbody tr", { timeout: 30000 });

const hybridRows = await roadmapRows(page);
const hybridPagination = await page.locator(".migration-table-section .function-list-pagination span").textContent();
await page.screenshot({ path: path.join(OUT_DIR, "gbuilder-migration-roadmap-hybrid.png"), fullPage: true });

await page.locator('.migration-view select.form-select[aria-label="Strategy preset"]').selectOption("risk_mitigation");
await page.waitForTimeout(500);

const riskRows = await roadmapRows(page);
await page.screenshot({ path: path.join(OUT_DIR, "gbuilder-migration-roadmap-risk.png"), fullPage: true });

const orderChanged =
  hybridRows.length > 0 &&
  riskRows.length > 0 &&
  (hybridRows[0].community !== riskRows[0].community ||
    hybridRows[0].priority !== riskRows[0].priority);

await page.waitForTimeout(2000);
const graph = await sigmaMetrics(page);
await page.screenshot({ path: path.join(OUT_DIR, "gbuilder-migration-graph.png"), fullPage: true });

const manifestMigration = await page.evaluate(async () => {
  const embedded = document.getElementById("rbuilder-manifest");
  if (embedded?.textContent) {
    const m = JSON.parse(embedded.textContent);
    return m.analysis?.migration_available ?? null;
  }
  const res = await fetch("./manifest.json");
  const m = await res.json();
  return m.analysis?.migration_available ?? null;
});

const totalCommunities = Number((hybridPagination ?? "").match(/of\s+([\d,]+)/)?.[1]?.replace(/,/g, "") ?? "0");

const report = {
  base: BASE,
  migrationTabFound: true,
  manifestMigrationAvailable: manifestMigration,
  pageRowCount: hybridRows.length,
  totalCommunities,
  hybridPagination: hybridPagination?.trim() ?? "",
  hybridTop5: hybridRows.slice(0, 5),
  riskTop5: riskRows.slice(0, 5),
  presetChangedOrderOrScore: orderChanged,
  graph,
  screenshots: [
    "gbuilder-migration-roadmap-hybrid.png",
    "gbuilder-migration-roadmap-risk.png",
    "gbuilder-migration-graph.png",
  ],
};

console.log(JSON.stringify(report, null, 2));
await browser.close();

const ok =
  report.pageRowCount > 0 &&
  report.totalCommunities >= 300 &&
  report.presetChangedOrderOrScore &&
  report.graph.found &&
  report.graph.canvasW > 0;

process.exit(ok ? 0 : 1);
