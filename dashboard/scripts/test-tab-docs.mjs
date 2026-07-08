import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

const TABS = [
  { id: "graph", label: "Graph Visualization", defaultOpen: false, title: "Graph visualization" },
  { id: "blast", label: "Blast Radius", defaultOpen: true, title: "Blast radius" },
  { id: "cfg", label: "CFG / PDG Analysis", defaultOpen: true, title: "CFG / PDG analysis" },
  { id: "guide", label: "Query Guide", defaultOpen: true, title: "Query guide (GQL)" },
];

async function docPanelState(page) {
  const toggle = page.locator(".rb-tab-doc-panel .rb-tab-doc-toggle").first();
  const expanded = (await toggle.getAttribute("aria-expanded")) === "true";
  const bodyVisible = (await page.locator(".rb-tab-doc-body").count()) > 0;
  const title = await toggle.locator(".fw-semibold").textContent();
  return { expanded, bodyVisible, title: title?.trim() ?? "" };
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(1500);

const results = {};

for (const tab of TABS) {
  await page.getByRole("button", { name: tab.label, exact: true }).click();
  await page.waitForTimeout(400);

  const initial = await docPanelState(page);
  results[tab.id] = { initial };

  if (initial.title.toLowerCase() !== tab.title.toLowerCase()) {
    results[tab.id].titleMismatch = { expected: tab.title, actual: initial.title };
  }

  if (tab.defaultOpen) {
    if (!initial.expanded || !initial.bodyVisible) {
      results[tab.id].defaultOpenFailed = initial;
    }
    const goalVisible = await page.locator(".rb-tab-doc-body", { hasText: "Goal:" }).count();
    results[tab.id].goalVisible = goalVisible === 1;
  } else {
    if (initial.expanded || initial.bodyVisible) {
      results[tab.id].defaultClosedFailed = initial;
    }
    await page.locator(".rb-tab-doc-toggle").first().click();
    await page.waitForTimeout(200);
    const opened = await docPanelState(page);
    results[tab.id].afterOpen = opened;
    results[tab.id].openShowsGoal =
      opened.expanded && (await page.locator(".rb-tab-doc-body", { hasText: "Goal:" }).count()) === 1;
    await page.locator(".rb-tab-doc-toggle").first().click();
    await page.waitForTimeout(200);
    const closed = await docPanelState(page);
    results[tab.id].afterClose = closed;
  }
}

console.log(JSON.stringify(results, null, 2));

const ok = TABS.every((tab) => {
  const r = results[tab.id];
  if (r.titleMismatch) return false;
  if (tab.defaultOpen) return r.goalVisible && !r.defaultOpenFailed;
  return r.openShowsGoal && r.afterClose && !r.afterClose.expanded && !r.afterClose.bodyVisible;
});

await browser.close();
process.exit(ok ? 0 : 1);
