import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

async function sigmaMetrics(page) {
  return page.evaluate(() => {
    const host = document.querySelector(".sigma-host");
    if (!host) return { found: false };
    const rect = host.getBoundingClientRect();
    const canvas = host.querySelector("canvas");
    const sigmaNodes = document.querySelectorAll('[class*="sigma-"]').length;
    return {
      found: true,
      hostW: rect.width,
      hostH: rect.height,
      canvasW: canvas?.width ?? 0,
      canvasH: canvas?.height ?? 0,
      sigmaClassNodes: sigmaNodes,
      wrapH: host.closest(".analysis-graph-canvas-wrap, .cfg-graph-wrap, .dataflow-graph-wrap")
        ?.getBoundingClientRect().height ?? 0,
    };
  });
}

async function waitForDataflowGraph(page) {
  await page.locator(".dataflow-graph-panel").waitFor({ state: "visible", timeout: 20000 });
  await page.waitForFunction(
    () => {
      const host = document.querySelector(".dataflow-graph-panel .sigma-host");
      const canvas = host?.querySelector("canvas");
      return Boolean(host && canvas && canvas.height >= 32);
    },
    { timeout: 20000 },
  );
  await page.waitForTimeout(800);
}

async function pickDataflowFunction(page) {
  const specific = page.locator(".function-list-item", {
    has: page.locator(".function-list-item-name", { hasText: "addEmbeddingSimilarityEdges" }),
  });
  if ((await specific.count()) > 0) {
    return specific.first();
  }
  return page.locator(".function-list-item").first();
}

async function testDataflowVariableFilter(page) {
  await page.getByRole("button", { name: "Dataflow", exact: true }).click();
  await page.waitForTimeout(400);

  const fn = await pickDataflowFunction(page);
  const fnName = await fn.locator(".function-list-item-name").textContent();
  await fn.click();
  await page.waitForTimeout(400);

  const varSelect = page.locator("#df-var");
  await varSelect.waitFor({ state: "visible", timeout: 10000 });
  const options = await varSelect.locator("option").evaluateAll((nodes) =>
    nodes.map((n) => n.getAttribute("value") ?? "").filter(Boolean),
  );
  if (options.length > 0) {
    await varSelect.selectOption(options[0]);
  }
  await waitForDataflowGraph(page);

  const header = await page.locator(".dataflow-graph-panel .border-bottom").first().textContent();
  const metrics = await sigmaMetrics(page);
  return { header, metrics, variable: options[0] ?? "", fnName: fnName?.trim() ?? "" };
}

async function testDataflow(page) {
  await page.getByRole("button", { name: "Dataflow", exact: true }).click();
  await page.waitForTimeout(500);

  const firstFn = page.locator(".function-list-item").first();
  await firstFn.waitFor({ state: "visible", timeout: 10000 });
  const fnName = await firstFn.locator(".function-list-item-name").textContent();
  await firstFn.click();
  await waitForDataflowGraph(page);

  const metrics = await sigmaMetrics(page);
  const panel = await page.locator(".dataflow-graph-panel").count();
  const webglErrors = await page.evaluate(() => {
    return window.__webglErrors ?? [];
  });

  return { tab: "dataflow", fnName, panel, metrics, webglErrors };
}

async function testBlastAutoCompute(page) {
  await page.getByRole("button", { name: "Blast Radius", exact: true }).click();
  await page.waitForTimeout(500);

  const computeBtn = page.getByRole("button", { name: "Compute blast radius" });
  if ((await computeBtn.count()) > 0) {
    throw new Error("Compute blast radius button should be removed");
  }

  const firstFn = page.locator(".function-list-item").first();
  await firstFn.waitFor({ state: "visible", timeout: 10000 });
  const fnName = await firstFn.locator(".function-list-item-name").textContent();
  await firstFn.click();

  await page.getByText("Callers of", { exact: false }).waitFor({ state: "visible", timeout: 15000 });

  await page.locator("#blast-depth").fill("7");
  await page.waitForTimeout(500);

  await page.waitForFunction(
    () => {
      const el = document.querySelector(".card-body .fs-4.fw-semibold.text-primary");
      return el && el.textContent && el.textContent.trim().length > 0;
    },
    { timeout: 15000 },
  );

  return {
    fnName,
    headerText: await page.locator(".card-header").filter({ hasText: "Callers of" }).textContent(),
    rowCount: await page.locator("table tbody tr").count(),
  };
}

async function testCfg(page) {
  await page.getByRole("button", { name: "CFG / PDG Analysis" }).click();
  await page.waitForTimeout(500);

  const firstFn = page.locator(".function-list-item").first();
  await firstFn.waitFor({ state: "visible", timeout: 10000 });
  const fnName = await firstFn.locator(".function-list-item-name").textContent();
  const cfgIndex = await page.evaluate(async () => {
    const res = await fetch("./cfg_index.json");
    return res.json();
  });
  const archiveOnly = cfgIndex.detail_mode === "archive_only";
  const fnCount = cfgIndex.function_count ?? 0;

  if (!archiveOnly) {
    await firstFn.click();
    await page.waitForTimeout(2000);
  }

  const metrics = archiveOnly ? { found: true, archiveOnly: true } : await sigmaMetrics(page);
  const panel = await page.locator(".cfg-graph-panel").count();

  return { tab: "cfg", fnName, panel, metrics, archiveOnly, fnCount };
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

page.on("console", (msg) => {
  const t = msg.text();
  if (/WebGL|GL_INVALID|texImage2D|Framebuffer/i.test(t)) {
    console.log("CONSOLE:", t.slice(0, 200));
  }
});

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(2000);

const dataflow = await testDataflow(page);
const dataflowVar = await testDataflowVariableFilter(page);
const blast = await testBlastAutoCompute(page);
const cfg = await testCfg(page);

console.log(JSON.stringify({ dataflow, dataflowVar, blast, cfg }, null, 2));

const ok =
  dataflow.metrics.found &&
  dataflow.metrics.hostH >= 32 &&
  dataflow.metrics.hostH <= 4096 &&
  dataflow.metrics.canvasH >= 32 &&
  dataflow.metrics.canvasH <= 4096 &&
  dataflowVar.metrics.found &&
  dataflowVar.metrics.hostH >= 32 &&
  dataflowVar.metrics.canvasH >= 32 &&
  (dataflowVar.header?.trim().length ?? 0) > 0 &&
  blast.rowCount > 0 &&
  blast.headerText?.includes("Callers of") &&
  (cfg.archiveOnly
    ? cfg.fnCount > 0 && cfg.metrics.found
    : cfg.metrics.found &&
      cfg.metrics.hostH >= 32 &&
      cfg.metrics.hostH <= 4096 &&
      cfg.metrics.canvasH >= 32 &&
      cfg.metrics.canvasH <= 4096);

await browser.close();
process.exit(ok ? 0 : 1);
