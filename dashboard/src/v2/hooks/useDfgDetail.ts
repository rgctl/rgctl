/**
 * Load PDG / dataflow detail for the active function.
 *
 * Prefers `slice/<id>.json` when present; falls back to HTTP Range on
 * `slice.record_{index,data}.bin` when `manifest.capabilities.slice_detail_archive`
 * is set (or when the per-file JSON 404s and a record pack exists).
 */
import { useEffect, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import type { Manifest } from "../data";
import type { DfgDetail } from "../rooms/DfgGraph";

export type DfgState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; dfg: DfgDetail }
  | { kind: "absent"; reason: string }
  | { kind: "failed"; reason: string };

/**
 * Detail cache shared by Understand and Taint. v1's worker kept slice bundles in a Map; v2 was
 * refetching and re-parsing the same `slice/<uuid>.json` every time a lens remounted.
 */
const dfgCache = new Map<string, Promise<DfgDetail | null>>();

/** Test seam. */
export function resetDfgDetailCache(): void {
  dfgCache.clear();
}

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

function toDfg(raw: SliceBundle): DfgDetail | null {
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
  manifest: Manifest | null = null,
): DfgState {
  const [state, setState] = useState<DfgState>({ kind: "idle" });
  const queryClient = useQueryClient();
  const resolvedManifest =
    manifest ?? queryClient.getQueryData<Manifest | null>(["manifest"]) ?? null;

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
    const cacheKey = `${detailDir}\x1f${functionId}\x1f${resolvedManifest?.graph?.digest ?? ""}`;
    let pending = dfgCache.get(cacheKey);
    if (!pending) {
      const created: Promise<DfgDetail | null> = fetch(
        `/api/pdg?id=${encodeURIComponent(functionId)}`,
      ).then(async (res) => (res.ok ? toDfg((await res.json()) as SliceBundle) : null));
      dfgCache.set(cacheKey, created);
      // A failed read is not a cache entry: a retry after the bundle/server recovers should try again.
      created.catch(() => dfgCache.delete(cacheKey));
      pending = created;
    }
    pending
      .then((dfg) => {
        if (cancelled) return;
        setState(
          dfg
            ? { kind: "ready", dfg }
            : {
                kind: "absent",
                reason: `No PDG detail for \`${functionId}\` (checked slice JSON and record pack).`,
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
  }, [functionId, enabled, detailDir, resolvedManifest]);

  return state;
}
