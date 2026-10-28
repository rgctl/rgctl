/**
 * Load PDG / dataflow detail for the active function (`slice/<id>.json` by default).
 */
import { useEffect, useState } from "react";
import { bundleUrl } from "../data";
import type { DfgDetail } from "../rooms/DfgGraph";

export type DfgState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; dfg: DfgDetail }
  | { kind: "absent"; reason: string }
  | { kind: "failed"; reason: string };

interface SliceBundle {
  function_id: string;
  name: string;
  pdg?: {
    nodes?: Array<{
      id: string;
      line: number;
      label: string;
      kind?: string;
      defined?: string[];
      used?: string[];
    }>;
    edges?: Array<{
      source: string;
      target: string;
      kind: string;
      variable?: string | null;
    }>;
  };
}

export async function loadDfgDetail(
  functionId: string,
  detailDir = "slice",
  fetchImpl: typeof fetch = fetch,
): Promise<DfgDetail | null> {
  const res = await fetchImpl(bundleUrl(`${detailDir}/${functionId}.json`), { cache: "no-store" });
  if (!res.ok) return null;
  const raw = (await res.json()) as SliceBundle;
  const nodes = raw.pdg?.nodes ?? [];
  const edges = raw.pdg?.edges ?? [];
  if (nodes.length === 0) return null;
  return {
    function_id: raw.function_id,
    name: raw.name,
    nodes,
    edges,
  };
}

export function useDfgDetail(
  functionId: string | null,
  enabled: boolean,
  detailDir = "slice",
): DfgState {
  const [state, setState] = useState<DfgState>({ kind: "idle" });

  useEffect(() => {
    if (!enabled) {
      setState({ kind: "idle" });
      return;
    }
    if (!functionId) {
      setState({
        kind: "absent",
        reason: "No function is selected, so there is no DFG sidecar to load.",
      });
      return;
    }
    let cancelled = false;
    setState({ kind: "loading" });
    loadDfgDetail(functionId, detailDir)
      .then((dfg) => {
        if (cancelled) return;
        setState(
          dfg
            ? { kind: "ready", dfg }
            : {
                kind: "absent",
                reason: `No PDG detail at \`${detailDir}/${functionId}.json\` for this subject.`,
              },
        );
      })
      .catch((err: unknown) => {
        if (cancelled) return;
        setState({ kind: "failed", reason: String(err) });
      });
    return () => {
      cancelled = true;
    };
  }, [functionId, enabled, detailDir]);

  return state;
}
