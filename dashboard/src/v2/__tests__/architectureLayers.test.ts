/**
 * The layer model, as arithmetic.
 *
 * Everything the Architecture room derives is here, so the claims can be checked without a browser: `jsdom`
 * has no WebGL, and a canvas that cannot be tested is a canvas whose numbers nobody checks. The fixture is
 * small and hand-built *on purpose* — it is a unit test of the derivations. The real-corpus numbers
 * (10 volumes / 232 packages, 214 communities, 43.1% coverage) belong to the captured-bundle test, because a
 * hand-written fixture encodes the author's belief about the shape and the shape is the question.
 */
import { describe, expect, it } from "vitest";
import {
  aggregateCommunities, clampFootprints, communityLabel, couplingOf, coverageOf,
  communityHueIndex, footprintScale, groupNamespaces, heightScale, hueFor, layoutVolumes, namespaceOf, sunflower, toPackages,
} from "../architectureLayers";
import { buildView, channelReport, scalesFor } from "../architecture";
import type { CommunitySummary, MetaEdge, MetaNode, Metagraph } from "../architectureTypes";

function node(id: number, label: string, size: number, functions: number, communityId: number): MetaNode {
  return { id, label, size, functions, classes: 0, avg_complexity: 0, x: 0, y: 0, community_id: communityId };
}

/**
 * Two namespaces that group (org.compiere x2, de.metas x2), one package that is its own namespace, one
 * path-heuristic bucket (`root`), one absolute-path artifact, and the producer's remainder bucket.
 */
const NODES: MetaNode[] = [
  node(0, "main.java-gen.org.compiere.model", 4000, 900, 100),   // namespace org.compiere
  node(1, "main.java-legacy.org.compiere.model", 900, 200, 100), // same namespace, same community → shared
  node(2, "de.metas.util", 2500, 500, 200),                      // namespace de.metas
  node(3, "de.metas.handlingunits.impl", 800, 150, 200),         // same namespace → shared community
  node(4, "io.swagger.client.model", 300, 60, 300),              // its own namespace, alone
  node(5, "root", 2100, 0, 500),                                 // path-heuristic artifact
  node(6, ".Users.sami.code.repo.e2e.spec", 120, 30, 600),       // absolute-path artifact
  node(7, "(other)", 13045, 4000, 400),                          // the remainder bucket
];

const EDGES: MetaEdge[] = [
  { source: 0, target: 2, weight: 500, kind: "calls" },          // org.compiere ↔ de.metas
  { source: 0, target: 1, weight: 40, kind: "references" },      // same community → not a pair
  { source: 2, target: 3, weight: 30, kind: "uses" },            // same community
  { source: 0, target: 4, weight: 20, kind: "calls" },           // org.compiere ↔ io.swagger
  { source: 4, target: 2, weight: 10, kind: "calls" },           // io.swagger ↔ de.metas
  { source: 0, target: 5, weight: 600, kind: "references" },     // touches `root` → artifact edge
  { source: 5, target: 7, weight: 900, kind: "references" },     // touches `root` and `(other)`
];

const METAGRAPH: Metagraph = {
  schema_version: 3,
  mode: "community_only",
  community_only: true,
  threshold_community_only: 50000,
  source_node_count: 40000,
  nodes: NODES,
  edges: EDGES,
};

const SUMMARY: CommunitySummary = {
  schema_version: 1,
  modularity: 0.386,
  communities: [
    { id: 100, label: "I_C_InvoiceLine", color: "#85cc3e", member_count: 5200, package_count: 2 },
    { id: 200, label: "Mutable", color: "#cc3ea1", member_count: 3300, package_count: 2 },
    { id: 300, label: "io.swagger", color: "#85cc3e", member_count: 2420, package_count: 2 },
    { id: 400, label: "(other)", color: "#333333", member_count: 13045, package_count: 1 },
    { id: 500, label: "root", color: "#444444", member_count: 2100, package_count: 0 },
    { id: 600, label: ".Users", color: "#555555", member_count: 120, package_count: 0 },
  ],
};

describe("the namespace rule", () => {
  it("groups a label past its build-layout tokens", () => {
    expect(namespaceOf("main.java-gen.org.compiere.model")).toEqual({ key: "org.compiere", kind: "package" });
    expect(namespaceOf("de.metas.handlingunits.impl")).toEqual({ key: "de.metas", kind: "package" });
    expect(namespaceOf("io.swagger.client.model")).toEqual({ key: "io.swagger", kind: "package" });
  });

  it("does not mistake a build layout for a namespace", () => {
    // without the token list this would be the `main` namespace, holding two thirds of a real corpus
    expect(namespaceOf("main.java-gen.org.compiere.model").key).not.toBe("main");
  });

  it("reports the producer's two fallbacks as what they are", () => {
    expect(namespaceOf("(other)").kind).toBe("remainder");
    expect(namespaceOf("root").kind).toBe("artifact");
    expect(namespaceOf(".Users.sami.code.repo.e2e.spec").kind).toBe("artifact");
  });
});

