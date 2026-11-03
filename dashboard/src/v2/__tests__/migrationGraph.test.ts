import { describe, expect, it } from "vitest";
import { computeMigrationPlan } from "../../migration/engine";
import { MIGRATION_PRESETS } from "../../migration/presets";
import {
  classifyMigrationGraph,
  parseMigrationGraph,
} from "../data/loadMigrationGraph";
import { drawModelChannels, layoutMigration, MAX_LAYOUT_EDGES } from "../migrationLayout";
import fixture from "./fixtures/migration_graph.json";

describe("classifyMigrationGraph", () => {
  it("treats missing / invalid as missing (not empty)", () => {
    expect(classifyMigrationGraph(null).kind).toBe("missing");
    expect(classifyMigrationGraph({}).kind).toBe("missing");
    expect(classifyMigrationGraph({ schema_version: 2, mode: "package_macro" }).kind).toBe("missing");
  });

  it("distinguishes empty communities from missing file", () => {
    const empty = {
      schema_version: 3,
      mode: "package_macro",
      modularity: 0,
      communities: [],
      edges: [],
    };
    const result = classifyMigrationGraph(empty);
    expect(result.kind).toBe("empty");
    if (result.kind === "empty") {
      expect(result.graph.communities).toHaveLength(0);
    }
  });

  it("accepts a real exported migration_graph.json shape", () => {
    const result = classifyMigrationGraph(fixture);
    expect(result.kind).toBe("ready");
    const parsed = parseMigrationGraph(fixture);
    expect(parsed?.schema_version).toBe(3);
    expect(parsed?.mode).toBe("package_macro");
    expect(parsed!.communities.length).toBeGreaterThan(0);
  });
});

describe("computeMigrationPlan weights", () => {
  it("changes plan weights when presets change", () => {
    const graph = parseMigrationGraph(fixture)!;
    const hybrid = computeMigrationPlan(
      graph,
      "hybrid_default",
      MIGRATION_PRESETS[3].weights,
      "priority",
    );
    const foundational = computeMigrationPlan(
      graph,
      "foundational_first",
      MIGRATION_PRESETS[0].weights,
      "priority",
    );
    expect(hybrid.steps).toHaveLength(graph.communities.length);
    expect(foundational.steps).toHaveLength(graph.communities.length);
    expect(hybrid.weights).not.toEqual(foundational.weights);
    expect(hybrid.preset).toBe("hybrid_default");
    expect(foundational.preset).toBe("foundational_first");
  });
});

describe("layoutMigration (no WebGL)", () => {
  it("builds positions for every community", () => {
    const graph = parseMigrationGraph(fixture)!;
    const plan = computeMigrationPlan(
      graph,
      "hybrid_default",
      MIGRATION_PRESETS[3].weights,
      "scheduled",
    );
    const layout = layoutMigration(plan, graph.communities, graph.edges);
    expect(layout.nodes).toHaveLength(graph.communities.length);
    for (const n of layout.nodes) {
      expect(n.position).toHaveLength(3);
      expect(Number.isFinite(n.position[0])).toBe(true);
    }
    const channels = drawModelChannels(layout);
    expect(channels.sizes).toHaveLength(layout.nodes.length);
    expect(channels.hues.every((h) => h >= 0 && h < 360)).toBe(true);
    // Sunflower disk: extent ~ O(sqrt(N)), never one ring per schedule step (~ O(N)).
    expect(channels.extent).toBeLessThan(40 * Math.sqrt(Math.max(layout.nodes.length, 1)));
  });

  it("keeps metasfresh-scale N in a compact disk (not N rings)", () => {
    const communities = Array.from({ length: 256 }, (_, id) => ({
      id,
      label: `c${id}`,
      member_count: 10 + (id % 20),
      avg_pagerank: 0.01 + (id % 50) / 500,
      avg_harmonic: 0.01,
      avg_betweenness: 0,
      max_blast: id % 10,
    }));
    const graph = {
      schema_version: 3,
      mode: "package_macro" as const,
      modularity: 0.4,
      communities,
      edges: [{ source: 0, target: 1, weight: 2, kind: "calls" }],
    };
    const plan = computeMigrationPlan(graph, "hybrid_default", MIGRATION_PRESETS[3].weights, "scheduled");
    const layout = layoutMigration(plan, graph.communities, graph.edges);
    const { extent } = drawModelChannels(layout);
    expect(layout.nodes).toHaveLength(256);
    // Old bug: ringR = 28 * schedule_step → extent ~ 7000. Disk must stay under ~250.
    expect(extent).toBeLessThan(250);
  });

  it("keeps coupling edges bounded so the canvas stays readable", () => {
    const communities = Array.from({ length: 40 }, (_, id) => ({
      id,
      label: `c${id}`,
      member_count: 5,
      avg_pagerank: 0.1,
      avg_harmonic: 0.1,
      avg_betweenness: 0,
      max_blast: 1,
    }));
    const edges = [];
    for (let i = 0; i < 40; i++) {
      for (let j = i + 1; j < 40; j++) {
        edges.push({ source: i, target: j, weight: (i + j) % 17, kind: "calls" });
      }
    }
    const graph = {
      schema_version: 3,
      mode: "package_macro" as const,
      modularity: 0.4,
      communities,
      edges,
    };
    const plan = computeMigrationPlan(graph, "hybrid_default", MIGRATION_PRESETS[3].weights, "scheduled");
    const layout = layoutMigration(plan, graph.communities, graph.edges);
    expect(edges.length).toBeGreaterThan(MAX_LAYOUT_EDGES);
    expect(layout.edges.length).toBeLessThanOrEqual(MAX_LAYOUT_EDGES);
  });
});

describe("MigrationScene reduced motion", () => {
  it("resolveReducedMotion honors explicit opts and prefers-reduced-motion", async () => {
    const { resolveReducedMotion } = await import("../universe/MigrationScene");
    expect(resolveReducedMotion({ reducedMotion: true })).toBe(true);
    expect(resolveReducedMotion({ reducedMotion: false })).toBe(false);
  });
});
