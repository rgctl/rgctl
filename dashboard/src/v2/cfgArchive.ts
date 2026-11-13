/**
 * CFG detail from the binary record pack (`archive_only` bundles).
 *
 * Large repos (e.g. metasfresh) skip per-file `cfg/<id>.json` and write
 * `cfg_pdg.record_{index,data}.bin` instead. v1 decoded records with WASM
 * `parseCfgDetail` — same path here, lazy so the universe shell does not pay for
 * WASM until a subject actually needs an archive CFG.
 */
import { fetchCfgRecordBytes, parseCfgRecordIndex } from "../cfgRecordIndex";
import { bundleUrl, type CfgDetail } from "./data";

export interface CfgIndexMeta {
  schema_version?: number;
  available?: boolean;
  detail_mode?: string;
  record_index_path?: string | null;
  record_data_path?: string | null;
  function_count?: number;
}

type ParseCfgDetail = (bytes: Uint8Array) => string;
type WasmInit = (module_or_path?: unknown) => Promise<unknown>;

let wasmReady: Promise<ParseCfgDetail> | null = null;
let recordIndex: Map<string, { offset: number; length: number }> | null = null;
let recordPaths: { indexPath: string; dataPath: string } | null = null;

/** Test seam — clear cached WASM / record index between cases. */
export function resetCfgArchiveCaches(): void {
  wasmReady = null;
  recordIndex = null;
  recordPaths = null;
  resetSliceArchiveCaches();
}

/** Always re-read the small index JSON so stubs / re-exports stay honest. */
export async function loadCfgIndexMeta(
  fetchImpl: typeof fetch = fetch,
): Promise<CfgIndexMeta | null> {
  try {
    const res = await fetchImpl(bundleUrl("cfg_index.json"), { cache: "no-store" });
    if (!res.ok) return null;
    return (await res.json()) as CfgIndexMeta;
  } catch {
    return null;
  }
}

export function isArchiveOnly(meta: CfgIndexMeta | null): boolean {
  return (
    meta?.detail_mode === "archive_only" &&
    Boolean(meta.record_index_path) &&
    Boolean(meta.record_data_path)
  );
}

async function ensureParseCfgDetail(): Promise<ParseCfgDetail> {
  if (!wasmReady) {
    wasmReady = (async () => {
      const mod = (await import("../../wasm/rgctl_wasm.js")) as {
        default: WasmInit;
        parseCfgDetail: ParseCfgDetail;
      };
      await mod.default();
      return mod.parseCfgDetail;
    })();
  }
  return wasmReady;
}

async function ensureRecordIndex(
  indexPath: string,
  dataPath: string,
  fetchImpl: typeof fetch = fetch,
): Promise<Map<string, { offset: number; length: number }>> {
  if (recordIndex && recordPaths?.indexPath === indexPath) {
    return recordIndex;
  }
  const res = await fetchImpl(bundleUrl(indexPath), { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`${indexPath}: HTTP ${res.status}`);
  }
  recordIndex = parseCfgRecordIndex(new Uint8Array(await res.arrayBuffer()));
  recordPaths = { indexPath, dataPath };
  return recordIndex;
}

/**
 * Decode one function's CFG from the record pack via WASM.
 * Returns `null` when the function is not in the pack.
 */
export async function loadCfgDetailFromArchive(
  functionId: string,
  meta?: CfgIndexMeta | null,
  fetchImpl: typeof fetch = fetch,
): Promise<CfgDetail | null> {
  const index = meta ?? (await loadCfgIndexMeta(fetchImpl));
  const indexPath = index?.record_index_path;
  const dataPath = index?.record_data_path;
  if (!indexPath || !dataPath) {
    throw new Error("CFG record pack not available in this bundle");
  }

  const [parseCfgDetail, locations] = await Promise.all([
    ensureParseCfgDetail(),
    ensureRecordIndex(indexPath, dataPath, fetchImpl),
  ]);

  const location = locations.get(functionId);
  if (!location) {
    return null;
  }

  const bytes = await fetchCfgRecordBytes(bundleUrl(dataPath).toString(), location, fetchImpl);
  const json = parseCfgDetail(bytes);
  const raw = JSON.parse(json) as CfgDetail & { schema_version?: number };
  return {
    function_id: raw.function_id,
    name: raw.name,
    file_path: raw.file_path,
    entry: raw.entry,
    exits: raw.exits ?? [],
    blocks: raw.blocks ?? [],
    edges: raw.edges,
    idom: raw.idom,
    dominance_frontiers: raw.dominance_frontiers,
  };
}

