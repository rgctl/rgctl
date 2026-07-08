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

function nodeTouchesVariable(node: SlicePdgNode, variable: string): boolean {
  return node.defined.includes(variable) || node.used.includes(variable);
}

export function computeDataflowGraph(
  nodes: SlicePdgNode[],
  edges: SlicePdgEdge[],
  variable: string | null,
  includeControl: boolean,
): DataflowGraphPayload {
  const varFilter = variable?.trim() || null;
  const allDataEdges = edges.filter((e) => e.kind === "data");

  let dataEdges: SlicePdgEdge[];
  const nodeIds = new Set<string>();

  if (!varFilter) {
    dataEdges = allDataEdges;
    for (const n of nodes) nodeIds.add(n.id);
  } else {
    dataEdges = allDataEdges.filter((e) => e.variable === varFilter);
    for (const e of dataEdges) {
      nodeIds.add(e.source);
      nodeIds.add(e.target);
    }
    for (const n of nodes) {
      if (nodeTouchesVariable(n, varFilter)) {
        nodeIds.add(n.id);
      }
    }
    if (dataEdges.length === 0 && nodeIds.size > 0) {
      dataEdges = allDataEdges.filter(
        (e) => nodeIds.has(e.source) && nodeIds.has(e.target),
      );
    }
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

/** Variables that have PDG data edges (preferred for the filter dropdown). */
export function listPdgVariables(nodes: SlicePdgNode[], edges: SlicePdgEdge[]): string[] {
  const fromEdges = new Set<string>();
  for (const e of edges) {
    if (e.kind === "data" && e.variable) {
      fromEdges.add(e.variable);
    }
  }
  if (fromEdges.size > 0) {
    return [...fromEdges].sort((a, b) => a.localeCompare(b));
  }
  const vars = new Set<string>();
  for (const n of nodes) {
    for (const v of n.defined) vars.add(v);
    for (const v of n.used) vars.add(v);
  }
  return [...vars].sort((a, b) => a.localeCompare(b));
}