describe("the three objects", () => {
  const packages = toPackages(METAGRAPH, SUMMARY);

  it("makes a package of every metagraph node and marks community sharing", () => {
    const byLabel = new Map(packages.map((p) => [p.label, p]));
    expect(byLabel.get("de.metas.util")?.sharesCommunity).toBe(true);
    expect(byLabel.get("io.swagger.client.model")?.sharesCommunity).toBe(false); // alone in community 300
  });

  it("wraps only the namespaces that actually group something", () => {
    const { volumes, standalone } = groupNamespaces(packages);
    expect(volumes.map((v) => v.key)).toEqual(["org.compiere", "de.metas"]);
    expect(volumes[0].packages).toHaveLength(2);
    // its own namespace → drawn, never wrapped
    expect(standalone.map((p) => p.label)).toEqual(["io.swagger.client.model"]);
    // and the buckets are in neither: they are not packages
    const inVolumes = volumes.flatMap((v) => v.packages.map((p) => p.label));
    expect(inVolumes).not.toContain("root");
    expect(inVolumes).not.toContain("(other)");
  });

  it("sizes a volume by its members, so the region means something", () => {
    const { volumes } = groupNamespaces(packages);
    expect(volumes[0]).toMatchObject({ key: "org.compiere", members: 4900, functions: 1100 });
  });
});

describe("communities are an aggregation, not a container", () => {
  const packages = toPackages(METAGRAPH, SUMMARY);
  const groups = aggregateCommunities(packages.filter((p) => p.kind === "package"), SUMMARY);

  it("re-partitions the same members, so the sums agree", () => {
    const byCommunity = groups.reduce((n, g) => n + g.members, 0);
    const byPackage = packages.filter((p) => p.kind === "package").reduce((n, p) => n + p.size, 0);
    expect(byCommunity).toBe(byPackage);
  });

  it("marks which communities actually hold more than one package", () => {
    const shared = groups.filter((g) => g.shared).map((g) => g.id);
    expect(shared).toEqual([100, 200]);
  });

  it("keeps the producer's label verbatim, including the ones that name nothing", () => {
    expect(communityLabel(SUMMARY, 200)).toBe("Mutable");
    expect(communityLabel(SUMMARY, 999)).toBe("999");
  });
});

describe("coverage comes from whichever evidence exists", () => {
  it("uses per-node membership when the mode provides it", () => {
    const withIndices: Metagraph = {
      ...METAGRAPH,
      nodes: NODES.map((n) => ({ ...n, member_indices: [n.id] })),
    };
    expect(coverageOf(withIndices, SUMMARY)).toEqual({
      covered: NODES.length,
      declared: 40000,
      uncovered: 40000 - NODES.length,
      excludedGenerated: null,
      excludedKind: null,
      source: "member_indices",
    });
  });

  it("falls back to the summary's member counts", () => {
    expect(coverageOf(METAGRAPH, SUMMARY)).toEqual({
      covered: 26185,
      declared: 40000,
      uncovered: 13815,
      excludedGenerated: null,
      excludedKind: null,
      source: "member_count",
    });
  });

  it("is unknown — never zero — when the bundle carries neither", () => {
    const bare: CommunitySummary = { communities: [{ id: 1, label: "x" }] };
    expect(coverageOf(METAGRAPH, bare)).toBeNull();
    expect(coverageOf(METAGRAPH, null)).toBeNull();
  });
});

describe("coupling", () => {
  it("counts the community pairs the package edges form, and the edges it must not draw", () => {
    const packages = toPackages(METAGRAPH, SUMMARY);
    const { communityPairs, artifactEdges } = couplingOf(packages, EDGES);
    // 100|200, 100|300, 200|300 — same-community edges are not pairs
    expect(communityPairs).toBe(3);
    // the two edges touching `root` and `(other)` are named, not drawn
    expect(artifactEdges).toBe(2);
  });
});

