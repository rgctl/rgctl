/**
 * Read the dashboard bundle.
 *
 * Everything here comes from `.rgctl/dashboard/`, written by `rgctl discover --with-dashboard` and served
 * next to the bundle itself — the same directory v1 reads, so the shell needs no new export.
 *
 * Shapes here were read off a real bundle (5,345 nodes, 1,479 functions), not assumed: the first draft of
 * this file expected `manifest.digest` and `sources_index.sources`, and the real manifest nests the digest
 * under `graph` and the source index calls its array `files`. Both would have rendered an empty shell
 * against real data while passing every type check.
 *
 * On absence: a missing sidecar is a typed absence the UI must state, not an empty array it renders as a
 * clean result. `available: false` in an index means the same thing as a 404.
 */

/** Asset-relative URL. Bundled assets live in `assets/`, data one level up, so a bare relative fetch
 *  from a worker resolves against the wrong directory. */
export function bundleUrl(name: string): URL {
  if (typeof import.meta !== "undefined" && import.meta.url) {
    return new URL(name, new URL("../", import.meta.url));
  }
  return new URL(name, window.location.href);
}

async function getJson<T>(name: string): Promise<T | null> {
  const res = await fetch(bundleUrl(name));
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`${name}: HTTP ${res.status}`);
  return (await res.json()) as T;
}

export interface Manifest {
  schema_version?: number;
  dashboard_version?: string;
  export_fingerprint?: string;
  /** Unix seconds, as a **string** in the real bundle. */
  generated_at?: string;
  graph?: { node_count?: number; edge_count?: number; digest?: string; payload_format?: string };
  analysis?: {
    cfg_available?: boolean;
    cfg_function_count?: number;
    slice_available?: boolean;
    slice_function_count?: number;
    blast_available?: boolean;
    taint_available?: boolean;
    mutations_available?: boolean;
  };
  metrics?: { function_count?: number; class_count?: number; calls_count?: number };
  view?: { community_count?: number; metanode_count?: number };
  semantic?: { available?: boolean; functions_indexed?: number; model_id?: string };
  /** Pipeline phases; the only honest source of "coverage" in the bundle today. */
  phases?: Record<string, string>;
}

export interface FunctionEntry {
  function_id: string;
  name: string;
  file_path?: string | null;
  block_count?: number;
  cfg_edge_count?: number;
  source_lines?: number;
  pdg_nodes?: number;
  data_edges?: number;
}

interface IndexPayload {
  schema_version?: number;
  available?: boolean;
  function_count?: number;
  functions?: FunctionEntry[];
}

export interface SourceFileEntry {
  source_id: string;
  file_path: string;
  total_lines: number;
}

export interface CfgBlock {
  id: number;
  label: string;
  start_line: number;
  end_line: number;
  statements: string[];
  stmts_total?: number;
}

export interface CfgEdge {
  from: number;
  to: number;
  edge_type: string;
}

/**
 * Read off a real bundle, not assumed. The first draft of this interface guessed `df`; the export calls it
 * `dominance_frontiers`, and it also carries `edges`, which the guess omitted entirely.
 *
 * `idom` and `dominance_frontiers` are indexed by **block id**, and block ids are sequential from 0, so
 * `idom[b.id]` is the immediate dominator of that block. That contract is what the layout depends on.
 *
 * The *direction* of `dominance_frontiers` is the part that gets misread, so it is stated here rather than
 * inferred at each call site: `dominance_frontiers[i]` is **DF(i)** — the blocks where a φ for definitions
 * made in block `i` belongs — which mirrors `DominatorTree::frontier` in `rgctl-analysis`. A block is
 * therefore a φ placement site when it appears in *some* row, not when its own row is non-empty. Read the
 * other way round, the graph dots the branch instead of the merge.
 */
export interface CfgDetail {
  function_id: string;
  name: string;
  file_path?: string | null;
  entry: number;
  exits: number[];
  blocks: CfgBlock[];
  /**
   * Optional even though the current schema always writes it: a bundle produced by an older version would
   * otherwise crash the room, and the design's rule is that an absence is *stated*, not thrown. The graph
   * renders the blocks without edges and says so.
   */
  edges?: CfgEdge[];
  idom?: number[];
  dominance_frontiers?: number[][];
}

export async function loadManifest(): Promise<Manifest | null> {
  return getJson<Manifest>("manifest.json");
}

