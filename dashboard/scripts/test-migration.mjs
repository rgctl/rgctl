import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

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
      };
    }),
  );
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(1200);

await page.getByRole("button", { name: "Migration", exact: true }).click();
await page.waitForSelector(".migration-view", { timeout: 15000 });

const tuningVisible = await page.getByRole("heading", { name: "Metrics & tuning" }).count();
const graphHeadingVisible = await page.getByRole("heading", { name: "Package graph" }).count();
const tableHeadingVisible = await page.locator(".migration-table-section h2").count();
const columnHelpCount = await page.locator(".migration-table-section .functions-col-help").count();

await page.waitForSelector(".migration-view tbody tr", { timeout: 10000 });
const initialRows = await roadmapRows(page);
const paginationText = await page.locator(".migration-table-section .function-list-pagination span").textContent();

await page.locator('.migration-view select.form-select[aria-label="Strategy preset"]').selectOption("foundational_first");
await page.waitForTimeout(400);
const afterPresetRows = await roadmapRows(page);

const firstPriorityChanged =
  initialRows.length > 0 &&
  afterPresetRows.length > 0 &&
  initialRows[0].priority !== afterPresetRows[0].priority;

await page.waitForTimeout(1200);
const graphMetrics = await sigmaMetrics(page);

const alphaSlider = page.locator('.migration-view input[type="range"]').first();
await alphaSlider.fill("0.9");
await page.waitForTimeout(300);
const customSelected =
  (await page.locator('.migration-view select.form-select[aria-label="Strategy preset"]').inputValue()) === "custom";

await page.waitForTimeout(800);
const graphAfterTuning = await sigmaMetrics(page);

const report = {
  unifiedLayout: { tuningVisible, graphHeadingVisible, tableHeadingVisible, columnHelpCount },
  roadmapRowCount: initialRows.length,
  paginationText: paginationText?.trim() ?? "",
  topRoadmap: initialRows.slice(0, 3),
  afterPresetTop: afterPresetRows.slice(0, 3),
  presetChangedOrderOrScore: firstPriorityChanged || initialRows[0]?.community !== afterPresetRows[0]?.community,
  graph: graphMetrics,
  customPresetSelected: customSelected,
  graphAfterTuning,
};

console.log(JSON.stringify(report, null, 2));
await browser.close();

const ok =
  report.unifiedLayout.tuningVisible > 0 &&
  report.unifiedLayout.graphHeadingVisible > 0 &&
  report.unifiedLayout.tableHeadingVisible > 0 &&
  report.unifiedLayout.columnHelpCount >= 8 &&
  report.roadmapRowCount > 0 &&
  report.graph.found &&
  report.graph.canvasW > 0 &&
  report.graph.canvasH > 0 &&
  report.customPresetSelected &&
  report.graphAfterTuning.found;

if (!ok) {
  console.error("Migration UI test failed");
  process.exit(1);
}
