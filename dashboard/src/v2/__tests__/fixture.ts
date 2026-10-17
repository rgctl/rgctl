/**
 * A hand-built bundle, so the shell's tests are hermetic.
 *
 * Deliberately not the real 26 MB bundle: a test that depends on a repository being indexed somewhere is a
 * test that passes on one machine. The shapes here are copied from a real bundle (`sources_index.json`
 * keys its array `files`; the manifest nests the digest under `graph`) because those details are exactly
 * what a guess gets wrong.
 */
import { vi } from "vitest";

export const SUBJECT = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
export const OTHER = "11111111-2222-4333-8444-555555555555";
export const SOURCE_PATH = "/repo/src/auth.ts";
const SOURCE_ID = "src1";

export const SOURCE_LINES = [
  "export function handleAuth(req: Request) {",
  "  const token = req.headers.get('authorization');",
  "  if (!token) {",
  "    return new Response('unauthorized', { status: 401 });",
  "  }",
  "  return verify(token);",
  "}",
];

const FUNCTIONS = [
  {
    function_id: SUBJECT,
    name: "handleAuth",
    file_path: SOURCE_PATH,
    block_count: 5,
    cfg_edge_count: 6,
    source_lines: SOURCE_LINES.length,
    pdg_nodes: 4,
  },
  {
    function_id: OTHER,
    name: "verify",
    file_path: SOURCE_PATH,
    block_count: 2,
    cfg_edge_count: 1,
    source_lines: SOURCE_LINES.length,
    pdg_nodes: 1,
  },
];

const CFG = {
  [SUBJECT]: {
    function_id: SUBJECT,
    name: "handleAuth",
    file_path: SOURCE_PATH,
    entry: 0,
    exits: [3],
    blocks: [
      { id: 0, label: "B0", start_line: 1, end_line: 2, statements: ["const token = …"], stmts_total: 1 },
      { id: 1, label: "B1", start_line: 3, end_line: 4, statements: ["if (!token)"], stmts_total: 1 },
      // A synthetic block: no source line. `start_line: 0` means the absence of a line, not line zero.
      { id: 2, label: "B2", start_line: 0, end_line: 0, statements: [], stmts_total: 0 },
      { id: 3, label: "B3", start_line: 6, end_line: 7, statements: ["return verify(token)"], stmts_total: 1 },
    ],
    // Field names taken from a real `cfg/<id>.json`, not guessed. The first version of this fixture omitted
    // `edges` entirely and named the frontier `df` — the export calls it `dominance_frontiers` — which made
    // the graph throw and unmount the whole tree, taking three unrelated tests with it.
    edges: [
      { from: 0, to: 1, edge_type: "next" },
      { from: 1, to: 2, edge_type: "if_true" },
      { from: 1, to: 3, edge_type: "if_false" },
      { from: 3, to: 2, edge_type: "return" },
    ],
    // Indexed by block id, which is what the layout walks.
    idom: [0, 0, 1, 1],
    dominance_frontiers: [[], [2], [], []],
  },
};

const BODIES: Record<string, unknown> = {
  "manifest.json": {
    schema_version: 1,
    dashboard_version: "0.4.19",
    generated_at: String(Math.floor(Date.now() / 1000) - 3600),
    graph: { node_count: 42, edge_count: 7, digest: "deadbeefdeadbeef" },
    analysis: { cfg_available: true, slice_available: true, blast_available: true, taint_available: false },
    semantic: { available: true },
  },
  "cfg_index.json": { schema_version: 1, available: true, function_count: FUNCTIONS.length, functions: FUNCTIONS },
  "slice_index.json": { schema_version: 1, available: true, function_count: FUNCTIONS.length, functions: FUNCTIONS },
  "sources_index.json": { schema_version: 1, files: [{ source_id: SOURCE_ID, file_path: SOURCE_PATH, total_lines: SOURCE_LINES.length }] },
  [`cfg/${SUBJECT}.json`]: CFG[SUBJECT],
};

function json(body: unknown, status = 200) {
  return {
    ok: status >= 200 && status < 300,
    status,
    json: async () => body,
    text: async () => JSON.stringify(body),
  } as unknown as Response;
}

function text(body: string, status = 200) {
  return { ok: true, status, text: async () => body, json: async () => JSON.parse(body) } as unknown as Response;
}

/** Serve the fixture bundle. Anything unknown is a 404, which the loaders treat as a typed absence. */
export function installFetchStub() {
  const stub = vi.fn(async (input: RequestInfo | URL) => {
    const raw = typeof input === "string" ? input : input instanceof URL ? input.pathname : String(input);
    const path = raw.startsWith("file:") ? new URL(raw).pathname : raw;
    for (const [name, body] of Object.entries(BODIES)) {
      if (path.endsWith(name)) return json(body);
    }
    if (path.endsWith(`sources/${SOURCE_ID}.txt`)) return text(SOURCE_LINES.join("\n"));
    return json({ error: "not found" }, 404);
  });
  vi.stubGlobal("fetch", stub);
  return stub;
}
