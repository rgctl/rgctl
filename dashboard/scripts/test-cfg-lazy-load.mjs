import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(1500);

await page.getByRole("button", { name: "CFG / PDG Analysis", exact: true }).click();
await page.waitForSelector(".function-list-item", { timeout: 20000 });
await page.locator(".function-list-item").first().click();
await page.waitForTimeout(400);

const warning = await page.locator(".alert-warning", { hasText: "Large repository" }).count();
const loadBtn = await page.getByRole("button", { name: /Load CFG graph/ }).count();
await page.getByRole("button", { name: /Load CFG graph/ }).click();
await page.waitForSelector(".cfg-graph-panel", { timeout: 30000 });
const sigma = await page.locator(".cfg-graph-wrap canvas").count();

console.log(JSON.stringify({ warning, loadBtn, sigma }, null, 2));

await browser.close();
process.exit(warning === 1 && loadBtn === 1 && sigma >= 1 ? 0 : 1);
