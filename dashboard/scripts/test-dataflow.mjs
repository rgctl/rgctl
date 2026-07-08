import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";

async function main() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(msg.text());
  });
  page.on("pageerror", (err) => errors.push(String(err)));

  await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
  await page.waitForSelector(".rb-app", { timeout: 30000 });

  await page.getByRole("button", { name: /Dataflow/i }).click();
  await page.waitForTimeout(500);

  const emptyHint = await page.locator(".dataflow-view .text-muted").first().textContent();
  const indexOk = await page.evaluate(async () => {
    const r = await fetch("dataflow_index.json");
    const j = await r.json();
    return { ok: r.ok, available: j.available, count: j.function_count, first: j.functions?.[0] };
  });

  // Select first function in sidebar
  const firstItem = page.locator(".function-list-item").first();
  const hasItems = (await firstItem.count()) > 0;
  if (hasItems) {
    await firstItem.click();
    await page.waitForTimeout(2000);
  }

  const state = await page.evaluate(() => {
    const panel = document.querySelector(".dataflow-graph-panel");
    const wrap = document.querySelector(".dataflow-graph-wrap");
    const host = document.querySelector(".dataflow-graph-wrap .sigma-host");
    const canvas = document.querySelector(".dataflow-graph-wrap canvas");
    const title = document.querySelector(".dataflow-graph-panel .fw-semibold")?.textContent ?? null;
    const stage = document.querySelector(".analysis-graph-stage");
    return {
      hasPanel: !!panel,
      hasStage: !!stage,
      stageRect: stage ? stage.getBoundingClientRect() : null,
      wrapRect: wrap ? wrap.getBoundingClientRect() : null,
      hostRect: host ? host.getBoundingClientRect() : null,
      hasCanvas: !!canvas,
      canvasSize: canvas ? { w: canvas.width, h: canvas.height } : null,
      title,
      selectedItem: document.querySelector(".function-list-item.active")?.textContent?.trim() ?? null,
      errorAlert: document.querySelector(".dataflow-view .alert")?.textContent ?? null,
      emptyMsg: document.querySelector(".dataflow-view > p.text-muted")?.textContent ?? null,
    };
  });

  console.log(JSON.stringify({ emptyHint, indexOk, hasItems, state, errors }, null, 2));

  if (!indexOk.available) throw new Error("dataflow index not available");
  if (!hasItems) throw new Error("no functions in sidebar");
  if (!state.hasPanel) throw new Error("graph panel not rendered after selection");
  if (!state.hasCanvas) throw new Error("sigma canvas not mounted");

  const panel = await page.locator(".rb-tab-panel-body").boundingBox();
  const wrap = await page.locator(".dataflow-graph-wrap").boundingBox();
  if (!panel || !wrap) throw new Error("missing layout boxes");
  const visibleTop = Math.max(wrap.y, panel.y);
  const visibleBottom = Math.min(wrap.y + wrap.height, panel.y + panel.height);
  const visibleHeight = visibleBottom - visibleTop;
  if (visibleHeight < 120) {
    throw new Error(`graph mostly clipped in tab panel (visible height ${visibleHeight})`);
  }

  await browser.close();
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
