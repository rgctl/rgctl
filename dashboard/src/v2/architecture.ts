/**
 * Load the architecture datasets and compose the view the room renders.
 *
 * The room reads `metagraph.json` and `communities.json` — both written by `discover --with-universe`
 * today — and `migration_graph.json` only to quote its disagreeing count. No export change is required, and
 * nothing here recomputes a figure the bundle already states.
 */
import { bundleUrl } from "./data";
import {
  aggregateCommunities, communityLabel, couplingOf, coverageOf, disagreementOf, footprintScale,
  groupNamespaces, heightScale, toPackages,
} from "./architectureLayers";
import type {
  ArchitectureState, ArchitectureView, CommunitySummary, Metagraph, MigrationSummary,
} from "./architectureTypes";

async function getJson<T>(name: string): Promise<T | null> {
  let res: Response;
  try {
    res = await fetch(bundleUrl(name));
  } catch (error) {
    throw new Error(`${name}: ${(error as Error).message}`);
  }
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`${name}: HTTP ${res.status}`);
  return (await res.json()) as T;
}

/** Compose the view. Exported so a test can drive it with a fixture instead of the network. */
export function buildView(
  metagraph: Metagraph,
  summary: CommunitySummary | null,
  migration: MigrationSummary | null,
): ArchitectureView {
  const packages = toPackages(metagraph, summary ?? { communities: [] });
  const real = packages.filter((p) => p.kind === "package");
  const { volumes, standalone } = groupNamespaces(packages);
  // every metagraph node, buckets included: the summary's counts are over all of them, so aggregating
  // only the drawable packages would silently disagree with the export's own 197-of-214 figure
  const communities = summary ? aggregateCommunities(packages, summary) : [];
  const { communityPairs, artifactEdges } = couplingOf(packages, metagraph.edges);
  const migrationCount = migration
    ? (migration.communities?.length ?? migration.nodes?.length ?? null)
    : null;

  return {
    coverage: coverageOf(metagraph, summary),
    volumes,
    standalone,
    packages: real,
    artifacts: packages.filter((p) => p.kind === "artifact"),
    remainder: packages.find((p) => p.kind === "remainder") ?? null,
    communities,
    modularity: summary?.modularity ?? null,
    mode: metagraph.mode ?? "unknown",
    communityPairs,
    edgeList: metagraph.edges.map((e) => ({ source: e.source, target: e.target, weight: e.weight })),
    edges: metagraph.edges.length,
    artifactEdges,
    disagreement: disagreementOf(metagraph, summary, migrationCount),
  };
}

/** Which channel each of the room's own derivations produced, for the sentences that must state them. */
export function channelReport(view: ArchitectureView): {
  heights: "log";
  footprints: "sqrt+clamped";
  sizeMedian: number;
  sizeMax: number;
  skew: number;
  packagesInVolumes: number;
  singletonNamespaces: number;
} {
  const all = [...view.packages, ...view.artifacts, ...(view.remainder ? [view.remainder] : [])];
  const sizes = all.map((p) => p.size).sort((a, b) => a - b);
  const median = sizes[Math.floor(sizes.length / 2)] || 0;
  const max = sizes[sizes.length - 1] || 0;
  return {
    heights: "log",
    footprints: "sqrt+clamped",
    sizeMedian: median,
    sizeMax: max,
    skew: median > 0 ? Math.round(max / median) : 0,
    packagesInVolumes: view.volumes.reduce((n, v) => n + v.packages.length, 0),
    singletonNamespaces: view.standalone.length,
  };
}

/** Load and compose. A missing metagraph is a state, not an empty city. */
export async function loadArchitecture(): Promise<ArchitectureState> {
  let metagraph: Metagraph | null;
  try {
    metagraph = await getJson<Metagraph>("metagraph.json");
  } catch (error) {
    return { kind: "unreadable", reason: (error as Error).message };
  }
  if (!metagraph || !Array.isArray(metagraph.nodes) || metagraph.nodes.length === 0) {
    return { kind: "no-metagraph" };
  }

  let summary: CommunitySummary | null = null;
  let migration: MigrationSummary | null = null;
  try {
    summary = await getJson<CommunitySummary>("communities.json");
    migration = await getJson<MigrationSummary>("migration_graph.json");
  } catch (error) {
    return { kind: "unreadable", reason: (error as Error).message };
  }

  const view = buildView(metagraph, summary, migration);
  if (!summary || summary.communities.length === 0) return { kind: "no-communities", view };
  return { kind: "ready", view };
}

/** Scales for the drawing, derived from the view's own distribution. */
export function scalesFor(view: ArchitectureView): {
  height: (size: number) => number;
  footprint: (functions: number) => number;
} {
  const all = [...view.packages, ...view.artifacts, ...(view.remainder ? [view.remainder] : [])];
  return {
    height: heightScale(all.map((p) => p.size)),
    footprint: footprintScale(all.map((p) => p.functions)),
  };
}

export { communityLabel };
