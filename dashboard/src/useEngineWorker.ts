import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import type { BlastRadiusPayload, CfgDetailPayload, DataflowGraphPayload, EngineReady, NodeListPayload, SliceDirection, SliceResultPayload, SubgraphPayload, WorkerInWithoutId, WorkerOut } from "./types";

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
        if (data.type === "subgraph") p.resolve(data.payload);
        else if (data.type === "node_list") p.resolve(data.payload);
        else if (data.type === "slice_result") p.resolve(data.payload);
        else if (data.type === "blast_result") p.resolve(data.payload);
        else if (data.type === "dataflow_result") p.resolve(data.payload);
        else if (data.type === "cfg_detail_result") p.resolve(data.payload);
      }
    };

    worker.postMessage({ type: "init" } satisfies WorkerInWithoutId);
    return () => {
      worker.terminate();
      workerRef.current = null;
    };
  }, []);

  const send = useCallback(<T,>(msg: Exclude<WorkerInWithoutId, { type: "init" }>): Promise<T> => {
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
      worker.postMessage({ ...msg, requestId });
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

  const computeSliceRequest = useCallback(
    (functionId: string, line: number, variable: string, direction: SliceDirection) =>
      send<SliceResultPayload>({
        type: "compute_slice",
        functionId,
        line,
        variable,
        direction,
      }),
    [send],
  );

  const blastRadius = useCallback(
    (nodeIndex: number, maxDepth: number) =>
      send<BlastRadiusPayload>({ type: "blast_radius", nodeIndex, maxDepth }),
    [send],
  );

  const computeDataflow = useCallback(
    (functionId: string, variable: string | null, includeControl: boolean) =>
      send<DataflowGraphPayload>({
        type: "compute_dataflow",
        functionId,
        variable,
        includeControl,
      }),
    [send],
  );

  const loadCfgDetail = useCallback(
    (functionId: string) => send<CfgDetailPayload>({ type: "load_cfg_detail", functionId }),
    [send],
  );

  return {
    engine,
    error,
    expand,
    listNodes,
    computeSlice: computeSliceRequest,
    blastRadius,
    computeDataflow,
    loadCfgDetail,
    wasmReady: engine?.wasm ?? false,
  };
}