export interface SliceIndexMeta {
  schema_version?: number;
  available?: boolean;
  detail_mode?: string;
  record_index_path?: string | null;
  record_data_path?: string | null;
  function_count?: number;
}

let sliceIndexMetaCache: SliceIndexMeta | null | undefined = undefined;
let sliceRecordIndex: Map<string, { offset: number; length: number }> | null = null;
let sliceRecordPaths: { indexPath: string; dataPath: string } | null = null;

/** Test seam — clear slice record-pack caches between cases. */
export function resetSliceArchiveCaches(): void {
  sliceIndexMetaCache = undefined;
  sliceRecordIndex = null;
  sliceRecordPaths = null;
}

export async function loadSliceIndexMeta(
  fetchImpl: typeof fetch = fetch,
): Promise<SliceIndexMeta | null> {
  if (sliceIndexMetaCache !== undefined) {
    return sliceIndexMetaCache;
  }
  try {
    const res = await fetchImpl(bundleUrl("slice_index.json"), { cache: "no-store" });
    if (!res.ok) {
      sliceIndexMetaCache = null;
      return null;
    }
    const raw = (await res.json()) as SliceIndexMeta;
    // Retain only lightweight header fields, not the `functions` array (~23 MB on large repos).
    sliceIndexMetaCache = {
      schema_version: raw.schema_version,
      available: raw.available,
      detail_mode: raw.detail_mode,
      record_index_path: raw.record_index_path,
      record_data_path: raw.record_data_path,
      function_count: raw.function_count,
    };
    return sliceIndexMetaCache;
  } catch {
    return null;
  }
}

export function isSliceArchiveOnly(meta: SliceIndexMeta | null): boolean {
  return (
    meta?.detail_mode === "archive_only" &&
    Boolean(meta.record_index_path) &&
    Boolean(meta.record_data_path)
  );
}

async function ensureSliceRecordIndex(
  indexPath: string,
  dataPath: string,
  fetchImpl: typeof fetch = fetch,
): Promise<Map<string, { offset: number; length: number }>> {
  if (sliceRecordIndex && sliceRecordPaths?.indexPath === indexPath) {
    return sliceRecordIndex;
  }
  const res = await fetchImpl(bundleUrl(indexPath), { cache: "no-store" });
  if (!res.ok) {
    throw new Error(`${indexPath}: HTTP ${res.status}`);
  }
  sliceRecordIndex = parseCfgRecordIndex(new Uint8Array(await res.arrayBuffer()));
  sliceRecordPaths = { indexPath, dataPath };
  return sliceRecordIndex;
}

/**
 * Fetch one function's slice/PDG JSON bytes from `slice.record_*.bin` via HTTP Range.
 * Returns `null` when the function is not in the pack.
 */
export async function loadSliceJsonFromArchive(
  functionId: string,
  meta?: SliceIndexMeta | null,
  fetchImpl: typeof fetch = fetch,
): Promise<Uint8Array | null> {
  const index = meta ?? (await loadSliceIndexMeta(fetchImpl));
  const indexPath = index?.record_index_path;
  const dataPath = index?.record_data_path;
  if (!indexPath || !dataPath) {
    throw new Error("Slice record pack not available in this bundle");
  }

  const locations = await ensureSliceRecordIndex(indexPath, dataPath, fetchImpl);
  const location = locations.get(functionId);
  if (!location) {
    return null;
  }
  return fetchCfgRecordBytes(bundleUrl(dataPath).toString(), location, fetchImpl);
}
