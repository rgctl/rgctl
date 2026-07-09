import type { MigrationCommunityNode } from "./types";

/** ForceAtlas2 attraction multiplier for edges within the same Louvain cluster. */
export const MIGRATION_INTRA_CLUSTER_LAYOUT_WEIGHT = 4.0;
/** ForceAtlas2 attraction multiplier for cross-cluster call edges. */
export const MIGRATION_INTER_CLUSTER_LAYOUT_WEIGHT = 0.35;

export function migrationEdgeLayoutWeight(
  source: MigrationCommunityNode | undefined,
  target: MigrationCommunityNode | undefined,
  callWeight: number,
): number {
  const sameCluster =
    source?.louvain_community_id != null &&
    source.louvain_community_id === target?.louvain_community_id;
  const base = Math.max(0.5, callWeight);
  return sameCluster
    ? base * MIGRATION_INTRA_CLUSTER_LAYOUT_WEIGHT
    : base * MIGRATION_INTER_CLUSTER_LAYOUT_WEIGHT;
}

/** Hidden layout edges pull same-cluster packages together when they lack direct calls. */
export function addLouvainLayoutEdges(
  graph: import("graphology").default,
  communities: MigrationCommunityNode[],
): void {
  const byCluster = new Map<number, number[]>();
  for (const node of communities) {
    if (node.louvain_community_id == null) continue;
    const list = byCluster.get(node.louvain_community_id) ?? [];
    list.push(node.id);
    byCluster.set(node.louvain_community_id, list);
  }

  for (const ids of byCluster.values()) {
    if (ids.length < 2) continue;
    for (let i = 0; i < ids.length - 1; i += 1) {
      const a = String(ids[i]);
      const b = String(ids[i + 1]);
      const key = `layout:${a}<->${b}`;
      if (graph.hasEdge(key)) continue;
      if (!graph.hasNode(a) || !graph.hasNode(b)) continue;
      graph.addUndirectedEdgeWithKey(key, a, b, {
        weight: MIGRATION_INTRA_CLUSTER_LAYOUT_WEIGHT,
        size: 0,
        color: "transparent",
        layoutOnly: true,
      });
    }
  }
}
