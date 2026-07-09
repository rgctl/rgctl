export interface MigrationWeights {
  alpha: number;
  beta: number;
  gamma: number;
}

export type MigrationOrderMode = "scheduled" | "priority";

export interface MigrationCommunityNode {
  id: number;
  label: string;
  member_count: number;
  avg_pagerank: number;
  avg_harmonic: number;
  avg_betweenness: number;
  max_blast: number;
  louvain_community_id?: number | null;
}

export interface MigrationCommunityEdge {
  source: number;
  target: number;
  weight: number;
  kind: string;
}

export interface MigrationGraphPayload {
  schema_version: number;
  mode?: string;
  modularity: number;
  communities: MigrationCommunityNode[];
  edges: MigrationCommunityEdge[];
}

export interface MigrationPlanStep {
  step: number;
  community_id: number;
  label: string;
  priority_score: number;
  schedule_step: number;
  priority_rank: number;
  avg_pagerank: number;
  avg_harmonic: number;
  max_blast: number;
}

export interface MigrationPlanPayload {
  schema_version: number;
  preset: string;
  preset_label: string;
  weights: MigrationWeights;
  order_mode: MigrationOrderMode;
  steps: MigrationPlanStep[];
}

export type MigrationPresetId =
  | "hybrid_default"
  | "foundational_first"
  | "dense_cluster"
  | "risk_mitigation"
  | "custom";
