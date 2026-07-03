import Graph from "graphology";
import circular from "graphology-layout/circular";
import forceAtlas2 from "graphology-layout-forceatlas2";

/** Spread nodes with ForceAtlas2 (better than fixed circle for package graphs). */
export function layoutForceAtlas2(graph: Graph, iterations?: number): void {
  if (graph.order === 0) return;
  circular.assign(graph, { scale: graph.order > 80 ? 200 : 120 });
  const iters = iterations ?? (graph.order > 120 ? 250 : graph.order > 40 ? 180 : 120);
  const settings = forceAtlas2.inferSettings(graph);
  forceAtlas2.assign(graph, {
    iterations: iters,
    settings: {
      ...settings,
      gravity: 0.8,
      scalingRatio: graph.order > 80 ? 18 : 12,
      strongGravityMode: false,
      slowDown: 2,
      barnesHutOptimize: graph.order > 80,
    },
  });
}

/** Weakly-connected components for community coloring. */
export function componentColors(
  nodeIds: string[],
  edges: Array<{ source: string; target: string }>,
): Map<string, string> {
  const parent = new Map<string, string>();
  for (const id of nodeIds) parent.set(id, id);

  const find = (id: string): string => {
    let root = id;
    while (parent.get(root) !== root) root = parent.get(root)!;
    let cur = id;
    while (parent.get(cur) !== root) {
      const next = parent.get(cur)!;
      parent.set(cur, root);
      cur = next;
    }
    return root;
  };

  const unite = (a: string, b: string) => {
    const ra = find(a);
    const rb = find(b);
    if (ra !== rb) parent.set(rb, ra);
  };

  for (const e of edges) {
    if (parent.has(e.source) && parent.has(e.target)) unite(e.source, e.target);
  }

  const roots = new Map<string, number>();
  for (const id of nodeIds) {
    const r = find(id);
    if (!roots.has(r)) roots.set(r, roots.size);
  }

  const colors = new Map<string, string>();
  for (const id of nodeIds) {
    const idx = roots.get(find(id)) ?? 0;
    colors.set(id, communityColor(idx));
  }
  return colors;
}

function communityColor(index: number): string {
  const hue = (index * 47 + 210) % 360;
  return `hsl(${hue} 58% 52%)`;
}

/** Shorten long Java package paths for on-graph labels. */
export function shortGraphLabel(label: string): string {
  const parts = label.split(".").filter(Boolean);
  if (parts.length <= 4) return label;
  return `…${parts.slice(-4).join(".")}`;
}
