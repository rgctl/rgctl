/**
 * Record an rgctl dashboard feature montage — one beat per main tab.
 *
 * Timing: prepare each tab, then hold a fixed showcase window. SRT cues use
 * wall-clock timestamps from those showcase windows (no equal-slot guessing).
 * Encode stays near 1× so captions stay aligned.
 *
 * Prereq (ecommerce-java fixture recommended):
 *   rgctl -r rgctl-tests/ecommerce-java discover . -l java -e target \
 *     --with-cfg --with-security --with-taint --with-dashboard --with-harmonic \
 *     --export-migration-hints
 *   rgctl -r rgctl-tests/ecommerce-java semantic index --embedder vocab
 *   rgctl -r rgctl-tests/ecommerce-java serve --port 8080
 *
 * Usage:
 *   DASHBOARD_URL=http://127.0.0.1:8080/ node dashboard/scripts/record-feature-demo.mjs
 *
 * Outputs:
 *   docs/videos/rgctl-feature-demo-no-captions.mp4
 *   docs/videos/rgctl-feature-demo.srt
 *   docs/videos/rgctl-feature-demo.raw.webm  (intermediate)
 */

import { chromium } from "playwright";
import fs from "node:fs";
import path from "node:path";
import { spawnSync } from "node:child_process";

const BASE = process.env.DASHBOARD_URL ?? "http://127.0.0.1:8080/";
const ROOT = path.resolve(import.meta.dirname, "../..");
const OUT_DIR = path.join(ROOT, "docs/videos");
const RAW_WEBM = path.join(OUT_DIR, "rgctl-feature-demo.raw.webm");
const OUT_NO_CAPTIONS = path.join(OUT_DIR, "rgctl-feature-demo-no-captions.mp4");
const OUT_SRT = path.join(OUT_DIR, "rgctl-feature-demo.srt");

/** Showcase hold per tab (after prep). Override with DEMO_HOLD_SEC. */
const HOLD_MS = Math.round(Number(process.env.DEMO_HOLD_SEC ?? "6.5") * 1000);
/** Cap total speedup so captions stay readable (1 = never speed up). */
const MAX_SPEEDUP = Number(process.env.DEMO_MAX_SPEEDUP ?? "1");

const FN = process.env.CAPTURE_FN_DATAFLOW ?? "checkout";
const FN_BLAST = process.env.CAPTURE_FN_BLAST ?? "clearCart";
const FN_TAINT = process.env.CAPTURE_FN_TAINT ?? "checkout";
const FN_CFG = process.env.CAPTURE_FN_CFG ?? "checkout";
const FN_SLICE = process.env.CAPTURE_FN_SLICE ?? "addItem";
const SEMANTIC_QUERY = process.env.CAPTURE_SEMANTIC_QUERY ?? "shopping cart checkout";
const SLICE_LINE = process.env.CAPTURE_SLICE_LINE ?? "53";
const SLICE_VAR = process.env.CAPTURE_SLICE_VAR ?? "item";
const MUTATIONS_TYPE = process.env.MUTATIONS_TYPE ?? "ShoppingCart";

/**
 * One segment per primary tab (Dataflow / Query Guide combine their features).
 * `caption` / `body` → SRT during the showcase hold only.
 */
