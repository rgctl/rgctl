import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";

async function main() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
  await page.getByRole("button", { name: /Dataflow/i }).click();
  await page.getByPlaceholder("Search functions…").fill("toMethodNodeFromQuery");
  await page.waitForTimeout(400);
  await page.locator(".function-list-item").first().click();
  await page.waitForTimeout(2000);

  const dataflowSide = await page.locator(".analysis-graph-side").count();
  if (dataflowSide !== 1) throw new Error("statements panel missing in dataflow view");

  await page.selectOption("#df-view", "dominator");
  await page.waitForTimeout(2000);

  const dominatorSide = await page.locator(".analysis-graph-side").count();
  const statementRows = await page.locator(".dataflow-source-panel tbody tr").count();
  if (dominatorSide !== 1) throw new Error("statements panel hidden in dominator view");
  if (statementRows < 10) throw new Error(`expected PDG statements in panel, got ${statementRows}`);

  await page.selectOption("#df-view", "dataflow");
  await page.waitForTimeout(1500);

  await page.locator(".dataflow-source-panel tbody tr").first().click();
  await page.waitForTimeout(300);
  let highlighted = await page.locator(".dataflow-source-panel tbody tr.table-primary").count();
  const selectedId = await page.locator(".dataflow-source-panel").getAttribute("data-selected-id");
  if (highlighted < 1 || !selectedId) {
    throw new Error(`statement click did not select row (highlighted=${highlighted}, id=${selectedId})`);
  }

  await page.selectOption("#df-view", "dominator");
  await page.waitForTimeout(1500);
  await page.locator(".dataflow-source-panel tbody tr").nth(2).click();
  await page.waitForTimeout(300);
  highlighted = await page.locator(".dataflow-source-panel tbody tr.table-primary").count();
  if (highlighted < 1) throw new Error("dominator statement click did not highlight block lines");

  console.log(
    JSON.stringify({ dataflowSide, dominatorSide, statementRows, highlighted }, null, 2),
  );
  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
