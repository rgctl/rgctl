/// <reference lib="webworker" />

import init, { EngineContext } from "../wasm/rbuilder_wasm.js";

export type WorkerOut =
  | {
      type: "ready";
      nodeCount: number;
      edgeCount: number;
      schemaVersion: number;
      digest: string;
      wasm: boolean;
    }
  | { type: "error"; message: string };

self.onmessage = async (ev: MessageEvent<{ type: string }>) => {
  if (ev.data?.type !== "init") return;

  try {
    const payloadRes = await fetch("./graph_payload.bin");
    if (!payloadRes.ok) {
      throw new Error(`graph_payload.bin: HTTP ${payloadRes.status}`);
    }
    const bytes = new Uint8Array(await payloadRes.arrayBuffer());

    let wasm = false;
    let nodeCount = 0;
    let edgeCount = 0;
    let schemaVersion = 0;
    let digest = "";

    try {
      await init();
      const engine = new EngineContext(bytes);
      nodeCount = engine.node_count;
      edgeCount = engine.edge_count;
      schemaVersion = engine.schema_version;
      digest = engine.digest;
      wasm = true;
    } catch (wasmErr) {
      // Fallback: parse header in JS (same layout as columnar v2) when WASM missing.
      if (bytes.length < 136 || bytes[0] !== 0x52 || bytes[1] !== 0x42) {
        throw wasmErr;
      }
      schemaVersion = new DataView(bytes.buffer).getUint32(8, true);
      nodeCount = Number(new DataView(bytes.buffer).getBigUint64(12, true));
      edgeCount = Number(new DataView(bytes.buffer).getBigUint64(20, true));
      digest = new TextDecoder().decode(bytes.slice(28, 92)).replace(/\0+$/, "");
    }

    const msg: WorkerOut = {
      type: "ready",
      nodeCount,
      edgeCount,
      schemaVersion,
      digest,
      wasm,
    };
    self.postMessage(msg);
  } catch (e) {
    const msg: WorkerOut = {
      type: "error",
      message: e instanceof Error ? e.message : String(e),
    };
    self.postMessage(msg);
  }
};

export {};
