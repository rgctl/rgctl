/**
 * Live blast-radius expand for a taint subject (function UUID).
 *
 * Engine load is deferred until `enabled` — metasfresh's graph_payload is huge.
 */
import { useEffect, useRef, useState } from "react";
import type { BlastWorkerIn, BlastWorkerOut } from "../blastWorker";

export interface BlastCaller {
  index: number;
  name: string;
  depth: number;
  node_type: number;
  node_type_name: string;
}

export interface BlastPayload {
  seed_index: number;
  seed_name: string;
  depth_limit: number;
  direct_caller_count: number;
  impact_zone_count: number;
  score: number;
  callers: BlastCaller[];
}

export type BlastRadiusState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; payload: BlastPayload }
  | { kind: "absent"; reason: string }
  | { kind: "failed"; reason: string };

const DEPTH_DEBOUNCE_MS = 300;

export function useBlastRadius(
  functionId: string | null,
  depth: number,
  enabled: boolean,
): BlastRadiusState {
  const [state, setState] = useState<BlastRadiusState>({ kind: "idle" });
  const [workerEpoch, setWorkerEpoch] = useState(0);
  const workerRef = useRef<Worker | null>(null);
  const nextId = useRef(1);
  const pending = useRef(
    new Map<number, { resolve: (v: BlastWorkerOut) => void; reject: (e: Error) => void }>(),
  );
  const engineReady = useRef(false);

  useEffect(() => {
    if (!enabled) {
      setState({ kind: "idle" });
      workerRef.current = null;
      engineReady.current = false;
      return;
    }
    if (typeof Worker === "undefined") {
      setState({
        kind: "failed",
        reason: "Web Workers are unavailable in this environment — cannot load the blast engine.",
      });
      return;
    }

    const worker = new Worker(new URL("../blastWorker.ts", import.meta.url), { type: "module" });
    workerRef.current = worker;
    worker.onmessage = (ev: MessageEvent<BlastWorkerOut>) => {
      const data = ev.data;
      if (data.type === "error") {
        if (data.requestId != null) {
          pending.current.get(data.requestId)?.reject(new Error(data.message));
          pending.current.delete(data.requestId);
        } else {
          setState({ kind: "failed", reason: data.message });
        }
        return;
      }
      if (data.requestId != null) {
        const p = pending.current.get(data.requestId);
        if (!p) return;
        pending.current.delete(data.requestId);
        p.resolve(data);
      }
    };
    worker.onerror = (ev) => {
      setState({ kind: "failed", reason: ev.message || "blast worker error" });
    };
    setWorkerEpoch((n) => n + 1);

    return () => {
      worker.terminate();
      if (workerRef.current === worker) workerRef.current = null;
      engineReady.current = false;
      pending.current.clear();
    };
  }, [enabled]);

  useEffect(() => {
    if (!enabled) return;
    if (!functionId) {
      setState({
        kind: "absent",
        reason: "No function is selected, so there is no blast seed.",
      });
      return;
    }

    const worker = workerRef.current;
    if (!worker) return;

    let cancelled = false;
    setState({ kind: "loading" });

    const send = (msg: BlastWorkerIn) =>
      new Promise<BlastWorkerOut>((resolve, reject) => {
        pending.current.set(msg.requestId, { resolve, reject });
        worker.postMessage(msg);
      });

    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          if (!engineReady.current) {
            const ready = await send({ type: "init", requestId: nextId.current++ });
            if (cancelled) return;
            if (ready.type !== "ready") {
              throw new Error("blast engine init failed");
            }
            engineReady.current = true;
          }
          const result = await send({
            type: "blast_by_uuid",
            requestId: nextId.current++,
            uuid: functionId,
            maxDepth: depth,
          });
          if (cancelled) return;
          if (result.type !== "blast_result") {
            throw new Error("unexpected blast worker response");
          }
          setState({ kind: "ready", payload: result.payload });
        } catch (e) {
          if (cancelled) return;
          const message = e instanceof Error ? e.message : String(e);
          if (/no columnar node/i.test(message)) {
            setState({
              kind: "absent",
              reason: `UUID ${functionId} is not in graph_payload.bin — cannot expand blast.`,
            });
          } else {
            setState({ kind: "failed", reason: message });
          }
        }
      })();
    }, DEPTH_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [enabled, functionId, depth, workerEpoch]);

  return state;
}