const TAB_SEGMENTS = [
  {
    key: "overview",
    tab: null,
    panel: ".rb-stats-row",
    caption: "Overview",
    body: "Discover metrics · graph snapshot ready",
  },
  {
    key: "graph",
    tab: "Graph Visualization",
    panel: ".graph-panel.h-100",
    caption: "Graph",
    body: "Communities · metagraph overview",
  },
  {
    key: "search",
    tab: "Search",
    panel: ".search-view",
    caption: "Search",
    body: "Semantic query · results ranked by fusion",
  },
  {
    key: "functions",
    tab: "Functions",
    panel: ".functions-view, .functions-table",
    caption: "Functions",
    body: "PageRank · betweenness · blast hotspots",
  },
  {
    key: "cfg",
    tab: "CFG / PDG Analysis",
    panel: ".cfg-detail, .cfg-graph-panel",
    caption: "CFG",
    body: "Control-flow blocks & edges",
  },
  {
    key: "dataflow",
    tab: "Dataflow",
    panel: ".dataflow-view",
    caption: "Dataflow",
    body: "CPG field mutations · PDG · dominator tree",
  },
  {
    key: "slice",
    tab: "Program Slicing",
    panel: ".slice-view",
    caption: "Slice",
    body: "Backward slice · criterion & highlights",
  },
  {
    key: "blast",
    tab: "Blast Radius",
    panel: ".blast-view",
    caption: "Blast radius",
    body: "Upstream impact score & callers",
  },
  {
    key: "taint",
    tab: "Taint Analysis",
    panel: ".taint-view",
    caption: "Taint",
    body: "Source → sink flows",
  },
  {
    key: "migration",
    tab: "Migration",
    panel: ".migration-view, .migration-tuning",
    caption: "Migration",
    body: "Package roadmap · presets & ordering",
  },
  {
    key: "guide",
    tab: "Query Guide",
    panel: ".guide-view",
    caption: "Query Guide",
    body: "CLI cookbook · check & export recipes",
  },
];

fs.mkdirSync(OUT_DIR, { recursive: true });

function sleep(ms) {
  return new Promise((r) => setTimeout(r, ms));
}

function resolveFfmpeg() {
  const full = "/opt/homebrew/opt/ffmpeg-full/bin/ffmpeg";
  if (fs.existsSync(full)) return full;
  return "ffmpeg";
}

function formatSrtTime(sec) {
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = Math.floor(sec % 60);
  const ms = Math.min(999, Math.round((sec - Math.floor(sec)) * 1000));
  return `${String(h).padStart(2, "0")}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")},${String(ms).padStart(3, "0")}`;
}

/** @param {{ caption: string, body: string, startSec: number, endSec: number }[]} cues */
function writeSrt(outPath, cues) {
  const lines = [];
  cues.forEach((cue, i) => {
    lines.push(String(i + 1));
    lines.push(`${formatSrtTime(cue.startSec)} --> ${formatSrtTime(cue.endSec)}`);
    lines.push(cue.caption);
    lines.push(cue.body);
    lines.push("");
  });
  fs.writeFileSync(outPath, lines.join("\n"));
}

async function clickTab(page, label) {
  const tab = page.locator(".rb-main-tabs").getByRole("button", { name: label, exact: true });
  await tab.scrollIntoViewIfNeeded();
  await tab.click();
  await sleep(400);
}

async function selectFunction(page, name) {
  await page
    .locator(".function-list-item")
    .first()
    .waitFor({ state: "visible", timeout: 90000 })
    .catch(() => {});
  const search = page.locator('.function-list-sidebar input[type="search"]');
  if (await search.count()) {
    await search.fill("");
    await sleep(100);
    await search.fill(name);
    await sleep(600);
  }
  const item = page.locator(".function-list-item", {
    has: page.locator(".function-list-item-name", { hasText: name }),
  });
  if ((await item.count()) > 0) {
    await item.first().click();
    await sleep(500);
    return;
  }
  // Clear filter so we can pick a loaded row (large repos may omit rare names).
  if (await search.count()) {
    await search.fill("");
    await sleep(400);
  }
  const fallback = page.locator(".function-list-item").first();
  if (await fallback.count()) {
    await fallback.click();
    await sleep(800);
  }
}

async function waitForBlastResults(page) {
  await page.locator(".blast-view").waitFor({ state: "visible", timeout: 15000 });
  await page.waitForFunction(
    () => {
      const view = document.querySelector(".blast-view");
      if (!view) return false;
      const text = view.textContent ?? "";
      if (text.includes("Callers of")) return true;
      const score = view.querySelector(".fs-4.fw-semibold.text-primary");
      return !!(score && score.textContent && score.textContent.trim().length > 0);
    },
    { timeout: 90000 },
  );
  await sleep(350);
}

