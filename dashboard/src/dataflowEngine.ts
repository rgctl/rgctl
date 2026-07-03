import type { SlicePdgEdge, SlicePdgNode } from "./types";

export interface DataflowGraphPayload {
  variable: string | null;
  include_control: boolean;
  nodes: DataflowGraphNode[];
  edges: DataflowGraphEdge[];
  lines: number[];
  data_edge_count: number;
  control_edge_count: number;
}

export interface DataflowGraphNode {
  id: string;
  line: number;
  label: string;
  defined: string[];
  used: string[];
}

export interface DataflowGraphEdge {
  source: string;
  target: string;
  kind: "data" | "control";
  variable?: string | null;
}

export function computeDataflowGraph(
  nodes: SlicePdgNode[],
  edges: SlicePdgEdge[],
  variable: string | null,
  includeControl: boolean,
): DataflowGraphPayload {
  const varFilter = variable?.trim() || null;

  let dataEdges = edges.filter((e) => e.kind === "data");
  if (varFilter) {
    dataEdges = dataEdges.filter((e) => e.variable === varFilter);
  }

  const nodeIds = new Set<string>();
  for (const e of dataEdges) {
    nodeIds.add(e.source);
    nodeIds.add(e.target);
  }

  let controlEdges: SlicePdgEdge[] = [];
  if (includeControl && nodeIds.size > 0) {
    controlEdges = edges.filter(
      (e) =>
        e.kind === "control" &&
        nodeIds.has(e.source) &&
        nodeIds.has(e.target),
    );
  }

  if (!varFilter && nodeIds.size === 0) {
    for (const n of nodes) nodeIds.add(n.id);
    dataEdges = edges.filter((e) => e.kind === "data");
    if (includeControl) {
      controlEdges = edges.filter((e) => e.kind === "control");
    }
  }

  const filteredNodes = nodes
    .filter((n) => nodeIds.has(n.id))
    .sort((a, b) => a.line - b.line || a.id.localeCompare(b.id));

  const graphNodes: DataflowGraphNode[] = filteredNodes.map((n) => ({
    id: n.id,
    line: n.line,
    label: n.label,
    defined: n.defined,
    used: n.used,
  }));

  const graphEdges: DataflowGraphEdge[] = [
    ...dataEdges.map((e) => ({
      source: e.source,
      target: e.target,
      kind: "data" as const,
      variable: e.variable,
    })),
    ...controlEdges.map((e) => ({
      source: e.source,
      target: e.target,
      kind: "control" as const,
      variable: e.variable,
    })),
  ];

  const lines = [...new Set(filteredNodes.map((n) => n.line))].sort((a, b) => a - b);

  return {
    variable: varFilter,
    include_control: includeControl,
    nodes: graphNodes,
    edges: graphEdges,
    lines,
    data_edge_count: dataEdges.length,
    control_edge_count: controlEdges.length,
  };
}

export function listPdgVariables(nodes: SlicePdgNode[]): string[] {
  const vars = new Set<string>();
  for (const n of nodes) {
    for (const v of n.defined) vars.add(v);
    for (const v of n.used) vars.add(v);
  }
  return [...vars].sort((a, b) => a.localeCompare(b));
}
