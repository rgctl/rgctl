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
  await page.waitForTimeout(2500);

  const state = await page.evaluate(() => {
    const title = document.querySelector(".dataflow-graph-panel .fw-semibold")?.textContent ?? null;
    const rows = document.querySelectorAll(".dataflow-source-panel tbody tr").length;
    const stage = document.querySelector(".analysis-graph-stage");
    const wrap = document.querySelector(".dataflow-graph-wrap");
    const panel = document.querySelector(".rb-tab-panel-body");
    const rect = (el) => (el ? el.getBoundingClientRect() : null);
    const s = rect(stage);
    const w = rect(wrap);
    const p = rect(panel);
    const visible =
      w && p
        ? Math.min(w.bottom, p.bottom) - Math.max(w.top, p.top)
        : 0;
    return {
      title,
      sourceRows: rows,
      hasCanvas: !!document.querySelector(".dataflow-graph-wrap canvas"),
      stageH: s?.height ?? 0,
      wrapH: w?.height ?? 0,
      visibleWrapInPanel: visible,
    };
  });

  console.log(JSON.stringify(state, null, 2));
  await browser.close();

  if (!state.title?.includes("106 data")) {
    throw new Error(`expected 106 data edges in title, got: ${state.title}`);
  }
  if (state.sourceRows < 10) {
    throw new Error(`expected 12 source rows, got ${state.sourceRows}`);
  }
  if (!state.hasCanvas) throw new Error("no canvas");
  if (state.visibleWrapInPanel < 120) {
    throw new Error(`graph clipped (visible ${state.visibleWrapInPanel}px)`);
  }
  if (state.stageH > 1200) {
    console.warn(`warning: stage tall (${state.stageH}px) but wrap visible ${state.visibleWrapInPanel}px`);
  }
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