async function waitWasm(page) {
  await page.waitForSelector(".rb-app", { timeout: 60000 });
  await page.waitForFunction(
    () => {
      const msg = document.body.textContent ?? "";
      if (msg.includes("WASM engine required for blast-radius")) return false;
      if (msg.includes("Waiting for WASM engine")) return false;
      return true;
    },
    { timeout: 90000 },
  );
  await sleep(1000);
}

async function clearHighlights(page) {
  await page.evaluate(() => {
    document.querySelectorAll("[data-rb-demo-highlight]").forEach((el) => {
      el.style.outline = "";
      el.style.outlineOffset = "";
      el.style.boxShadow = "";
      el.removeAttribute("data-rb-demo-highlight");
    });
    document.getElementById("rb-demo-caption")?.remove();
  });
}

async function highlightTabAndPanel(page, tabLabel, panelSelector) {
  await page.evaluate(
    ({ tabLabel, panelSelector }) => {
      const styleHighlight = (el) => {
        el.setAttribute("data-rb-demo-highlight", "1");
        el.style.outline = "3px solid #0d6efd";
        el.style.outlineOffset = "3px";
        el.style.boxShadow = "0 0 0 6px rgba(13, 110, 253, 0.15)";
      };

      const tabBar = document.querySelector(".rb-main-tabs");
      if (tabBar) {
        styleHighlight(tabBar);
        tabBar.scrollIntoView({ block: "nearest", behavior: "instant" });
      }

      if (tabLabel) {
        for (const btn of document.querySelectorAll(".rb-main-tabs .nav-link")) {
          const label = btn.querySelector("span")?.textContent?.trim() ?? btn.textContent?.trim();
          if (label === tabLabel) styleHighlight(btn);
        }
      }

      const workspace = document.querySelector(".rb-tab-workspace");
      if (workspace) styleHighlight(workspace);

      const panelCard = document.querySelector(".rb-tab-panel-card");
      if (panelCard) styleHighlight(panelCard);

      for (const sel of panelSelector.split(",").map((s) => s.trim())) {
        const panel = document.querySelector(sel);
        if (panel) {
          panel.scrollIntoView({ block: "nearest", behavior: "instant" });
          styleHighlight(panel);
          break;
        }
      }
    },
    { tabLabel, panelSelector },
  );
}

/**
 * Dataflow tab tour: mutations → PDG → dominator, with short dwells so each
 * feature is visible inside the single tab segment.
 */
async function prepareDataflowTour(page) {
  const typeInput = page.getByTestId("mutations-type-input");
  await typeInput.waitFor({ state: "visible", timeout: 15000 });
  await typeInput.fill("");
  await typeInput.fill(MUTATIONS_TYPE);
  await sleep(300);
  const exclude = page.getByTestId("mutations-exclude-ctors");
  if ((await exclude.count()) && !(await exclude.isChecked())) {
    await exclude.check();
  }
  await page.getByTestId("mutations-table").waitFor({ state: "visible", timeout: 12000 });
  await page.getByTestId("mutations-row").first().click();
  await page
    .locator(".dataflow-graph-panel, .dataflow-source-panel")
    .first()
    .waitFor({ state: "visible", timeout: 20000 })
    .catch(() => {});
  await sleep(800);

  // Brief focus on mutations panel, then PDG graph, then dominator.
  await highlightTabAndPanel(page, "Dataflow", "[data-testid='mutations-panel'], .mutations-panel");
  await sleep(1800);
  await clearHighlights(page);

  const dfView = page.locator("#df-view");
  if (await dfView.count()) {
    await dfView.selectOption("dataflow");
    await sleep(400);
  }
  await highlightTabAndPanel(page, "Dataflow", ".dataflow-graph-panel");
  await sleep(1800);
  await clearHighlights(page);

  if (await dfView.count()) {
    await dfView.selectOption("dominator");
    await sleep(700);
  }
  await highlightTabAndPanel(page, "Dataflow", ".dataflow-graph-panel");
  await sleep(1400);
  await clearHighlights(page);

  // Leave on dataflow + mutations context for the final showcase hold.
  if (await dfView.count()) {
    await dfView.selectOption("dataflow");
    await sleep(400);
  }
}

