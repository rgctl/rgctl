import type { MigrationCommunityNode, MigrationWeights } from "./types";

export function normalizeValues(values: number[]): number[] {
  if (values.length === 0) return values;
  const min = Math.min(...values);
  const max = Math.max(...values);
  if (Math.abs(max - min) < Number.EPSILON) {
    return values.map(() => 0.5);
  }
  return values.map((v) => (v - min) / (max - min));
}

export function communityPriorityScores(
  communities: MigrationCommunityNode[],
  weights: MigrationWeights,
): Map<number, number> {
  const normPr = normalizeValues(communities.map((c) => c.avg_pagerank));
  const normHm = normalizeValues(communities.map((c) => c.avg_harmonic));
  const normBl = normalizeValues(communities.map((c) => c.max_blast));

  const scores = new Map<number, number>();
  communities.forEach((node, i) => {
    const score =
      weights.alpha * normPr[i] + weights.beta * normHm[i] - weights.gamma * normBl[i];
    scores.set(node.id, score);
  });
  return scores;
}
