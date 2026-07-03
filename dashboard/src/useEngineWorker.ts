import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { EngineReady, NodeListPayload, SubgraphPayload, WorkerIn, WorkerInWithoutId, WorkerOut } from "./types";

export function useEngineWorker() {
  const workerRef = useRef<Worker | null>(null);
  const nextId = useRef(1);
  const pending = useRef(
    new Map<number, { resolve: (v: unknown) => void; reject: (e: Error) => void }>(),
  );
  const [engine, setEngine] = useState<EngineReady | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const worker = new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
    workerRef.current = worker;

    worker.onmessage = (ev: MessageEvent<WorkerOut>) => {
      const data = ev.data;
      if (data.type === "ready") {
        setEngine({
          nodeCount: data.nodeCount,
          edgeCount: data.edgeCount,
          schemaVersion: data.schemaVersion,
          digest: data.digest,
          wasm: data.wasm,
        });
        return;
      }
      if (data.type === "error") {
        if (data.requestId != null) {
          pending.current.get(data.requestId)?.reject(new Error(data.message));
          pending.current.delete(data.requestId);
        } else {
          setError(data.message);
        }
        return;
      }
      if (data.requestId != null) {
        const p = pending.current.get(data.requestId);
        if (!p) return;
        pending.current.delete(data.requestId);
        if (data.type === "subgraph") {
          p.resolve(data.payload);
        } else if (data.type === "node_list") {
          p.resolve(data.payload);
        }
      }
    };

    worker.postMessage({ type: "init" } satisfies WorkerIn);
    return () => {
      worker.terminate();
      workerRef.current = null;
    };
  }, []);

  const send = useCallback(<T>(msg: Exclude<WorkerInWithoutId, { type: "init" }>): Promise<T> => {
    return new Promise((resolve, reject) => {
      const worker = workerRef.current;
      if (!worker) {
        reject(new Error("worker not running"));
        return;
      }
      const requestId = nextId.current++;
      pending.current.set(requestId, {
        resolve: resolve as (v: unknown) => void,
        reject,
      });
      worker.postMessage({ ...msg, requestId } as WorkerIn);
    });
  }, []);

  const expand = useCallback(
    (indices: number[], typeMask: number) =>
      send<SubgraphPayload>({ type: "expand", indices, typeMask }),
    [send],
  );

  const listNodes = useCallback(
    (typeMask: number, offset: number, limit: number) =>
      send<NodeListPayload>({ type: "list_nodes", typeMask, offset, limit }),
    [send],
  );

  return { engine, error, expand, listNodes, wasmReady: engine?.wasm ?? false };
}
