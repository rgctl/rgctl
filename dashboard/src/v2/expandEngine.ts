/**
 * Architecture expansion over the API.
 *
 * The browser no longer downloads `graph_payload.bin` or starts a WASM worker. The server owns the
 * columnar view and expands member indices; this module only shapes the request.
 */
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
  edges: Array<{ source: number; target: number; edge_type: number }>;
}

export interface ExpandProgress {
  phase: "engine" | "parsing" | "expanding";
  loaded: number;
  total: number;
}

export type ExpandProgressListener = (progress: ExpandProgress) => void;

export async function expandMembers(
  indices: Uint32Array,
  typeMask = 0,
  _onProgress?: ExpandProgressListener,
): Promise<ExpandedSubgraph> {
  const query = new URLSearchParams({
    indices: Array.from(indices).join(","),
    type_mask: String(typeMask),
  });
  const response = await fetch(`/api/expand?${query.toString()}`);
  if (!response.ok) throw new Error(`expand: HTTP ${response.status}`);
  return (await response.json()) as ExpandedSubgraph;
}

/** Test seam retained for callers that used to reset the worker. */
export function resetExpandEngine(): void {}
