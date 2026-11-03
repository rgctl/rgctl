/**
 * Fetch + classify taint_index.json and per-function flow bundles.
 */
import { bundleUrl } from "../data";

export interface TaintFunctionEntry {
  function_id: string;
  name: string;
  file_path?: string | null;
  flow_count: number;
  vulnerable_count: number;
}

export interface TaintIndexPayload {
  schema_version: number;
  available: boolean;
  detail_dir: string;
  function_count: number;
  total_flows: number;
  vulnerable_flows: number;
  functions: TaintFunctionEntry[];
}

export interface TaintFlowView {
  id: number;
  variable: string;
  source_type: string;
  sink_type: string;
  severity: number;
  vulnerable: boolean;
  sanitizers: string[];
  source_line: number;
  sink_line: number;
  source_text: string;
  sink_text: string;
  path_lines: number[];
  path_statements: string[];
}

export interface TaintBundlePayload {
  schema_version: number;
  function_id: string;
  name: string;
  file_path?: string | null;
  flows: TaintFlowView[];
}

export type TaintIndexLoad =
  | { kind: "missing" }
  | { kind: "unavailable"; index: TaintIndexPayload }
  | { kind: "empty"; index: TaintIndexPayload }
  | { kind: "ready"; index: TaintIndexPayload };

export async function loadTaintIndex(fetchImpl: typeof fetch = fetch): Promise<TaintIndexLoad> {
  try {
    const res = await fetchImpl(bundleUrl("taint_index.json"), { cache: "no-store" });
    if (!res.ok) return { kind: "missing" };
    const raw: unknown = await res.json();
    return classifyTaintIndex(raw);
  } catch {
    return { kind: "missing" };
  }
}

export function classifyTaintIndex(raw: unknown): TaintIndexLoad {
  const index = parseTaintIndex(raw);
  if (!index) return { kind: "missing" };
  if (!index.available) return { kind: "unavailable", index };
  if (index.functions.length === 0) return { kind: "empty", index };
  return { kind: "ready", index };
}

export function parseTaintIndex(raw: unknown): TaintIndexPayload | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  if (typeof o.schema_version !== "number") return null;
  if (typeof o.available !== "boolean") return null;
  if (!Array.isArray(o.functions)) return null;
  return raw as TaintIndexPayload;
}

export async function loadTaintBundle(
  detailDir: string,
  functionId: string,
  fetchImpl: typeof fetch = fetch,
): Promise<TaintBundlePayload | null> {
  try {
    const res = await fetchImpl(bundleUrl(`${detailDir}/${functionId}.json`), { cache: "no-store" });
    if (!res.ok) return null;
    const raw: unknown = await res.json();
    if (!raw || typeof raw !== "object") return null;
    const o = raw as Record<string, unknown>;
    if (!Array.isArray(o.flows)) return null;
    return raw as TaintBundlePayload;
  } catch {
    return null;
  }
}

export interface DataflowIndexEntry {
  function_id: string;
  name: string;
  pdg_nodes?: number;
  data_edges?: number;
  block_count?: number;
}

export interface DataflowIndexPayload {
  schema_version: number;
  available: boolean;
  detail_dir?: string;
  function_count: number;
  functions: DataflowIndexEntry[];
}

export async function loadDataflowIndex(
  fetchImpl: typeof fetch = fetch,
): Promise<DataflowIndexPayload | null> {
  try {
    const res = await fetchImpl(bundleUrl("dataflow_index.json"), { cache: "no-store" });
    if (!res.ok) return null;
    return (await res.json()) as DataflowIndexPayload;
  } catch {
    return null;
  }
}

export interface BlastFunctionScore {
  index: number;
  score: number;
  direct: number;
  zone: number;
}

export interface BlastIndexPayload {
  schema_version: number;
  available: boolean;
  functions?: BlastFunctionScore[];
}

export async function loadBlastIndex(
  fetchImpl: typeof fetch = fetch,
): Promise<BlastIndexPayload | null> {
  try {
    const res = await fetchImpl(bundleUrl("blast_index.json"), { cache: "no-store" });
    if (!res.ok) return null;
    return (await res.json()) as BlastIndexPayload;
  } catch {
    return null;
  }
}

/**
 * Blast scores are keyed by columnar `index`, not `function_id`.
 * Without a join map we cannot attribute a score — typed absence, not zero.
 */
export function blastForFunction(
  blast: BlastIndexPayload | null,
  _functionId: string,
  columnarIndex: number | null,
): { kind: "ready"; score: BlastFunctionScore } | { kind: "absent"; reason: string } {
  if (!blast || !blast.available) {
    return { kind: "absent", reason: "Blast was not exported in this bundle (`blast_index.json` missing or unavailable)." };
  }
  if (columnarIndex === null || columnarIndex === undefined) {
    return {
      kind: "absent",
      reason:
        "Blast scores are keyed by columnar node index, not `function_id`, and no join is available for this subject — showing a zero would be fabricated.",
    };
  }
  const hit = (blast.functions ?? []).find((f) => f.index === columnarIndex);
  if (!hit) {
    return {
      kind: "absent",
      reason: "No blast score row matches this function's columnar index in `blast_index.json`.",
    };
  }
  return { kind: "ready", score: hit };
}
