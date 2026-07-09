import { chromium } from "playwright";
import fs from "node:fs";
import path from "node:path";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";
const OUT_DIR =
  process.env.MIGRATION_DOCS_SCREENSHOT_DIR ??
  path.resolve(import.meta.dirname, "../../docs/images/migration-planner");

fs.mkdirSync(OUT_DIR, { recursive: true });

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage({ viewport: { width: 1440, height: 900 } });

await page.goto(BASE, { waitUntil: "networkidle", timeout: 120000 });
await page.waitForSelector(".rb-app", { timeout: 60000 });
await page.waitForTimeout(1500);

await page.getByRole("button", { name: "Migration", exact: true }).click();
await page.waitForSelector(".migration-view", { timeout: 30000 });
await page.waitForSelector(".migration-view tbody tr", { timeout: 30000 });
await page.waitForTimeout(2500);

await page.screenshot({
  path: path.join(OUT_DIR, "migration-unified-roadmap.png"),
  fullPage: true,
});

const tuning = page.locator(".migration-tuning");
await tuning.scrollIntoViewIfNeeded();
await page.waitForTimeout(400);
await tuning.screenshot({ path: path.join(OUT_DIR, "migration-metrics-tuning.png") });

const graphSection = page.locator(".migration-graph-section");
await graphSection.scrollIntoViewIfNeeded();
await page.waitForTimeout(2000);
await graphSection.screenshot({ path: path.join(OUT_DIR, "migration-package-graph.png") });

const tableSection = page.locator(".migration-table-section");
await tableSection.scrollIntoViewIfNeeded();
await page.waitForTimeout(400);
await tableSection.screenshot({ path: path.join(OUT_DIR, "migration-packages-table.png") });

await page.locator('.migration-view select[aria-label="Roadmap sort order"]').selectOption("priority");
await page.waitForTimeout(500);
await tableSection.screenshot({ path: path.join(OUT_DIR, "migration-priority-rank-table.png") });

await page.locator('.migration-view select.form-select[aria-label="Strategy preset"]').selectOption("risk_mitigation");
await page.waitForTimeout(600);
await graphSection.scrollIntoViewIfNeeded();
await page.waitForTimeout(1500);
await graphSection.screenshot({ path: path.join(OUT_DIR, "migration-graph-risk-preset.png") });

const helpIcon = page.locator(".migration-table-section .functions-col-help").first();
await helpIcon.hover();
await page.waitForTimeout(300);
await tableSection.screenshot({ path: path.join(OUT_DIR, "migration-column-tooltip.png") });

const report = {
  base: BASE,
  outDir: OUT_DIR,
  files: fs.readdirSync(OUT_DIR).filter((f) => f.endsWith(".png")),
};

console.log(JSON.stringify(report, null, 2));
await browser.close();
