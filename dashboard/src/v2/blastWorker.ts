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
  | { type: "error"; requestId?: number; message: string };

function bundleDataUrl(filename: string): URL {
  return new URL(filename, new URL("../", import.meta.url));
}

let engine: EngineContext | null = null;
let initPromise: Promise<void> | null = null;

async function ensureEngine(): Promise<EngineContext> {
  if (engine) return engine;
  if (!initPromise) {
    initPromise = (async () => {
      await init();
      const res = await fetch(bundleDataUrl("graph_payload.bin"));
      if (!res.ok) {
        throw new Error(`graph_payload.bin: HTTP ${res.status}`);
      }
      const bytes = new Uint8Array(await res.arrayBuffer());
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
