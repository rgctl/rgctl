/**
 * Live blast-radius expand for a focused function UUID.
 *
 * The engine lives in `expandEngine` as a module-level singleton shared with Architecture expansion.
 * This hook owns only the debounce and the four-state result; a per-lens Worker would reload the
 * graph payload on every Blast mount (Understand -> CFG -> Blast, Taint -> Understand -> Blast).
 */
import { useEffect, useState } from "react";
import { blastByUuid, type SharedBlastPayload } from "../expandEngine";

export interface BlastCaller {
  index: number;
  name: string;
  depth: number;
  node_type: number;
  node_type_name: string;
}

export type BlastPayload = SharedBlastPayload;

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

  useEffect(() => {
    if (!enabled) {
      setState({ kind: "idle" });
      return;
    }
    if (!functionId) {
      setState({
        kind: "absent",
        reason: "No function is selected, so there is no blast seed.",
      });
      return;
    }

    let cancelled = false;
    setState({ kind: "loading" });

    const timer = window.setTimeout(() => {
      void (async () => {
        try {
          const payload = await blastByUuid(functionId, depth);
          if (cancelled) return;
          setState({ kind: "ready", payload });
        } catch (error) {
          if (cancelled) return;
          const message = error instanceof Error ? error.message : String(error);
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
  }, [enabled, functionId, depth]);

  return state;
}
