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

console.log(JSON.stringify({ graphTabActive, statCards, totalNodes }, null, 2));

await browser.close();
process.exit(graphTabActive === 1 && statCards >= 8 && totalNodes === 1 ? 0 : 1);
