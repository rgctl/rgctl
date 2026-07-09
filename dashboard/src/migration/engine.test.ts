import { computeMigrationPlan } from "./engine";
import { MIGRATION_PRESETS, matchPreset } from "./presets";
import { communityPriorityScores, normalizeValues } from "./scoring";
import type { MigrationCommunityNode, MigrationGraphPayload } from "./types";
import { describe, expect, it } from "vitest";

describe("normalizeValues", () => {
  it("returns 0.5 when all values equal", () => {
    expect(normalizeValues([2, 2, 2])).toEqual([0.5, 0.5, 0.5]);
  });

  it("min-max normalizes range", () => {
    expect(normalizeValues([0, 5, 10])).toEqual([0, 0.5, 1]);
  });
});

describe("communityPriorityScores", () => {
  const communities: MigrationCommunityNode[] = [
    {
      id: 0,
      label: "low",
      member_count: 1,
      avg_pagerank: 0.1,
      avg_harmonic: 0.1,
      avg_betweenness: 0,
      max_blast: 10,
    },
    {
      id: 1,
      label: "high",
      member_count: 1,
      avg_pagerank: 0.9,
      avg_harmonic: 0.1,
      avg_betweenness: 0,
      max_blast: 10,
    },
  ];

  it("prefers high pagerank under foundational preset", () => {
    const scores = communityPriorityScores(communities, {
      alpha: 0.6,
      beta: 0.3,
      gamma: 0.1,
    });
    expect(scores.get(1)! > scores.get(0)!).toBe(true);
  });
});

describe("computeMigrationPlan", () => {
  const graph: MigrationGraphPayload = {
    schema_version: 2,
    mode: "package_macro",
    modularity: 0.5,
    communities: [
      {
        id: 0,
        label: "caller",
        member_count: 1,
        avg_pagerank: 0.5,
        avg_harmonic: 0.5,
        avg_betweenness: 0,
        max_blast: 0,
      },
      {
        id: 1,
        label: "callee",
        member_count: 2,
        avg_pagerank: 0.5,
        avg_harmonic: 0.5,
        avg_betweenness: 0,
        max_blast: 0,
      },
    ],
    edges: [{ source: 0, target: 1, weight: 1, kind: "calls" }],
  };

  it("scheduled mode puts callee first", () => {
    const plan = computeMigrationPlan(graph, "hybrid_default", MIGRATION_PRESETS[3].weights, "scheduled");
    expect(plan.steps[0].community_id).toBe(1);
    expect(plan.steps[0].schedule_step).toBe(1);
    expect(plan.order_mode).toBe("scheduled");
  });

  it("exposes both schedule_step and priority_rank on every row", () => {
    const plan = computeMigrationPlan(graph, "hybrid_default", MIGRATION_PRESETS[3].weights, "priority");
    expect(plan.steps.every((s) => s.schedule_step >= 1 && s.priority_rank >= 1)).toBe(true);
    expect(plan.order_mode).toBe("priority");
  });

  it("detects custom preset via matchPreset", () => {
    const custom = { alpha: 0.11, beta: 0.22, gamma: 0.67 };
    expect(matchPreset(custom)).toBe("custom");
    expect(matchPreset(MIGRATION_PRESETS[0].weights)).toBe("foundational_first");
  });
});
