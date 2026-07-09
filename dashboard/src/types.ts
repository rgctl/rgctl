import { bundleDataUrl } from "./bundleUrl";

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
  analysis?: AnalysisSection;
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
  communities_path?: string | null;
  communities_schema_version?: number | null;
  community_count?: number | null;
}

export interface BlastRadiusPayload {
  seed_index: number;
  seed_name: string;
  depth_limit: number;
  direct_caller_count: number;
  impact_zone_count: number;
  score: number;
  callers: BlastCallerEntry[];
}

export interface BlastCallerEntry {
  index: number;
  name: string;
  depth: number;
  node_type: number;
  node_type_name: string;
}

export interface BlastIndexPayload {
  schema_version: number;
  available: boolean;
  snapshot_path?: string | null;
  snapshot_copied?: boolean;
  functions?: BlastFunctionScore[];
}

export interface BlastFunctionScore {
  index: number;
  score: number;
  direct: number;
  zone: number;
}

export interface AnalysisSection {
  cfg_available: boolean;
  cfg_index_path: string;
  cfg_detail_dir: string;
  cfg_archive_path?: string | null;
  cfg_function_count: number;
  slice_available: boolean;
  slice_index_path: string;
  slice_detail_dir: string;
  slice_function_count: number;
  blast_available: boolean;
  blast_index_path: string;
  function_metrics_path?: string;
  blast_snapshot_path?: string | null;
  dataflow_available: boolean;
  dataflow_index_path: string;
  dataflow_detail_dir: string;
  dataflow_function_count: number;
  taint_available: boolean;
  taint_index_path: string;
  taint_detail_dir: string;
  taint_function_count: number;
  taint_flow_count: number;
  taint_vulnerable_count: number;
  migration_graph_path?: string | null;
  migration_plan_path?: string | null;
  migration_available?: boolean;
  migration_community_count?: number | null;
}

export interface DataflowIndexPayload {
  schema_version: number;
  available: boolean;
  detail_dir: string;
  function_count: number;
  functions: DataflowFunctionEntry[];
}

export interface DataflowFunctionEntry {
  function_id: string;
  name: string;
  file_path?: string | null;
  pdg_nodes: number;
  data_edges: number;
  block_count?: number;
}

export interface DataflowGraphPayload {
  view_mode: "dataflow" | "dominator";
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
  kind: "data" | "control" | "cfg" | "dominates";
  variable?: string | null;
}

export interface TaintIndexPayload {
  schema_version: number;
  available: boolean;
  detail_dir: string;
  function_count: number;
  total_flows: number;
  vulnerable_flows: number;
  functions: TaintFunctionEntry[];
}

export interface TaintFunctionEntry {
  function_id: string;
  name: string;
  file_path?: string | null;
  flow_count: number;
  vulnerable_count: number;
}

export interface TaintBundlePayload {
  schema_version: number;
  function_id: string;
  name: string;
  file_path?: string | null;
  flows: TaintFlowView[];
}

export interface TaintFlowView {
  id: number;
  variable: string;
  source_type: string;
  sink_type: string;
  severity: number;
  vulnerable: boolean;
  sanitizers: string[];
  source_line: number;
  sink_line: number;
  source_text: string;
  sink_text: string;
  path_lines: number[];
  path_statements: string[];
}

export type SliceDirection = "backward" | "forward";

export interface SliceIndexPayload {
  schema_version: number;
  available: boolean;
  function_count: number;
  functions: SliceFunctionEntry[];
}

export interface SliceFunctionEntry {
  function_id: string;
  name: string;
  file_path?: string | null;
  source_lines: number;
  pdg_nodes: number;
}

export interface SliceBundlePayload {
  schema_version: number;
  function_id: string;
  name: string;
  file_path?: string | null;
  source: string;
  total_lines: number;
  pdg: SlicePdgPayload;
}

export interface SlicePdgPayload {
  nodes: SlicePdgNode[];
  edges: SlicePdgEdge[];
}

export interface SlicePdgNode {
  id: string;
  line: number;
  label: string;
  kind: string;
  block_index?: number;
  defined: string[];
  used: string[];
}

export interface SlicePdgEdge {
  source: string;
  target: string;
  kind: string;
  variable?: string | null;
}

export interface SliceResultPayload {
  criterion: { line: number; variable: string };
  direction: SliceDirection;
  reduction_percent: number;
  lines: number[];
  nodes: SlicePdgNode[];
  edges: SlicePdgEdge[];
}

export interface CfgIndexPayload {
  schema_version: number;
  available: boolean;
  archive_path?: string | null;
  function_count: number;
  functions: CfgFunctionEntry[];
}

export interface CfgFunctionEntry {
  function_id: string;
  name: string;
  file_path?: string | null;
  block_count: number;
  cfg_edge_count: number;
}

export interface CfgDetailPayload {
  schema_version: number;
  function_id: string;
  name: string;
  file_path?: string | null;
  entry: number;
  exits: number[];
  blocks: CfgBlockView[];
  edges: CfgEdgeView[];
  idom: Array<number | null>;
  dominance_frontiers: number[][];
}

export interface CfgBlockView {
  id: number;
  label: string;
  start_line: number;
  end_line: number;
  statements: string[];
}

export interface CfgEdgeView {
  from: number;
  to: number;
  edge_type: string;
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
  community_id?: number | null;
}

export interface CommunitySummary {
  id: number;
  label: string;
  color: string;
  member_count: number;
  package_count: number;
}

export interface CommunitiesPayload {
  schema_version: number;
  modularity: number;
  communities: CommunitySummary[];
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
  community_id?: number | null;
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
  blast_score: number;
  pagerank?: number;
  betweenness?: number;
  harmonic?: number;
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
  | { type: "list_nodes"; typeMask: number; offset: number; limit: number }
  | {
      type: "compute_slice";
      functionId: string;
      line: number;
      variable: string;
      direction: SliceDirection;
    }
  | { type: "blast_radius"; nodeIndex: number; maxDepth: number }
  | {
      type: "compute_dataflow";
      functionId: string;
      variable: string | null;
      includeControl: boolean;
    };

export type WorkerIn =
  | { type: "init" }
  | { type: "expand"; requestId: number; indices: number[]; typeMask: number }
  | { type: "list_nodes"; requestId: number; typeMask: number; offset: number; limit: number }
  | {
      type: "compute_slice";
      requestId: number;
      functionId: string;
      line: number;
      variable: string;
      direction: SliceDirection;
    }
  | { type: "blast_radius"; requestId: number; nodeIndex: number; maxDepth: number }
  | {
      type: "compute_dataflow";
      requestId: number;
      functionId: string;
      variable: string | null;
      includeControl: boolean;
    };

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
  | { type: "slice_result"; requestId: number; payload: SliceResultPayload }
  | { type: "blast_result"; requestId: number; payload: BlastRadiusPayload }
  | { type: "dataflow_result"; requestId: number; payload: DataflowGraphPayload }
  | { type: "error"; requestId?: number; message: string };

export async function loadManifest(): Promise<DashboardManifest> {
  const embedded = document.getElementById("rbuilder-manifest");
  if (embedded?.textContent) {
    return JSON.parse(embedded.textContent) as DashboardManifest;
  }
  const res = await fetch(bundleDataUrl("manifest.json"));
  if (!res.ok) {
    throw new Error(`manifest.json: HTTP ${res.status}`);
  }
  return (await res.json()) as DashboardManifest;
}
