import type { MigrationPresetId, MigrationWeights } from "./types";

export interface MigrationPreset {
  id: MigrationPresetId;
  label: string;
  weights: MigrationWeights;
}

export const MIGRATION_PRESETS: MigrationPreset[] = [
  {
    id: "foundational_first",
    label: "Foundational First",
    weights: { alpha: 0.6, beta: 0.3, gamma: 0.1 },
  },
  {
    id: "dense_cluster",
    label: "Dense Cluster Extraction",
    weights: { alpha: 0.2, beta: 0.5, gamma: 0.3 },
  },
  {
    id: "risk_mitigation",
    label: "Risk Mitigation",
    weights: { alpha: 0.1, beta: 0.2, gamma: 0.7 },
  },
  {
    id: "hybrid_default",
    label: "Hybrid Default",
    weights: { alpha: 0.33, beta: 0.33, gamma: 0.34 },
  },
];

export function presetById(id: MigrationPresetId): MigrationPreset {
  return MIGRATION_PRESETS.find((p) => p.id === id) ?? MIGRATION_PRESETS[3];
}

export function matchPreset(weights: MigrationWeights): MigrationPresetId {
  for (const preset of MIGRATION_PRESETS) {
    const w = preset.weights;
    if (
      nearlyEqual(weights.alpha, w.alpha) &&
      nearlyEqual(weights.beta, w.beta) &&
      nearlyEqual(weights.gamma, w.gamma)
    ) {
      return preset.id;
    }
  }
  return "custom";
}

function nearlyEqual(a: number, b: number): boolean {
  return Math.abs(a - b) < 0.001;
}
