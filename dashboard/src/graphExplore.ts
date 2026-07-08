import type { Metanode } from "./types";

export type CategoryFilter = "all" | "functions" | "classes" | "both";

export interface GraphFilterState {
  search: string;
  communityId: number | null;
  category: CategoryFilter;
  soloCommunity: boolean;
}

export function passesCategory(node: Metanode, category: CategoryFilter): boolean {
  switch (category) {
    case "functions":
      return node.functions > 0 && node.classes === 0;
    case "classes":
      return node.classes > 0 && node.functions === 0;
    case "both":
      return node.functions > 0 && node.classes > 0;
    default:
      return true;
  }
}

export function passesSearch(node: Metanode, query: string): boolean {
  const q = query.trim().toLowerCase();
  if (!q) return true;
  return node.label.toLowerCase().includes(q);
}

export function passesCommunity(node: Metanode, communityId: number | null): boolean {
  if (communityId === null) return true;
  return node.community_id === communityId;
}

export function passesFilters(node: Metanode, filters: GraphFilterState): boolean {
  if (!passesSearch(node, filters.search)) return false;
  if (!passesCommunity(node, filters.communityId)) return false;
  if (!passesCategory(node, filters.category)) return false;
  if (filters.soloCommunity && filters.communityId !== null) {
    return node.community_id === filters.communityId;
  }
  return true;
}

export function buildUndirectedAdjacency(
  edges: Array<{ source: string; target: string }>,
): Map<string, Set<string>> {
  const adj = new Map<string, Set<string>>();
  const touch = (id: string) => {
    if (!adj.has(id)) adj.set(id, new Set());
  };
  for (const e of edges) {
    touch(e.source);
    touch(e.target);
    adj.get(e.source)!.add(e.target);
    adj.get(e.target)!.add(e.source);
  }
  return adj;
}

export function neighborhoodIds(
  focusId: string | null,
  adjacency: Map<string, Set<string>>,
): Set<string> {
  if (!focusId) return new Set();
  const hood = new Set<string>([focusId]);
  for (const nb of adjacency.get(focusId) ?? []) hood.add(nb);
  return hood;
}

/** Deterministic spiral layout seed (no Math.random). */
export function deterministicPositions(
  count: number,
  seed = 0,
): Array<{ x: number; y: number }> {
  const golden = 2.399963229728653;
  const out: Array<{ x: number; y: number }> = [];
  for (let i = 0; i < count; i++) {
    const angle = i * golden + seed * 0.17;
    const r = 6 + Math.sqrt(i + 1) * 2.8;
    out.push({ x: Math.cos(angle) * r, y: Math.sin(angle) * r });
  }
  return out;
}

export function firstMatchingNodeId(
  nodes: Metanode[],
  filters: GraphFilterState,
): string | null {
  const q = filters.search.trim().toLowerCase();
  if (!q) return null;
  const hit = nodes.find(
    (n) =>
      passesCommunity(n, filters.communityId) &&
      passesCategory(n, filters.category) &&
      n.label.toLowerCase().includes(q),
  );
  return hit ? String(hit.id) : null;
}
