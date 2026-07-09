import { presetById } from "./presets";
import { priorityRankOrder } from "./priorityRank";
import { communityPriorityScores } from "./scoring";
import { topologicalSchedule } from "./topologicalSort";
import type {
  MigrationGraphPayload,
  MigrationOrderMode,
  MigrationPlanPayload,
  MigrationPlanStep,
  MigrationPresetId,
  MigrationWeights,
} from "./types";

export function computeMigrationPlan(
  graph: MigrationGraphPayload,
  preset: MigrationPresetId,
  weights: MigrationWeights,
  orderMode: MigrationOrderMode,
): MigrationPlanPayload {
  const scores = communityPriorityScores(graph.communities, weights);
  const scheduleOrder = topologicalSchedule(graph.communities, graph.edges, scores);
  const priorityOrder = priorityRankOrder(graph.communities, scores);

  const scheduleRank = new Map<number, number>();
  scheduleOrder.forEach((id, idx) => scheduleRank.set(id, idx + 1));
  const priorityRank = new Map<number, number>();
  priorityOrder.forEach((id, idx) => priorityRank.set(id, idx + 1));

  const rows: MigrationPlanStep[] = graph.communities.map((node) => ({
    step: 0,
    community_id: node.id,
    label: node.label,
    priority_score: scores.get(node.id) ?? 0,
    schedule_step: scheduleRank.get(node.id) ?? 0,
    priority_rank: priorityRank.get(node.id) ?? 0,
    avg_pagerank: node.avg_pagerank,
    avg_harmonic: node.avg_harmonic,
    max_blast: node.max_blast,
  }));

  sortSteps(rows, orderMode);
  rows.forEach((row, idx) => {
    row.step = idx + 1;
  });

  const presetMeta = preset === "custom" ? presetById("hybrid_default") : presetById(preset);

  return {
    schema_version: 2,
    preset,
    preset_label: preset === "custom" ? "Custom" : presetMeta.label,
    weights,
    order_mode: orderMode,
    steps: rows,
  };
}

function sortSteps(steps: MigrationPlanStep[], orderMode: MigrationOrderMode): void {
  steps.sort((a, b) => {
    if (orderMode === "scheduled") {
      if (a.schedule_step !== b.schedule_step) return a.schedule_step - b.schedule_step;
    } else {
      if (a.priority_rank !== b.priority_rank) return a.priority_rank - b.priority_rank;
    }
    return a.community_id - b.community_id;
  });
}
