import type { MigrationCommunityEdge, MigrationCommunityNode } from "./types";

interface ScheduleNode {
  id: number;
  score: number;
}

/** Callee communities schedule before callers; cycles broken by highest score. */
export function topologicalSchedule(
  communities: MigrationCommunityNode[],
  edges: MigrationCommunityEdge[],
  scores: Map<number, number>,
): number[] {
  const ids = new Set(communities.map((c) => c.id));
  const inDegree = new Map<number, number>();
  const outgoing = new Map<number, number[]>();

  for (const id of ids) {
    inDegree.set(id, 0);
  }

  for (const edge of edges) {
    if (!ids.has(edge.source) || !ids.has(edge.target)) continue;
    const schedFrom = edge.target;
    const schedTo = edge.source;
    if (schedFrom === schedTo) continue;
    if (!outgoing.has(schedFrom)) outgoing.set(schedFrom, []);
    outgoing.get(schedFrom)!.push(schedTo);
    inDegree.set(schedTo, (inDegree.get(schedTo) ?? 0) + 1);
  }

  const ready: ScheduleNode[] = [];
  for (const id of ids) {
    if ((inDegree.get(id) ?? 0) === 0) {
      ready.push({ id, score: scores.get(id) ?? 0 });
    }
  }
  ready.sort(compareScheduleNodes);

  const order: number[] = [];
  const scheduled = new Set<number>();

  while (ready.length > 0) {
    ready.sort(compareScheduleNodes);
    const node = ready.pop()!;
    if (scheduled.has(node.id)) continue;
    scheduled.add(node.id);
    order.push(node.id);

    for (const next of outgoing.get(node.id) ?? []) {
      const deg = (inDegree.get(next) ?? 0) - 1;
      inDegree.set(next, deg);
      if (deg === 0 && !scheduled.has(next)) {
        ready.push({ id: next, score: scores.get(next) ?? 0 });
      }
    }
  }

  const remaining = [...ids]
    .filter((id) => !scheduled.has(id))
    .map((id) => ({ id, score: scores.get(id) ?? 0 }))
    .sort(compareScheduleNodes);

  for (const node of remaining) {
    if (!scheduled.has(node.id)) {
      scheduled.add(node.id);
      order.push(node.id);
    }
  }

  return order;
}

function compareScheduleNodes(a: ScheduleNode, b: ScheduleNode): number {
  if (b.score !== a.score) return b.score - a.score;
  return a.id - b.id;
}
