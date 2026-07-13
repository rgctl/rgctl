import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

async function sliceCfgChecks(page) {
  await page.getByRole("button", { name: "Program Slicing", exact: true }).click();
  await page.waitForTimeout(800);
  const sliceUnavailable = await page.getByText("Slice bundles require CFG/PDG").count();
  const sliceItems = await page.locator(".function-list-item").count();

  await page.getByRole("button", { name: "CFG / PDG Analysis", exact: true }).click();
  await page.waitForTimeout(800);
  const cfgUnavailable = await page.getByText("CFG bundles require").count();
  const cfgItems = await page.locator(".function-list-item").count();

  const cfgIndex = await page.evaluate(async () => {
    const res = await fetch("./cfg_index.json");
    return res.json();
  });

  if (cfgIndex.detail_mode === "archive_only") {
    return {
      sliceUnavailable,
      sliceItems,
      cfgUnavailable,
      cfgItems,
      cfgSigma: true,
      cfgArchiveOnly: true,
    };
  }

  if (cfgItems > 0) {
    await page.locator(".function-list-item").first().click();
    await page.waitForTimeout(1500);
  }
  const cfgSigma = await page.evaluate(() => {
    const host = document.querySelector(".cfg-graph-panel .sigma-host");
    const canvas = host?.querySelector("canvas");
    return Boolean(host && canvas && canvas.height >= 32);
  });

  return { sliceUnavailable, sliceItems, cfgUnavailable, cfgItems, cfgSigma, cfgArchiveOnly: false };
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 120000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(2000);

const graphSigma = await page.evaluate(() => {
  const host = document.querySelector(".sigma-host");
  const canvas = host?.querySelector("canvas");
  return Boolean(host && canvas && canvas.height >= 32);
});

const manifest = await page.evaluate(async () => {
  const res = await fetch("./manifest.json");
  const m = await res.json();
  return {
    slice: m.analysis?.slice_available,
    cfg: m.analysis?.cfg_available,
    sliceCount: m.analysis?.slice_function_count,
  };
});

const sliceCfg = await sliceCfgChecks(page);

console.log(JSON.stringify({ graphSigma, manifest, sliceCfg }, null, 2));

await browser.close();

const ok =
  graphSigma &&
  manifest.slice === true &&
  manifest.cfg === true &&
  manifest.sliceCount > 10000 &&
  sliceCfg.sliceUnavailable === 0 &&
  sliceCfg.sliceItems > 0 &&
  sliceCfg.cfgUnavailable === 0 &&
  sliceCfg.cfgItems > 0 &&
  sliceCfg.cfgSigma;

process.exit(ok ? 0 : 1);
