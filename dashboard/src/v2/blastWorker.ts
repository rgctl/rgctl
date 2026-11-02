/// <reference lib="webworker" />
/**
 * Lazy blast engine for the universe Taint room.
 *
 * Loads `graph_payload.bin` into WASM `EngineContext` once, then answers
 * `blastRadiusByUuid` / `indexOfUuid` so the UI never blocks on a 100MB+ parse.
 */
import init, { EngineContext } from "../../wasm/rgctl_wasm.js";

export type BlastWorkerIn =
  | { type: "init"; requestId: number }
  /** Expand a package's member indices into a subgraph: nodes plus the edges among them. */
  | { type: "expand"; requestId: number; indices: Uint32Array; typeMask: number }
  | { type: "blast_by_uuid"; requestId: number; uuid: string; maxDepth: number }
  | { type: "index_of_uuid"; requestId: number; uuid: string };

export type BlastWorkerOut =
  | { type: "ready"; requestId: number; nodeCount: number; edgeCount: number; digest: string }
  | {
      type: "blast_result";
      requestId: number;
      payload: {
        seed_index: number;
        seed_name: string;
        depth_limit: number;
        direct_caller_count: number;
        impact_zone_count: number;
        score: number;
        callers: Array<{
          index: number;
          name: string;
          depth: number;
          node_type: number;
          node_type_name: string;
        }>;
      };
    }
  | { type: "index_result"; requestId: number; index: number | null }
  | {
      type: "expand_result";
      requestId: number;
      payload: {
        nodes: Array<{
          index: number;
          name: string;
          node_type: number;
          node_type_name: string;
          complexity: number;
          file_path?: string | null;
        }>;
        edges: Array<{ source: number; target: number; edge_type: number }>;
      };
    }
  /**
   * Real progress for the one step that can report it: the payload download.
   *
   * `total` is the server's `Content-Length`, or `0` when the response did not declare one — the UI shows a
   * percentage only when it has a denominator, and an indeterminate state otherwise. The WASM parse after the
   * download has no progress hook, so it is reported as a phase, not as a fake fraction.
   */
  | { type: "progress"; phase: "engine" | "parsing"; loaded: number; total: number }
  | { type: "error"; requestId?: number; message: string };

function bundleDataUrl(filename: string): URL {
  return new URL(filename, new URL("../", import.meta.url));
}

let engine: EngineContext | null = null;
let initPromise: Promise<void> | null = null;

function emitProgress(phase: "engine" | "parsing", loaded: number, total: number): void {
  const out: BlastWorkerOut = { type: "progress", phase, loaded, total };
  self.postMessage(out);
}

/** Read the response body as a stream, reporting bytes as they arrive. */
async function downloadWithProgress(url: URL): Promise<Uint8Array> {
  const res = await fetch(url);
  if (!res.ok) throw new Error(`graph_payload.bin: HTTP ${res.status}`);
  const declared = Number(res.headers.get("content-length") ?? 0);
  const total = Number.isFinite(declared) && declared > 0 ? declared : 0;

  if (!res.body) {
    // No stream (older browser or a stubbed response): one honest jump, no invented percentage.
    const bytes = new Uint8Array(await res.arrayBuffer());
    emitProgress("engine", bytes.byteLength, total || bytes.byteLength);
    return bytes;
  }

  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let loaded = 0;
  let lastReported = 0;
  for (;;) {
    const { done, value } = await reader.read();
    if (done) break;
    if (value) {
      chunks.push(value);
      loaded += value.byteLength;
      // ~1 MiB between posts: enough to look live, few enough not to flood the main thread.
      if (loaded - lastReported >= 1024 * 1024) {
        lastReported = loaded;
        emitProgress("engine", loaded, total);
      }
    }
  }
  emitProgress("engine", loaded, total || loaded);
  const bytes = new Uint8Array(loaded);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  return bytes;
}

async function ensureEngine(): Promise<EngineContext> {
  if (engine) return engine;
  if (!initPromise) {
    initPromise = (async () => {
      await init();
      const bytes = await downloadWithProgress(bundleDataUrl("graph_payload.bin"));
      // No progress hook exists inside the WASM parse; say which phase this is rather than faking a number.
      emitProgress("parsing", bytes.byteLength, bytes.byteLength);
      engine = new EngineContext(bytes);
    })();
  }
  await initPromise;
  if (!engine) throw new Error("blast engine failed to initialize");
  return engine;
}

self.onmessage = async (ev: MessageEvent<BlastWorkerIn>) => {
  const msg = ev.data;
  try {
    switch (msg.type) {
      case "init": {
        const eng = await ensureEngine();
        const out: BlastWorkerOut = {
          type: "ready",
          requestId: msg.requestId,
          nodeCount: eng.node_count,
          edgeCount: eng.edge_count,
          digest: eng.digest,
        };
        self.postMessage(out);
        break;
      }
      case "blast_by_uuid": {
        const eng = await ensureEngine();
        const json = eng.blastRadiusByUuid(msg.uuid, msg.maxDepth >>> 0);
        const payload = JSON.parse(json) as BlastWorkerOut extends { type: "blast_result"; payload: infer P }
          ? P
          : never;
        const out: BlastWorkerOut = { type: "blast_result", requestId: msg.requestId, payload };
        self.postMessage(out);
        break;
      }
      case "expand": {
        const eng = await ensureEngine();
        const json = eng.expandIndices(msg.indices, msg.typeMask >>> 0);
        const payload = JSON.parse(json) as {
          nodes: Array<{
            index: number;
            name: string;
            node_type: number;
            node_type_name: string;
            complexity: number;
            file_path?: string | null;
          }>;
          edges: Array<{ source: number; target: number; edge_type: number }>;
        };
        const out: BlastWorkerOut = { type: "expand_result", requestId: msg.requestId, payload };
        self.postMessage(out);
        break;
      }
      case "index_of_uuid": {
        const eng = await ensureEngine();
        const index = eng.indexOfUuid(msg.uuid) ?? null;
        const out: BlastWorkerOut = { type: "index_result", requestId: msg.requestId, index };
        self.postMessage(out);
        break;
      }
      default:
        break;
    }
  } catch (e) {
    const out: BlastWorkerOut = {
      type: "error",
      requestId: "requestId" in msg ? msg.requestId : undefined,
      message: e instanceof Error ? e.message : String(e),
    };
    self.postMessage(out);
  }
};
