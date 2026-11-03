/**
 * Load migration_graph + recompute plan when weights / preset / order mode change.
 */
import { useCallback, useEffect, useMemo, useState } from "react";
import { computeMigrationPlan } from "../../migration/engine";
import { matchPreset, MIGRATION_PRESETS, presetById } from "../../migration/presets";
import type {
  MigrationGraphPayload,
  MigrationOrderMode,
  MigrationPlanPayload,
  MigrationPresetId,
  MigrationWeights,
} from "../../migration/types";
import {
  loadMigrationGraph,
  type MigrationGraphLoad,
} from "../data/loadMigrationGraph";
import { layoutMigration, type MigrationLayout } from "../migrationLayout";

export type MigrationRoomState =
  | { kind: "loading" }
  | { kind: "missing" }
  | { kind: "empty"; graph: MigrationGraphPayload }
  | {
      kind: "ready";
      graph: MigrationGraphPayload;
      plan: MigrationPlanPayload;
      layout: MigrationLayout;
    };

export interface UseMigrationSchedule {
  state: MigrationRoomState;
  weights: MigrationWeights;
  presetId: MigrationPresetId;
  orderMode: MigrationOrderMode;
  setWeights: (w: MigrationWeights) => void;
  applyPreset: (id: MigrationPresetId) => void;
  setOrderMode: (m: MigrationOrderMode) => void;
}

const DEFAULT_WEIGHTS: MigrationWeights = { alpha: 0.33, beta: 0.33, gamma: 0.34 };

export function useMigrationSchedule(): UseMigrationSchedule {
  const [load, setLoad] = useState<MigrationGraphLoad | null>(null);
  const [weights, setWeights] = useState<MigrationWeights>(DEFAULT_WEIGHTS);
  const [orderMode, setOrderMode] = useState<MigrationOrderMode>("scheduled");

  useEffect(() => {
    let cancelled = false;
    setLoad(null);
    void loadMigrationGraph().then((next) => {
      if (!cancelled) setLoad(next);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const presetId = useMemo(() => matchPreset(weights), [weights]);

  const state = useMemo((): MigrationRoomState => {
    if (load === null) return { kind: "loading" };
    if (load.kind === "missing") return { kind: "missing" };
    if (load.kind === "empty") return { kind: "empty", graph: load.graph };
    const plan = computeMigrationPlan(load.graph, presetId, weights, orderMode);
    const layout = layoutMigration(plan, load.graph.communities, load.graph.edges);
    return { kind: "ready", graph: load.graph, plan, layout };
  }, [load, weights, presetId, orderMode]);

  const applyPreset = useCallback((id: MigrationPresetId) => {
    if (id === "custom") return;
    const preset = presetById(id);
    setWeights({ ...preset.weights });
  }, []);

  return {
    state,
    weights,
    presetId,
    orderMode,
    setWeights,
    applyPreset,
    setOrderMode,
  };
}

export { MIGRATION_PRESETS, DEFAULT_WEIGHTS };
