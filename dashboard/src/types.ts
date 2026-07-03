export interface DashboardManifest {
  schema_version: number;
  dashboard_version: string;
  phases: Record<string, string>;
  graph: {
    payload_path: string;
    payload_format: string;
    node_count: number;
    edge_count: number;
    digest: string;
  };
  view?: ViewSection;
  metrics: {
    function_count: number;
    class_count: number;
    calls_count: number;
    avg_complexity: number;
    high_blast_radius_count: number;
  };
  generated_at: string;
}

export interface ViewSection {
  metagraph_path: string;
  metagraph_schema_version: number;
  metanode_count: number;
  metaedge_count: number;
  mode: string;
  community_only: boolean;
  threshold_community_only: number;
}

export interface MetagraphPayload {
  schema_version: number;
  mode: string;
  community_only: boolean;
  threshold_community_only: number;
  source_node_count: number;
  nodes: Metanode[];
  edges: Metaedge[];
}

export interface Metanode {
  id: number;
  label: string;
  size: number;
  functions: number;
  classes: number;
  avg_complexity: number;
  x: number;
  y: number;
  member_indices?: number[];
}

export interface Metaedge {
  source: number;
  target: number;
  weight: number;
  kind: string;
}

export interface SubgraphNode {
  index: number;
  name: string;
  node_type: number;
  node_type_name: string;
  complexity: number;
  file_path?: string | null;
}

export interface SubgraphEdge {
  source: number;
  target: number;
  edge_type: number;
}

export interface SubgraphPayload {
  nodes: SubgraphNode[];
  edges: SubgraphEdge[];
}

export interface NodeListEntry {
  index: number;
  name: string;
  node_type: number;
  node_type_name: string;
  complexity: number;
  file_path?: string | null;
}

export interface NodeListPayload {
  total: number;
  offset: number;
  items: NodeListEntry[];
}

export interface EngineReady {
  nodeCount: number;
  edgeCount: number;
  schemaVersion: number;
  digest: string;
  wasm: boolean;
}

/** Node-type bitmask (matches columnar u16 encoding). */
export const NODE_TYPE_MASK = {
  Function: 1 << 0,
  Class: 1 << 1,
  Struct: 1 << 2,
  Enum: 1 << 3,
  Interface: 1 << 4,
  Module: 1 << 5,
} as const;

export const DEFAULT_GRAPH_TYPE_MASK =
  NODE_TYPE_MASK.Function | NODE_TYPE_MASK.Class;

export const NODE_TYPE_FILTER_OPTIONS = [
  { bit: NODE_TYPE_MASK.Function, label: "Function" },
  { bit: NODE_TYPE_MASK.Class, label: "Class" },
  { bit: NODE_TYPE_MASK.Struct, label: "Struct" },
  { bit: NODE_TYPE_MASK.Interface, label: "Interface" },
  { bit: NODE_TYPE_MASK.Module, label: "Module" },
] as const;

export type WorkerInWithoutId =
  | { type: "init" }
  | { type: "expand"; indices: number[]; typeMask: number }
  | { type: "list_nodes"; typeMask: number; offset: number; limit: number };

export type WorkerIn =
  | { type: "init" }
  | { type: "expand"; requestId: number; indices: number[]; typeMask: number }
  | { type: "list_nodes"; requestId: number; typeMask: number; offset: number; limit: number };

export type WorkerOut =
  | {
      type: "ready";
      nodeCount: number;
      edgeCount: number;
      schemaVersion: number;
      digest: string;
      wasm: boolean;
    }
  | { type: "subgraph"; requestId: number; payload: SubgraphPayload }
  | { type: "node_list"; requestId: number; payload: NodeListPayload }
  | { type: "error"; requestId?: number; message: string };

export async function loadManifest(): Promise<DashboardManifest> {
  const embedded = document.getElementById("rbuilder-manifest");
  if (embedded?.textContent) {
    return JSON.parse(embedded.textContent) as DashboardManifest;
  }
  const res = await fetch("./manifest.json");
  if (!res.ok) {
    throw new Error(`manifest.json: HTTP ${res.status}`);
  }
  return (await res.json()) as DashboardManifest;
}
