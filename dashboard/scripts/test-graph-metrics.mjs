import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(1500);

const graphTabActive = await page.locator(".nav-link.active", { hasText: "Graph Visualization" }).count();
const statCards = await page.locator(".rb-stats-row .stat-card").count();
const totalNodes = await page.locator(".rb-stats-row .stat-label", { hasText: "Total Nodes" }).count();
const notificationsButton = await page.getByRole("button", { name: "System notifications" }).count();
await page.getByRole("button", { name: "System notifications" }).click();
const wasmReady = await page.locator(".rb-notifications-list", { hasText: "WASM engine ready" }).count();

console.log(
  JSON.stringify({ graphTabActive, statCards, totalNodes, notificationsButton, wasmReady }, null, 2),
);

await browser.close();
process.exit(
  graphTabActive === 1 && statCards >= 7 && totalNodes === 1 && notificationsButton === 1 && wasmReady === 1
    ? 0
    : 1,
);