async function prepareSegment(page, key) {
  try {
    switch (key) {
      case "functions": {
        const prBtn = page.getByRole("button", { name: /Sort by PR/i });
        if (await prBtn.count()) await prBtn.click();
        await sleep(300);
        break;
      }
      case "cfg": {
        await selectFunction(page, FN_CFG);
        const loadCfg = page.getByRole("button", { name: /Load CFG graph/i });
        if (await loadCfg.count()) await loadCfg.click();
        await page.locator(".cfg-detail").first().waitFor({ state: "visible", timeout: 25000 }).catch(() => {});
        await sleep(500);
        break;
      }
      case "dataflow": {
        await prepareDataflowTour(page);
        break;
      }
      case "slice": {
        await selectFunction(page, FN_SLICE);
        await page.locator("#slice-line").fill(String(SLICE_LINE));
        await page.locator("#slice-var").fill(SLICE_VAR);
        await page.getByRole("button", { name: "Compute slice" }).click();
        await page.getByText(/slice:/i).waitFor({ state: "visible", timeout: 15000 }).catch(() => {});
        await sleep(400);
        break;
      }
      case "blast": {
        await selectFunction(page, FN_BLAST);
        await waitForBlastResults(page);
        break;
      }
      case "taint": {
        await selectFunction(page, FN_TAINT);
        const row = page.locator(".taint-view table tbody tr").first();
        try {
          await row.waitFor({ state: "visible", timeout: 5000 });
          await row.click();
        } catch {
          // No taint rows — still show the empty tab.
        }
        await sleep(350);
        break;
      }
      case "migration": {
        await page.waitForSelector(".migration-tuning, .migration-view", { timeout: 20000 }).catch(() => {});
        await sleep(400);
        break;
      }
      case "search": {
        const input = page.locator('.search-view input[type="search"]');
        await input.waitFor({ state: "visible", timeout: 15000 });
        // Wait until /api/semantic/status enables the form (not the empty placeholder row).
        await page
          .locator('.search-view input[type="search"]:not([disabled])')
          .waitFor({ state: "visible", timeout: 60000 });
        await input.click();
        await input.fill("");
        await input.fill(SEMANTIC_QUERY);
        await sleep(300);
        await page.locator('.search-view button[type="submit"]').click();
        await page
          .locator(".search-results tbody tr td.fn-name")
          .first()
          .waitFor({ state: "visible", timeout: 60000 });
        // Hold a moment so the typed query + result rows are readable on camera.
        await sleep(1200);
        break;
      }
      case "guide": {
        const blastSection = page.locator(".guide-view section", { hasText: "Blast radius" });
        if (await blastSection.count()) {
          await blastSection.first().scrollIntoViewIfNeeded();
          await sleep(700);
        }
        const graphSection = page.locator(".guide-view section", { hasText: "Graph visualization" });
        if (await graphSection.count()) {
          await graphSection.first().scrollIntoViewIfNeeded();
          await sleep(700);
        }
        const dataflowSection = page.locator(".guide-view section", { hasText: "Dataflow" });
        if (await dataflowSection.count()) {
          await dataflowSection.first().scrollIntoViewIfNeeded();
          await sleep(500);
        }
        break;
      }
      default:
        break;
    }
  } catch (err) {
    console.warn(`prepareSegment(${key}) skipped:`, err.message ?? err);
  }
}

const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({
  viewport: { width: 1280, height: 720 },
  recordVideo: { dir: OUT_DIR, size: { width: 1280, height: 720 } },
});
const page = await context.newPage();
// Align wall clock with Playwright's video timeline (starts at context creation).
const recordingStartedAt = Date.now();

await page.goto(BASE, { waitUntil: "networkidle", timeout: 120000 });
await waitWasm(page);

/** @type {{ caption: string, body: string, startSec: number, endSec: number, key: string }[]} */
const cues = [];

