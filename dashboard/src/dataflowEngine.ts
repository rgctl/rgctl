import type { CfgDetailPayload, SlicePdgEdge, SlicePdgNode } from "./types";

export type DataflowViewMode = "dataflow" | "dominator";

export type DataflowEdgeKind = "data" | "control" | "cfg" | "dominates";

export interface DataflowGraphPayload {
  view_mode: DataflowViewMode;
  variable: string | null;
  include_control: boolean;
  include_cfg: boolean;
  nodes: DataflowGraphNode[];
  edges: DataflowGraphEdge[];
  lines: number[];
  data_edge_count: number;
  control_edge_count: number;
  cfg_edge_count: number;
}

export interface DataflowGraphNode {
  id: string;
  line: number;
  label: string;
  display_label: string;
  kind: "statement" | "block";
  defined: string[];
  used: string[];
  block_index?: number;
  frontier_size?: number;
}

export interface DataflowGraphEdge {
  source: string;
  target: string;
  kind: DataflowEdgeKind;
  variable?: string | null;
}

export interface DataflowBuildOptions {
  variable?: string | null;
  includeControl?: boolean;
  includeCfg?: boolean;
}

function nodeTouchesVariable(node: SlicePdgNode, variable: string): boolean {
  return node.defined.includes(variable) || node.used.includes(variable);
}

function truncateLabel(text: string, max = 50): string {
  const flat = text.replace(/\s+/g, " ").trim();
  if (flat.length <= max) return flat;
  return `${flat.slice(0, max - 1)}…`;
}

function resolveBlockIndex(
  node: SlicePdgNode,
  cfg: CfgDetailPayload | null,
): number | undefined {
  if (node.block_index != null) return node.block_index;
  if (!cfg) return undefined;
  const block = cfg.blocks.find(
    (b) =>
      node.line >= b.start_line &&
      node.line <= b.end_line &&
      (b.start_line > 0 || b.end_line > 0),
  );
  return block?.id;
}

function blockToNodes(
  nodes: SlicePdgNode[],
  cfg: CfgDetailPayload | null,
): Map<number, string[]> {
  const out = new Map<number, string[]>();
  for (const node of nodes) {
    const block = resolveBlockIndex(node, cfg);
    if (block == null) continue;
    if (!out.has(block)) out.set(block, []);
    out.get(block)!.push(node.id);
  }
  return out;
}

function cfgBridgeEdges(
  cfg: CfgDetailPayload,
  blockNodes: Map<number, string[]>,
  allowedNodeIds: Set<string>,
): DataflowGraphEdge[] {
  const edges: DataflowGraphEdge[] = [];
  for (const edge of cfg.edges) {
    const sourceNodes = blockNodes.get(edge.from) ?? [];
    const targetNodes = blockNodes.get(edge.to) ?? [];
    if (sourceNodes.length === 0 || targetNodes.length === 0) continue;
    const source = sourceNodes[sourceNodes.length - 1]!;
    const target = targetNodes[0]!;
    if (!allowedNodeIds.has(source) || !allowedNodeIds.has(target)) continue;
    edges.push({ source, target, kind: "cfg" });
  }
  return edges;
}

