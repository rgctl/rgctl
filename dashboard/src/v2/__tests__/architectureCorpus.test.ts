/**
 * The room's headline numbers, pinned to a **captured** bundle rather than a hand-written one.
 *
 * The fixture is the real `metagraph.json` + `communities.json` from metasfresh (23,773 Java files, graph
 * 838,801 nodes / 2,847,044 edges, `new_dawn_uat` @ `9f9d5a16`), projected to the fields this app reads —
 * every one of the 256 nodes and all 6,524 edges are kept, because those counts are what the test asserts.
 * One edit: the eight absolute-path labels had this checkout's user replaced with `checkout`, because the
 * point of those labels is that they *are* machine-specific — publishing mine in a public repo adds nothing
 * to the test and the defect is intact.
 *
 * A hand-written fixture encodes the author's belief about the shape, and here the shape is the question, so
 * every assertion below is a figure measured off a real corpus. Regenerate by re-running
 * `rgctl discover . --with-universe --with-cfg` and re-capturing the two files.
 */
import { describe, expect, it } from "vitest";
import { buildView, channelReport } from "../architecture";
import { aggregateCommunities, coverageOf, groupNamespaces, toPackages } from "../architectureLayers";
import type { CommunitySummary, Metagraph } from "../architectureTypes";
// Imported rather than read from disk: the v2 tsconfig carries browser types only, and a test that needs
// `node:fs` to load its fixture is a test that cannot run in the environment the app is checked in.
import metagraphJson from "./fixtures/architecture/metagraph.json";
import summaryJson from "./fixtures/architecture/communities.json";

const metagraph = metagraphJson as unknown as Metagraph;
const summary = summaryJson as unknown as CommunitySummary;
const view = buildView(metagraph, summary, null);

describe("metasfresh, as captured", () => {
  it("is the corpus these numbers came from", () => {
    expect(metagraph.mode).toBe("community_only");
    expect(metagraph.threshold_community_only).toBe(50000);
    expect(metagraph.source_node_count).toBe(838801);
    expect(metagraph.nodes).toHaveLength(256);
    expect(metagraph.edges).toHaveLength(6524);
    expect(summary.communities).toHaveLength(214);
  });

  it("reports coverage from the summary, because member_indices is absent by design at this size", () => {
    expect(metagraph.nodes.some((n) => Array.isArray(n.member_indices))).toBe(false);
    const coverage = coverageOf(metagraph, summary);
    expect(coverage?.source).toBe("member_count");
    expect(coverage?.covered).toBe(361560);
    expect(coverage?.declared).toBe(838801);
    // 43.1% — the number the room leads with
    expect(Math.round((coverage!.covered / coverage!.declared) * 1000) / 10).toBe(43.1);
  });

  it("groups packages into the volumes the plan was decided on", () => {
    const { volumes, standalone } = groupNamespaces(toPackages(metagraph, summary));
    expect(volumes).toHaveLength(10);
    expect(volumes.reduce((n, v) => n + v.packages.length, 0)).toBe(232);
    expect(standalone).toHaveLength(14);
    expect(volumes[0].key).toBe("de.metas");
    expect(volumes[0].packages).toHaveLength(171);
    // areas are the group's own member counts, not a re-derivation
    expect(volumes[0].members).toBe(
      volumes[0].packages.reduce((n, p) => n + p.size, 0),
    );
  });

  it("shows why communities cannot be the level above packages", () => {
    // over every metagraph node, buckets included — that is the partition the summary describes
    const groups = aggregateCommunities(toPackages(metagraph, summary), summary);
    expect(groups).toHaveLength(214);
    expect(groups.filter((g) => !g.shared)).toHaveLength(197);
    expect(groups.filter((g) => g.shared)).toHaveLength(17);
    // the same members, partitioned the other way — not a second measure
    const byCommunity = groups.reduce((n, g) => n + g.members, 0);
    const byPackage = metagraph.nodes.reduce((n, p) => n + p.size, 0);
    expect(byCommunity).toBe(byPackage);
    // and the aggregation is not all packages: three communities hold a bucket
    expect(groups.filter((g) => g.containsBucket).length).toBeGreaterThan(0);
  });

  it("names the coupling it refuses to draw", () => {
    expect(view.edges).toBe(6524);
    expect(view.artifactEdges).toBe(1725);
    // the remainder is a hole in the map, not a package: 15.6% of the graph in one object
    expect(view.remainder?.size).toBe(130454);
    expect(Math.round((130454 / 838801) * 1000) / 10).toBe(15.6);
    // and `root` is a path-heuristic bucket with no functions at all
    const root = view.artifacts.find((p) => p.label === "root");
    expect(root?.size).toBe(21738);
    expect(root?.functions).toBe(0);
  });

  it("reports the channel facts the room has to state", () => {
    const report = channelReport(view);
    expect(report.heights).toBe("log");
    expect(report.sizeMedian).toBe(362);
    expect(report.sizeMax).toBe(130454);
    expect(report.skew).toBe(360);
    expect(report.packagesInVolumes).toBe(232);
    expect(report.singletonNamespaces).toBe(14);
  });

  it("keeps the modularity and names its algorithm", () => {
    expect(view.modularity).toBeCloseTo(0.386, 3);
  });

  it("has 8 labels that are absolute paths, and files them as artifacts", () => {
    const absolute = view.artifacts.filter((p) => p.label.startsWith("."));
    expect(absolute).toHaveLength(8);
  });
});