for (const segment of TAB_SEGMENTS) {
  if (segment.tab) {
    await clickTab(page, segment.tab);
  }

  // Dataflow: caption covers the in-tab feature tour (mutations → PDG → dom).
  const captionFromPrep = segment.key === "dataflow";
  const startSec = captionFromPrep
    ? (Date.now() - recordingStartedAt) / 1000
    : null;

  await prepareSegment(page, segment.key);

  const holdStart = startSec ?? (Date.now() - recordingStartedAt) / 1000;
  await highlightTabAndPanel(page, segment.tab, segment.panel);
  await sleep(HOLD_MS);
  await clearHighlights(page);
  const endSec = (Date.now() - recordingStartedAt) / 1000;

  cues.push({
    key: segment.key,
    caption: segment.caption,
    body: segment.body,
    startSec: holdStart,
    endSec,
  });
}

await clearHighlights(page);
await sleep(400);

const video = page.video();
await context.close();
await browser.close();

if (!video) throw new Error("Playwright did not return a video handle");

const saved = await video.path();
fs.renameSync(saved, RAW_WEBM);

const ffmpegBin = resolveFfmpeg();
const probe = spawnSync(
  "ffprobe",
  ["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", RAW_WEBM],
  { encoding: "utf8" },
);
const rawDur = parseFloat(probe.stdout.trim() || "0");

// Align cue times to actual media duration (Playwright start lag).
const lastCueEnd = cues.length ? cues[cues.length - 1].endSec : rawDur;
const clockSpan = Math.max(lastCueEnd, 0.001);
const mediaScale = rawDur > 0 ? rawDur / clockSpan : 1;
const scaledCues = cues.map((c) => ({
  ...c,
  startSec: Math.max(0, c.startSec * mediaScale),
  endSec: Math.min(rawDur || c.endSec * mediaScale, c.endSec * mediaScale),
}));

let speedup = 1;
if (rawDur > 0 && MAX_SPEEDUP > 1) {
  // Only tiny speedup allowed — prefer longer accurate video over misaligned captions.
  const ideal = TAB_SEGMENTS.length * (HOLD_MS / 1000) * 1.35;
  if (rawDur > ideal * 1.4) {
    speedup = Math.min(MAX_SPEEDUP, rawDur / ideal);
  }
}

const timedCues = scaledCues.map((c) => ({
  caption: c.caption,
  body: c.body,
  startSec: c.startSec / speedup,
  endSec: c.endSec / speedup,
}));
writeSrt(OUT_SRT, timedCues);

let vf = "fps=30,scale=1280:720:flags=lanczos";
if (speedup > 1.001) {
  vf = `setpts=PTS/${speedup},fps=30,scale=1280:720:flags=lanczos`;
}

const ffArgs = [
  "-y",
  "-i",
  RAW_WEBM,
  "-vf",
  vf,
  "-c:v",
  "libx264",
  "-preset",
  "fast",
  "-crf",
  "22",
  "-pix_fmt",
  "yuv420p",
  "-movflags",
  "+faststart",
];
if (speedup > 1.001) {
  ffArgs.push("-t", String(rawDur / speedup));
}

const ff = spawnSync(ffmpegBin, [...ffArgs, OUT_NO_CAPTIONS], { encoding: "utf8" });
if (ff.status !== 0) {
  console.error(ff.stderr);
  throw new Error("ffmpeg encode failed");
}

const finalProbe = spawnSync(
  "ffprobe",
  ["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", OUT_NO_CAPTIONS],
  { encoding: "utf8" },
);

console.log(
  JSON.stringify(
    {
      dashboard: BASE,
      output_no_captions: OUT_NO_CAPTIONS,
      srt: OUT_SRT,
      raw_duration_s: rawDur,
      final_duration_s: parseFloat(finalProbe.stdout.trim() || "0"),
      hold_ms: HOLD_MS,
      speedup,
      tabs: TAB_SEGMENTS.map((s) => s.key),
      cue_windows_s: timedCues.map((c) => ({
        caption: c.caption,
        start: Number(c.startSec.toFixed(2)),
        end: Number(c.endSec.toFixed(2)),
      })),
      next: "./docs/videos/burn-feature-demo-captions.sh",
    },
    null,
    2,
  ),
);
