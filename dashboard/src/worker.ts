/// <reference lib="webworker" />

import init, { EngineContext } from "../wasm/rbuilder_wasm.js";
import { bundleDataUrl } from "./bundleUrl";
import { computeSlice } from "./sliceEngine";
import { computeDataflowGraph } from "./dataflowEngine";
import type {
  NodeListPayload,
  SliceBundlePayload,
  SliceResultPayload,
  SubgraphPayload,
  WorkerIn,
  WorkerOut,
} from "./types";

let engine: EngineContext | null = null;
const sliceBundleCache = new Map<string, SliceBundlePayload>();

self.onmessage = async (ev: MessageEvent<WorkerIn>) => {
  const msg = ev.data;
  try {
    switch (msg.type) {
      case "init":
        await handleInit();
        break;
      case "expand":
        await handleExpand(msg.requestId, msg.indices, msg.typeMask);
        break;
      case "list_nodes":
        await handleListNodes(msg.requestId, msg.typeMask, msg.offset, msg.limit);
        break;
      case "compute_slice":
        await handleComputeSlice(
          msg.requestId,
          msg.functionId,
          msg.line,
          msg.variable,
          msg.direction,
        );
        break;
      case "blast_radius":
        await handleBlastRadius(msg.requestId, msg.nodeIndex, msg.maxDepth);
        break;
      case "compute_dataflow":
        await handleComputeDataflow(
          msg.requestId,
          msg.functionId,
          msg.variable,
          msg.includeControl,
        );
        break;
      default:
        break;
    }
  } catch (e) {
    const err: WorkerOut = {
      type: "error",
      requestId: "requestId" in msg ? msg.requestId : undefined,
      message: e instanceof Error ? e.message : String(e),
    };
    self.postMessage(err);
  }
};

async function handleInit() {
  const payloadRes = await fetch(bundleDataUrl("graph_payload.bin"));
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
    engine = new EngineContext(bytes);
    nodeCount = engine.node_count;
    edgeCount = engine.edge_count;
    schemaVersion = engine.schema_version;
    digest = engine.digest;
    wasm = true;
  } catch (wasmErr) {
    if (bytes.length < 136 || bytes[0] !== 0x52 || bytes[1] !== 0x42) {
      throw wasmErr;
    }
    schemaVersion = new DataView(bytes.buffer).getUint32(8, true);
    nodeCount = Number(new DataView(bytes.buffer).getBigUint64(12, true));
    edgeCount = Number(new DataView(bytes.buffer).getBigUint64(20, true));
    digest = new TextDecoder().decode(bytes.slice(28, 92)).replace(/\0+$/, "");
  }

  const out: WorkerOut = {
    type: "ready",
    nodeCount,
    edgeCount,
    schemaVersion,
    digest,
    wasm,
  };
  self.postMessage(out);
}

async function handleExpand(requestId: number, indices: number[], typeMask: number) {
  if (!engine) {
    throw new Error("WASM engine not loaded — expand requires wasm");
  }
  const json = engine.expandIndices(new Uint32Array(indices), typeMask >>> 0);
  const payload = JSON.parse(json) as SubgraphPayload;
  const out: WorkerOut = { type: "subgraph", requestId, payload };
  self.postMessage(out);
}

async function handleListNodes(
  requestId: number,
  typeMask: number,
  offset: number,
  limit: number,
) {
  if (!engine) {
    throw new Error("WASM engine not loaded — list_nodes requires wasm");
  }
  const json = engine.listNodes(typeMask >>> 0, offset >>> 0, limit >>> 0);
  const payload = JSON.parse(json) as NodeListPayload;
  const out: WorkerOut = { type: "node_list", requestId, payload };
  self.postMessage(out);
}

async function loadSliceBundle(functionId: string): Promise<SliceBundlePayload> {
  const cached = sliceBundleCache.get(functionId);
  if (cached) return cached;
  const res = await fetch(bundleDataUrl(`slice/${functionId}.json`));
  if (!res.ok) {
    throw new Error(`slice/${functionId}.json: HTTP ${res.status}`);
  }
  const bundle = (await res.json()) as SliceBundlePayload;
  sliceBundleCache.set(functionId, bundle);
  return bundle;
}

async function handleComputeSlice(
  requestId: number,
  functionId: string,
  line: number,
  variable: string,
  direction: "backward" | "forward",
) {
  const bundle = await loadSliceBundle(functionId);
  const payload = computeSlice(
    bundle.pdg.nodes,
    bundle.pdg.edges,
    line,
    variable,
    direction,
    bundle.total_lines,
  );
  const out: WorkerOut = { type: "slice_result", requestId, payload };
  self.postMessage(out);
}

async function handleBlastRadius(requestId: number, nodeIndex: number, maxDepth: number) {
  if (!engine) {
    throw new Error("WASM engine not loaded — blast_radius requires wasm");
  }
  const json = engine.blastRadius(nodeIndex >>> 0, maxDepth >>> 0);
  const payload = JSON.parse(json) as import("./types").BlastRadiusPayload;
  const out: WorkerOut = { type: "blast_result", requestId, payload };
  self.postMessage(out);
}

async function handleComputeDataflow(
  requestId: number,
  functionId: string,
  variable: string | null,
  includeControl: boolean,
) {
  const bundle = await loadSliceBundle(functionId);
  const payload = computeDataflowGraph(
    bundle.pdg.nodes,
    bundle.pdg.edges,
    null,
    { variable, includeControl },
  );
  const out: WorkerOut = { type: "dataflow_result", requestId, payload };
  self.postMessage(out);
}

export {};
