import type { MigrationCommunityNode } from "./types";

/** Pure score sort: highest priority first; tie-break lowest community id. */
export function priorityRankOrder(
  communities: MigrationCommunityNode[],
  scores: Map<number, number>,
): number[] {
  return [...communities]
    .map((c) => c.id)
    .sort((a, b) => {
      const sa = scores.get(a) ?? 0;
      const sb = scores.get(b) ?? 0;
      if (sb !== sa) return sb - sa;
      return a - b;
    });
}
