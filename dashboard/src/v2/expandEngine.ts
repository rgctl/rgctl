/**
 * The graph engine, asked one question: what is inside this package?
 *
 * `graph_payload.bin` is 85 MB on a mid-sized corpus, so the engine is loaded once inside a worker (the same
 * worker the Taint room uses for blast radius) and every expand is a message. Doing this on the main thread
 * would block first paint for seconds — the exact trade the ingest rules forbid in the CLI and this room
 * forbids in the browser.
 */
import type { BlastWorkerIn, BlastWorkerOut } from "./blastWorker";

export interface ExpandedNode {
  index: number;
  name: string;
  node_type: number;
  node_type_name: string;
  complexity: number;
  file_path?: string | null;
}

export interface ExpandedSubgraph {
  nodes: ExpandedNode[];
  /** Member edges. The engine emits **calls** only — see `MEMBER_EDGE_KINDS`. */
  edges: Array<{ source: number; target: number; edge_type: number }>;
}

/**
 * What the engine actually draws between members.
 *
 * `expand_indices` filters to `CALLS_EDGE` (`crates/rgctl-wasm/src/columnar.rs`), while the L1 coupling web
 * is calls + references + uses. Every surface that shows member edges must say which of the two it is
 * drawing, so this constant is the single place that fact lives.
 */
export const MEMBER_EDGE_KINDS = "calls only";

let worker: Worker | null = null;
let ready: Promise<void> | null = null;
let seq = 0;
const pending = new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>();

function spawn(): Worker {
  const w = new Worker(new URL("./blastWorker.ts", import.meta.url), { type: "module" });
  w.onmessage = (event: MessageEvent<BlastWorkerOut>) => {
    const msg = event.data;
    if (msg.type === "error") {
      const entry = msg.requestId === undefined ? undefined : pending.get(msg.requestId);
      if (entry) {
        pending.delete(msg.requestId as number);
        entry.reject(new Error(msg.message));
      }
      return;
    }
    if (!("requestId" in msg)) return;
    const entry = pending.get(msg.requestId);
    if (!entry) return;
    pending.delete(msg.requestId);
    entry.resolve(msg);
  };
  return w;
}

function request<T>(message: Omit<BlastWorkerIn, "requestId">): Promise<T> {
  const id = ++seq;
  return new Promise<T>((resolve, reject) => {
    pending.set(id, { resolve: resolve as (v: unknown) => void, reject });
    worker?.postMessage({ ...message, requestId: id } as BlastWorkerIn);
  });
}

async function ensureWorker(): Promise<Worker> {
  if (!worker) worker = spawn();
  if (!ready) {
    ready = (async () => {
      await request<BlastWorkerOut>({ type: "init" });
    })().catch((error) => {
      ready = null;
      throw error;
    });
  }
  await ready;
  return worker;
}

/** Expand member indices into a subgraph. `typeMask = 0` keeps every node kind. */
export async function expandMembers(
  indices: Uint32Array,
  typeMask = 0,
): Promise<ExpandedSubgraph> {
  await ensureWorker();
  const result = await request<BlastWorkerOut>({ type: "expand", indices, typeMask } as never);
  if (result.type !== "expand_result") throw new Error(`unexpected worker reply: ${result.type}`);
  return result.payload;
}

/** Test seam. */
export function resetExpandEngine(): void {
  worker?.terminate();
  worker = null;
  ready = null;
  pending.clear();
}
