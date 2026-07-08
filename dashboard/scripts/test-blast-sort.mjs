import { chromium } from "playwright";

const BASE = process.env.DASHBOARD_URL ?? "http://localhost:8765/";

function parseImpactScore(meta) {
  const match = (meta ?? "").match(/score (\d+(?:\.\d+)?)/);
  return match ? Number(match[1]) : 0;
}

function checkDescending(scores) {
  const violations = [];
  for (let i = 1; i < scores.length; i++) {
    if (scores[i] > scores[i - 1]) {
      violations.push({ index: i, prev: scores[i - 1], curr: scores[i] });
    }
  }
  return violations;
}

async function extractBlastList(page) {
  return page.locator(".function-list-item").evaluateAll((nodes) =>
    nodes.map((node) => {
      const name = node.querySelector(".function-list-item-name")?.textContent?.trim() ?? "";
      const meta = node.querySelector(".function-list-item-meta")?.textContent?.trim() ?? "";
      const scoreMatch = meta.match(/score (\d+(?:\.\d+)?)/);
      return {
        name,
        meta,
        impactScore: scoreMatch ? Number(scoreMatch[1]) : 0,
      };
    }),
  );
}

const browser = await chromium.launch({ headless: true });
const page = await browser.newPage();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 60000 });
await page.waitForSelector(".rb-app", { timeout: 30000 });
await page.waitForTimeout(1500);

await page.getByRole("button", { name: "Blast Radius", exact: true }).click();
await page.locator(".function-list-item").first().waitFor({ state: "visible", timeout: 15000 });
await page.waitForTimeout(800);

const list = await extractBlastList(page);
const scores = list.map((item) => item.impactScore);
const violations = checkDescending(scores);
const withScore = list.filter((item) => item.impactScore > 0);

const paginationVisible = await page.getByRole("button", { name: "Next page" }).count();

const report = {
  totalListed: list.length,
  maxPerPage: 30,
  withImpactScore: withScore.length,
  zeroImpactScore: list.length - withScore.length,
  top10: list.slice(0, 10),
  sortViolations: violations.slice(0, 10),
  sortOk: violations.length === 0,
  paginationVisible,
};

console.log(JSON.stringify(report, null, 2));

await browser.close();

const ok = report.sortOk && report.withImpactScore >= 10 && report.totalListed <= 30 && report.paginationVisible > 0;
process.exit(ok ? 0 : 1);
