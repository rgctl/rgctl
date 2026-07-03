import type { SliceDirection, SlicePdgEdge, SlicePdgNode, SliceResultPayload } from "./types";

export function computeSlice(
  nodes: SlicePdgNode[],
  edges: SlicePdgEdge[],
  line: number,
  variable: string,
  direction: SliceDirection,
  totalLines: number,
): SliceResultPayload {
  const criterionNode = nodes.find(
    (n) =>
      n.line === line &&
      (n.defined.includes(variable) || n.used.includes(variable)),
  );
  if (!criterionNode) {
    throw new Error(`No PDG node for "${variable}" at line ${line}`);
  }

  const sliceNodes = new Set<string>();
  const queue = [criterionNode.id];

  while (queue.length) {
    const id = queue.shift()!;
    if (sliceNodes.has(id)) continue;
    sliceNodes.add(id);

    if (direction === "backward") {
      for (const e of edges) {
        if (e.target === id) queue.push(e.source);
      }
    } else {
      for (const e of edges) {
        if (e.source === id) queue.push(e.target);
      }
    }
  }

  const lines = new Set<number>();
  for (const n of nodes) {
    if (sliceNodes.has(n.id)) lines.add(n.line);
  }

  const sliceNodeList = nodes.filter((n) => sliceNodes.has(n.id));
  const sliceEdges = edges.filter(
    (e) => sliceNodes.has(e.source) && sliceNodes.has(e.target),
  );

  const reduction =
    totalLines <= 0 ? 0 : 100 * (1 - lines.size / totalLines);

  return {
    criterion: { line, variable },
    direction,
    reduction_percent: reduction,
    lines: [...lines].sort((a, b) => a - b),
    nodes: sliceNodeList,
    edges: sliceEdges,
  };
}