describe("scales", () => {
  it("puts height on a log scale, because the distribution is skewed by orders of magnitude", () => {
    const height = heightScale([100, 200, 13045]);
    expect(height(200)).toBeGreaterThan(height(100));
    expect(height(13045)).toBeGreaterThan(height(200));
    // log: equal decades must add equal height. On a linear scale the third decade would be 9x taller.
    const firstDecade = height(1000) - height(100);
    const secondDecade = height(10000) - height(1000);
    expect(secondDecade).toBeCloseTo(firstDecade, 5);
    expect(firstDecade).toBeGreaterThan(1);
  });

  it("compresses footprint and never exceeds the cap", () => {
    const footprint = footprintScale([10, 60, 900, 85098]);
    expect(footprint(85098)).toBeLessThanOrEqual(34);
    expect(footprint(900)).toBeGreaterThanOrEqual(footprint(60));
  });

  it("clamps footprints to local spacing and reports how many it bound", () => {
    const items = [
      { side: 20, pos: [0, 0, 0] as [number, number, number] },
      { side: 20, pos: [7, 0, 0] as [number, number, number] },
      { side: 20, pos: [400, 0, 0] as [number, number, number] },
    ];
    const report = clampFootprints(items);
    expect(report.bound).toBe(2);
    expect((items[0] as { foot?: number }).foot).toBeLessThan(20);
    expect((items[2] as { foot?: number }).foot).toBe(20);
  });

  it("gives every rank its own hue, and the biggest structures the most separated ones", () => {
    const hues = Array.from({ length: 24 }, (_, i) => hueFor(i));
    expect(new Set(hues.map((h) => Math.round(h))).size).toBe(24);
  });

  it("folds community ids through the same hue index the universe layers share", () => {
    expect(communityHueIndex(97)).toBe(0);
    expect(hueFor(communityHueIndex(3))).toBe(hueFor(3));
  });
});

describe("layout", () => {
  const packages = toPackages(METAGRAPH, SUMMARY);
  const { volumes } = groupNamespaces(packages);

  it("is deterministic and produces finite coordinates", () => {
    const seatRadius = (n: number) => 20 * Math.sqrt(n);
    const a = layoutVolumes(volumes, 1, { seatRadius, pad: 16 });
    const b = layoutVolumes(volumes, 1, { seatRadius, pad: 16 });
    expect(a).toEqual(b);
    for (const v of a.volumes) {
      expect(Number.isFinite(v.centre[0])).toBe(true);
      expect(Number.isFinite(v.centre[1])).toBe(true);
      expect(v.radius).toBeGreaterThan(0);
    }
  });

  it("keeps the ring wide enough for the largest volume to fit inside its own arc", () => {
    const seatRadius = (n: number) => 40 * Math.sqrt(n);
    const layout = layoutVolumes(volumes, 0, { seatRadius, pad: 20 });
    const largest = Math.max(...layout.volumes.map((v) => v.radius));
    expect(layout.coreRadius).toBeGreaterThanOrEqual(largest);
  });

  it("seats packages inside their volume rather than on the ring", () => {
    const seat = sunflower([100, 100], 30, 0, 4);
    expect(Math.hypot(seat[0] - 100, seat[2] - 100)).toBeLessThanOrEqual(30);
  });
});

describe("the composed view", () => {
  const view = buildView(METAGRAPH, SUMMARY, { mode: "package_macro", communities: new Array(6) });

  it("reports the three counts with their sources, choosing none", () => {
    expect(view.disagreement).toEqual({ packages: 8, communities: 6, migration: 6 });
  });

  it("separates the packages from the buckets", () => {
    expect(view.packages).toHaveLength(5);
    expect(view.artifacts.map((p) => p.label)).toEqual(["root", ".Users.sami.code.repo.e2e.spec"]);
    expect(view.remainder?.label).toBe("(other)");
  });

  it("states the channel report the room has to display", () => {
    const report = channelReport(view);
    expect(report.heights).toBe("log");
    expect(report.packagesInVolumes).toBe(4);
    expect(report.singletonNamespaces).toBe(1);
    expect(report.skew).toBeGreaterThan(1);
  });

  it("builds scales over packages and buckets together, so nothing is left unmeasured", () => {
    const { height, footprint } = scalesFor(view);
    expect(height(view.remainder?.size ?? 0)).toBeGreaterThan(0);
    expect(footprint(0)).toBeGreaterThan(0);
  });
});

/**
 * The two reasons a node is missing, and the case where the bundle does not say.
 *
 * One total hid both: "this repository ships compiled bundles" and "an import is not a place in the architecture"
 * are different decisions, and the room can only report them if the producer counts them where it makes the
 * decision. A bundle written before that counting must say *nothing* rather than implying zero.
 */
describe("coverage names its exclusions", () => {
  it("carries the producer's two counts through", () => {
    const withBreakdown = { ...METAGRAPH, excluded_generated: 11240, excluded_kind: 37460 };
    expect(coverageOf(withBreakdown, SUMMARY)).toMatchObject({
      excludedGenerated: 11240,
      excludedKind: 37460,
    });
  });

  it("distinguishes 'not stated' from zero", () => {
    expect(coverageOf({ ...METAGRAPH, excluded_generated: 0, excluded_kind: 0 }, SUMMARY)).toMatchObject({
      excludedGenerated: 0,
      excludedKind: 0,
    });
    expect(coverageOf(METAGRAPH, SUMMARY)).toMatchObject({ excludedGenerated: null, excludedKind: null });
  });
});