/**
 * The function index. `cfg_index.json` and `slice_index.json` are disjoint per field — the first has
 * block/edge counts, the second PDG counts and source lines — so they are merged on `function_id`
 * rather than one being chosen and the other's columns lost.
 */
export async function loadFunctions(): Promise<FunctionEntry[]> {
  const [cfg, slice] = await Promise.all([
    getJson<IndexPayload>("cfg_index.json"),
    getJson<IndexPayload>("slice_index.json"),
  ]);
  const merged = new Map<string, FunctionEntry>();
  for (const f of cfg?.functions ?? []) merged.set(f.function_id, { ...f });
  for (const s of slice?.functions ?? []) {
    merged.set(s.function_id, { ...(merged.get(s.function_id) ?? s), ...s });
  }
  return [...merged.values()].sort((a, b) => a.name.localeCompare(b.name));
}

export async function loadCfgDetail(functionId: string): Promise<CfgDetail | null> {
  return getJson<CfgDetail>(`cfg/${functionId}.json`);
}

let sourceIndex: SourceFileEntry[] | null = null;

export async function loadSourceIndex(): Promise<SourceFileEntry[]> {
  if (sourceIndex) return sourceIndex;
  // The real key is `files`.
  const payload = await getJson<{ files?: SourceFileEntry[] }>("sources_index.json");
  sourceIndex = payload?.files ?? [];
  return sourceIndex;
}

const sourceText = new Map<string, string>();

/** Source text for a repo path, or `null` when this bundle did not export it. */
export async function loadSourceForPath(filePath: string): Promise<string | null> {
  const index = await loadSourceIndex();
  const entry = index.find((e) => e.file_path === filePath);
  if (!entry) return null;
  const cached = sourceText.get(entry.source_id);
  if (cached !== undefined) return cached;
  const res = await fetch(bundleUrl(`sources/${entry.source_id}.txt`));
  if (!res.ok) return null;
  const text = await res.text();
  sourceText.set(entry.source_id, text);
  return text;
}

/**
 * The longest directory prefix shared by every source path, so the UI can show repo-relative paths.
 *
 * Bundle paths are absolute (`/Users/…/<checkout>/src/…`) and the manifest does not record the repo root. Showing
 * them verbatim would put a developer's home directory in the dashboard, which is the leak fixed as D5 in
 * the v1 defect work. Deriving the prefix from the data beats guessing at it.
 */
let repoPrefix: string | null = null;

export async function repoRoot(): Promise<string> {
  if (repoPrefix !== null) return repoPrefix;
  const index = await loadSourceIndex();
  if (index.length === 0) {
    repoPrefix = "";
    return repoPrefix;
  }
  const split = index.map((e) => e.file_path.split("/").slice(0, -1));
  let common = split[0];
  for (const parts of split.slice(1)) {
    let i = 0;
    while (i < common.length && i < parts.length && common[i] === parts[i]) i++;
    common = common.slice(0, i);
    if (common.length === 0) break;
  }
  repoPrefix = common.join("/") + (common.length ? "/" : "");
  return repoPrefix;
}

/** Repo-relative path, falling back to the raw path when no prefix is known yet. */
export function relative(filePath: string, prefix: string): string {
  return prefix && filePath.startsWith(prefix) ? filePath.slice(prefix.length) : filePath;
}

/** 1-based inclusive excerpt, keeping line numbers so the reader can cite a line. */
export function excerpt(
  source: string,
  startLine: number,
  endLine: number,
): { line: number; text: string }[] {
  const lines = source.split("\n");
  const from = Math.max(1, startLine);
  const to = Math.min(lines.length, endLine);
  const out: { line: number; text: string }[] = [];
  for (let n = from; n <= to; n++) out.push({ line: n, text: lines[n - 1] ?? "" });
  return out;
}

/** Capabilities this bundle can actually evidence — never a plausible-looking constant. */
export function capabilities(m: Manifest | null): { label: string; available: boolean }[] {
  const a = m?.analysis;
  return [
    { label: "CFG / PDG", available: a?.cfg_available === true },
    { label: "slices", available: a?.slice_available === true },
    { label: "blast radius", available: a?.blast_available === true },
    { label: "taint", available: a?.taint_available === true },
    { label: "field writes", available: a?.mutations_available === true },
    { label: "semantic search", available: m?.semantic?.available === true },
  ];
}
