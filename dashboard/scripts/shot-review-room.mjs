/**
 * Capture the Review room from a real bundle.
 *
 * One-off, not a gate: the design doc's screenshot is evidence of what the room actually renders, and a
 * hand-taken screenshot from a mock is what the earlier ones were. Run with the bundle served:
 *   cd <repo>/.rgctl/dashboard && python3 -m http.server 8095 --bind 127.0.0.1
 *   node dashboard/scripts/shot-review-room.mjs <out.png>
 */
import { chromium } from "playwright";

const out = process.argv[2];
if (!out) throw new Error("usage: shot-review-room.mjs <out.png>");
const url = process.env.SHOT_URL ?? "http://127.0.0.1:8095/index.html";

// System Chrome rather than Playwright's downloaded build: the bundled browser is not installed here, and
// requiring a 150 MB download to take one screenshot of a local bundle is not a trade worth making.
const browser = await chromium.launch({ channel: "chrome" });
const page = await browser.newPage({ viewport: { width: 1500, height: 1100 }, deviceScaleFactor: 2 });
page.on("console", (m) => {
  if (m.type() === "error") console.error("[console]", m.text());
});
page.on("pageerror", (e) => console.error("[pageerror]", e.message));

await page.goto(url, { waitUntil: "networkidle" });
// The nav's Review destination is the button labelled by its own blurb.
await page.locator('button:has-text("what changed, what does it touch?")').first().click();
await page.waitForSelector("text=changed symbols", { timeout: 15000 });
await page.waitForTimeout(800);

const summary = await page.evaluate(() => {
  const stat = (label) => {
    const el = [...document.querySelectorAll("div")].find(
      (d) => d.firstElementChild?.textContent === label && d.children.length >= 2,
    );
    return el ? el.children[1].textContent : null;
  };
  return {
    changed: stat("changed symbols"),
    added: stat("call edges added"),
    removed: stat("removed"),
    url: location.pathname,
  };
});
console.log(JSON.stringify(summary));

await page.screenshot({ path: out });
await browser.close();
