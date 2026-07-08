import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";

async function waitForServer() {
  const deadline = Date.now() + 30000;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(`${BASE.replace(/\/$/, "")}/api/health`);
      if (res.ok) return;
    } catch {
      // retry
    }
    await new Promise((r) => setTimeout(r, 200));
  }
  throw new Error(`server not ready at ${BASE}`);
}

async function testQueryApi() {
  const res = await fetch(`${BASE.replace(/\/$/, "")}/api/query`, {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({ macro: "all_functions" }),
  });
  const body = await res.json();
  return { ok: res.ok, hasRows: Array.isArray(body.rows), count: body.count };
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await waitForServer();

const health = await fetch(`${BASE.replace(/\/$/, "")}/api/health`).then((r) => r.json());
const query = await testQueryApi();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(1500);

const title = await page.title();
const wasmScript = await page.locator('script[type="module"]').count();
const graphTab = await page.getByRole("button", { name: "Graph Visualization", exact: true }).count();

await page.getByRole("button", { name: "System notifications" }).click();
const wasmReady = await page.locator(".rb-notifications-list", { hasText: "WASM engine ready" }).count();
await page.keyboard.press("Escape");

await page.getByRole("button", { name: "Blast Radius", exact: true }).click();
await page.locator(".function-list-item").first().waitFor({ state: "visible", timeout: 15000 });
const blastListCount = await page.locator(".function-list-item").count();
const docPanel = await page.locator(".rb-tab-doc-panel").count();

const report = {
  base: BASE,
  health,
  query,
  title,
  wasmScript,
  graphTab,
  wasmReady,
  blastListCount,
  docPanel,
};

console.log(JSON.stringify(report, null, 2));

await browser.close();

const ok =
  health?.status === "ok" &&
  query.ok &&
  query.hasRows &&
  graphTab === 1 &&
  wasmReady === 1 &&
  blastListCount > 0 &&
  blastListCount <= 30 &&
  docPanel === 1;

process.exit(ok ? 0 : 1);
