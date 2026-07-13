import Graph from "graphology";
import circular from "graphology-layout/circular";
import forceAtlas2 from "graphology-layout-forceatlas2";
import type { LayoutRequest, LayoutResponse } from "./layoutClient";

function runForceAtlas2(req: LayoutRequest): LayoutResponse {
  const graph = new Graph();
  for (const node of req.nodes) {
    graph.addNode(node.id, { x: node.x, y: node.y });
  }
  for (const edge of req.edges) {
    if (
      graph.hasNode(edge.source) &&
      graph.hasNode(edge.target) &&
      edge.source !== edge.target
    ) {
      try {
        graph.addEdge(edge.source, edge.target, { weight: edge.weight ?? 1 });
      } catch {
        // parallel edges ignored
      }
    }
  }

  if (graph.order === 0) {
    return { positions: {} };
  }

  circular.assign(graph, { scale: graph.order > 80 ? 200 : 120 });
  const iters =
    req.iterations ??
    (graph.order > 120 ? 250 : graph.order > 40 ? 180 : 120);
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
      edgeWeightInfluence: 1,
    },
  });

  const positions: LayoutResponse["positions"] = {};
  graph.forEachNode((id, attrs) => {
    positions[id] = { x: attrs.x as number, y: attrs.y as number };
  });
  return { positions };
}

self.onmessage = (event: MessageEvent<LayoutRequest>) => {
  self.postMessage(runForceAtlas2(event.data));
};
