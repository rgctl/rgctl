import type Graph from "graphology";

export interface LayoutNodeInput {
  id: string;
  x: number;
  y: number;
}

export interface LayoutEdgeInput {
  source: string;
  target: string;
  weight?: number;
}

export interface LayoutRequest {
  nodes: LayoutNodeInput[];
  edges: LayoutEdgeInput[];
  iterations?: number;
}

export interface LayoutResponse {
  positions: Record<string, { x: number; y: number }>;
}

let layoutWorker: Worker | null = null;

function getLayoutWorker(): Worker {
  layoutWorker ??= new Worker(new URL("./layoutWorker.ts", import.meta.url), {
    type: "module",
  });
  return layoutWorker;
}

export function graphToLayoutRequest(graph: Graph, iterations?: number): LayoutRequest {
  const nodes: LayoutNodeInput[] = [];
  graph.forEachNode((id, attrs) => {
    nodes.push({
      id,
      x: (attrs.x as number) ?? Math.random(),
      y: (attrs.y as number) ?? Math.random(),
    });
  });

  const edges: LayoutEdgeInput[] = [];
  graph.forEachEdge((_edge, attrs, source, target) => {
    edges.push({
      source,
      target,
      weight: (attrs.weight as number) ?? 1,
    });
  });

  return { nodes, edges, iterations };
}

export function applyLayoutPositions(
  graph: Graph,
  positions: LayoutResponse["positions"],
): void {
  for (const [id, pos] of Object.entries(positions)) {
    if (graph.hasNode(id)) {
      graph.setNodeAttribute(id, "x", pos.x);
      graph.setNodeAttribute(id, "y", pos.y);
    }
  }
}

export function runLayoutWorker(request: LayoutRequest): Promise<LayoutResponse> {
  return new Promise((resolve, reject) => {
    const worker = getLayoutWorker();
    const onMessage = (event: MessageEvent<LayoutResponse>) => {
      cleanup();
      resolve(event.data);
    };
    const onError = (event: ErrorEvent) => {
      cleanup();
      reject(event.error ?? new Error(event.message));
    };
    const cleanup = () => {
      worker.removeEventListener("message", onMessage);
      worker.removeEventListener("error", onError);
    };
    worker.addEventListener("message", onMessage);
    worker.addEventListener("error", onError);
    worker.postMessage(request);
  });
}
