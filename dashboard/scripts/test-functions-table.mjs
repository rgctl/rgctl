import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

async function tableRows(page) {
  return page.locator(".functions-view table tbody tr").evaluateAll((trs) =>
    trs.map((tr) => [...tr.querySelectorAll("td")].map((td) => td.textContent?.trim() ?? "")),
  );
}

function parsePr(cell) {
  if (cell === "—") return 0;
  if (cell.includes("e")) return Number(cell);
  return Number(cell);
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.getByRole("button", { name: "Functions", exact: true }).click();
await page.waitForSelector(".functions-view table tbody tr", { timeout: 15000 });

const defaultRows = await tableRows(page);
const topName = defaultRows[0]?.[0] ?? "";
const topPr = parsePr(defaultRows[0]?.[2] ?? "0");

await page.getByPlaceholder("Search by name or file…").fill("parseFile");
await page.waitForTimeout(400);
const searchRows = await tableRows(page);
const searchHit = searchRows.find((r) => r[0] === "parseFile");

await page.getByPlaceholder("Search by name or file…").fill("");
await page.waitForTimeout(200);
await page.getByRole("button", { name: "Sort by Name" }).click();
await page.waitForTimeout(300);
const nameSorted = await tableRows(page);

const prHelp = page
  .locator(".functions-sort-header")
  .filter({ has: page.getByRole("button", { name: "Sort by PR" }) })
  .locator(".functions-col-help");
await prHelp.hover();
await page.waitForTimeout(200);

const tooltipLayout = await page.evaluate(() => {
  const help = document.querySelector(".functions-col-help:hover") || document.querySelector(".functions-col-help");
  const popup = help?.querySelector(".functions-col-help-popup");
  const search = document.querySelector(".functions-search");
  if (!popup || !search || !help) return null;
  const p = popup.getBoundingClientRect();
  const s = search.getBoundingClientRect();
  const h = help.getBoundingClientRect();
  const overlapsSearch = !(p.bottom < s.top || p.top > s.bottom || p.right < s.left || p.left > s.right);
  return {
    overlapsSearch,
    popupBelowSearch: p.top >= s.bottom - 1,
    popupBelowHelp: p.top >= h.bottom - 1,
  };
});

const report = {
  defaultTopName: topName,
  defaultTopPr: topPr,
  defaultSortByPr: topPr > 0.005,
  searchFoundParseFile: Boolean(searchHit),
  parseFilePr: searchHit?.[2],
  nameSortAscending:
    nameSorted.length >= 2 ? nameSorted[0][0].localeCompare(nameSorted[1][0]) <= 0 : true,
  prHeaderTooltip: await prHelp.locator(".functions-col-help-popup").textContent(),
  infoIcons: await page.locator(".functions-col-help").count(),
  ...tooltipLayout,
};

console.log(JSON.stringify(report, null, 2));
await browser.close();

const ok =
  report.defaultSortByPr &&
  report.searchFoundParseFile &&
  report.nameSortAscending &&
  (report.prHeaderTooltip?.includes("normalized") ?? false) &&
  report.infoIcons === 6 &&
  report.overlapsSearch === false &&
  report.popupBelowSearch === true;

process.exit(ok ? 0 : 1);