export function computeDataflowGraph(
  pdgNodes: SlicePdgNode[],
  pdgEdges: SlicePdgEdge[],
  cfg: CfgDetailPayload | null,
  options: DataflowBuildOptions = {},
): DataflowGraphPayload {
  const varFilter = options.variable?.trim() || null;
  const includeControl = options.includeControl ?? true;
  const includeCfg = options.includeCfg ?? true;
  const allDataEdges = pdgEdges.filter((e) => e.kind === "data");

  let dataEdges: SlicePdgEdge[];
  const nodeIds = new Set<string>();

  if (!varFilter) {
    dataEdges = allDataEdges;
    for (const n of pdgNodes) nodeIds.add(n.id);
  } else {
    dataEdges = allDataEdges.filter((e) => e.variable === varFilter);
    for (const e of dataEdges) {
      nodeIds.add(e.source);
      nodeIds.add(e.target);
    }
    for (const n of pdgNodes) {
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
    controlEdges = pdgEdges.filter(
      (e) =>
        e.kind === "control" &&
        nodeIds.has(e.source) &&
        nodeIds.has(e.target),
    );
  }

  const filteredNodes = pdgNodes
    .filter((n) => nodeIds.has(n.id))
    .sort((a, b) => a.line - b.line || a.id.localeCompare(b.id));

  const graphNodes: DataflowGraphNode[] = filteredNodes.map((n) => ({
    id: n.id,
    line: n.line,
    label: n.label,
    display_label: truncateLabel(n.label),
    kind: "statement",
    defined: n.defined,
    used: n.used,
    block_index: resolveBlockIndex(n, cfg),
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

  let cfgEdgeCount = 0;
  if (includeCfg && cfg && filteredNodes.length > 0) {
    const blockNodes = blockToNodes(filteredNodes, cfg);
    const cfgEdges = cfgBridgeEdges(cfg, blockNodes, nodeIds);
    cfgEdgeCount = cfgEdges.length;
    graphEdges.push(...cfgEdges);
  }

  const lines = [...new Set(filteredNodes.map((n) => n.line))].sort((a, b) => a - b);

  return {
    view_mode: "dataflow",
    variable: varFilter,
    include_control: includeControl,
    include_cfg: includeCfg,
    nodes: graphNodes,
    edges: graphEdges,
    lines,
    data_edge_count: dataEdges.length,
    control_edge_count: controlEdges.length,
    cfg_edge_count: cfgEdgeCount,
  };
}

export function computeDominatorGraph(cfg: CfgDetailPayload): DataflowGraphPayload {
  const nodes: DataflowGraphNode[] = cfg.blocks.map((block) => {
    const frontier = cfg.dominance_frontiers[block.id] ?? [];
    return {
      id: `block_${block.id}`,
      line: block.start_line,
      label: block.label,
      display_label: block.label,
      kind: "block",
      defined: [],
      used: [],
      block_index: block.id,
      frontier_size: frontier.length,
    };
  });

  const edges: DataflowGraphEdge[] = [];
  cfg.idom.forEach((parent, blockId) => {
    if (parent == null || parent === blockId) return;
    edges.push({
      source: `block_${parent}`,
      target: `block_${blockId}`,
      kind: "dominates",
    });
  });

  return {
    view_mode: "dominator",
    variable: null,
    include_control: false,
    include_cfg: false,
    nodes,
    edges,
    lines: [],
    data_edge_count: 0,
    control_edge_count: 0,
    cfg_edge_count: 0,
  };
}

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

/** Line numbers to highlight in the source panel for a graph node selection. */
export function highlightLinesForGraphNode(
  pdgNodes: SlicePdgNode[],
  cfg: CfgDetailPayload | null,
  graph: DataflowGraphPayload,
  selectedGraphNodeId: string | null,
): Set<number> {
  if (!selectedGraphNodeId) return new Set();

  const selected = graph.nodes.find((n) => n.id === selectedGraphNodeId);
  if (!selected) return new Set();

  if (graph.view_mode === "dataflow" || selected.kind === "statement") {
    return selected.line > 0 ? new Set([selected.line]) : new Set();
  }

  const blockId = selected.block_index;
  if (blockId == null) return new Set();

  const block = cfg?.blocks.find((b) => b.id === blockId);
  const lines = new Set<number>();

  for (const node of pdgNodes) {
    if (node.block_index === blockId) {
      if (node.line > 0) lines.add(node.line);
      continue;
    }
    if (
      block &&
      block.start_line > 0 &&
      node.line >= block.start_line &&
      node.line <= block.end_line
    ) {
      lines.add(node.line);
    }
  }

  if (lines.size === 0 && block && block.start_line > 0 && block.end_line >= block.start_line) {
    for (const node of pdgNodes) {
      if (node.line >= block.start_line && node.line <= block.end_line) {
        lines.add(node.line);
      }
    }
  }

  return lines;
}
