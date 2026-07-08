import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";
const FN = "addEmbeddingSimilarityEdges";

async function main() {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  const errors = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") errors.push(msg.text());
  });
  page.on("pageerror", (err) => errors.push(String(err)));

  await page.setViewportSize({ width: 1400, height: 900 });
  await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
  await page.getByRole("button", { name: /Dataflow/i }).click();
  await page.getByPlaceholder("Search functions…").fill(FN);
  await page.waitForTimeout(400);
  await page.locator(".function-list-item").first().click();
  await page.waitForTimeout(2500);

  const dataflow = await page.evaluate(() => {
    const title = document.querySelector(".dataflow-graph-panel .fw-semibold")?.textContent;
    const canvas = document.querySelector(".dataflow-graph-wrap canvas");
    const wrap = document.querySelector(".dataflow-graph-wrap");
    const host = document.querySelector(".dataflow-graph-wrap .sigma-host");
    const rows = document.querySelectorAll(".dataflow-source-panel tbody tr").length;
    return {
      title,
      rows,
      hasCanvas: !!canvas,
      wrapH: wrap?.getBoundingClientRect().height ?? 0,
      hostH: host?.getBoundingClientRect().height ?? 0,
      emptyMsg: document.querySelector(".dataflow-graph-panel p.text-muted")?.textContent ?? null,
    };
  });

  await page.selectOption("#df-view", "dominator");
  await page.waitForTimeout(2000);
  const dominator = await page.evaluate(() => ({
    title: document.querySelector(".dataflow-graph-panel .fw-semibold")?.textContent,
    hasCanvas: !!document.querySelector(".dataflow-graph-wrap canvas"),
  }));

  await page.selectOption("#df-view", "dataflow");
  await page.waitForTimeout(2000);
  const dataflowAgain = await page.evaluate(() => ({
    title: document.querySelector(".dataflow-graph-panel .fw-semibold")?.textContent,
    hasCanvas: !!document.querySelector(".dataflow-graph-wrap canvas"),
    wrapH: document.querySelector(".dataflow-graph-wrap")?.getBoundingClientRect().height ?? 0,
  }));

  console.log(JSON.stringify({ dataflow, dominator, dataflowAgain, errors }, null, 2));
  await page.screenshot({ path: "/tmp/addEmbedding-dataflow.png" });
  await browser.close();

  if (!dataflow.title?.includes("13 data")) {
    throw new Error(`unexpected dataflow title: ${dataflow.title}`);
  }
  if (dataflow.rows < 10) throw new Error(`expected statements, got ${dataflow.rows}`);
  if (!dataflow.hasCanvas) throw new Error("dataflow mode: no canvas");
  if (dataflow.wrapH < 32) throw new Error(`dataflow wrap too small: ${dataflow.wrapH}`);
  if (!dominator.hasCanvas) throw new Error("dominator mode: no canvas");
  if (!dataflowAgain.hasCanvas) throw new Error("dataflow after dominator: no canvas");
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
