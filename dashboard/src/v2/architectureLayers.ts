/**
 * Every derivation the Architecture room makes, as pure functions over the bundle's own numbers.
 *
 * Nothing here draws, fetches, or touches the DOM: the room and the canvas both consume these results, and a
 * test can assert them without WebGL — which matters, because `jsdom` has none and the canvas is therefore
 * untestable by construction.
 *
 * Three derivations are *ours*, and the room says so wherever it shows them:
 *   1. the namespace grouping (the export has no such object),
 *   2. the community aggregation (the export groups packages by a vote; this only re-presents them),
 *   3. every layout and scale (the export's `x`/`y` is `layout_circle` — a constant-radius ring).
 */
import type {
  CommunityEntry, CommunityGroup, CommunitySummary, Coverage, MetaEdge, Metagraph, NamespaceVolume,
  PackageNode,
} from "./architectureTypes";

/**
 * Tokens that name a build layout rather than a namespace: `main/java/com/x` is not the `main` package.
 * The list is corpus-flavoured on purpose (Maven and Gradle conventions plus this repo's own examples), and
 * it is exported so the room can state the rule and a test can pin it — the volume count is a property of
 * this list, not of the corpus.
 */
export const BUILD_TOKENS = [
  "main", "root", "test", "tests", "e2e", "__tests__", "src", "java", "java-gen", "java-legacy",
  "frontend-webui", "mobile-webui", "resources", "js", "ts",
] as const;

/** A reverse-domain prefix: `org.compiere` is one namespace, `org` alone would group half the world. */
export const REVERSE_DOMAIN = [
  "com", "org", "net", "io", "de", "at", "it", "ch", "uk", "fr", "nl", "se", "dk", "no", "fi", "es", "pl",
  "cz", "ru", "jp", "cn", "in", "br", "au", "ca", "us", "eu", "edu", "gov",
] as const;

const BUILD = new Set<string>(BUILD_TOKENS);
const DOMAIN = new Set<string>(REVERSE_DOMAIN);

/** The two labels the producer writes that are not namespaces at all. */
export const REMAINDER_LABEL = "(other)";
export const ROOT_LABEL = "root";

/**
 * The namespace a package label belongs to.
 *
 * `(other)` is the exporter's tail bucket (`split_off(MAX_METANODES - 1)`), `root` is what `package_label`
 * returns when a file matches neither `/java/` nor `/src/`, and an absolute path is that same fallback keeping
 * the machine-specific parent. Those three are reported as what they are rather than grouped.
 */
export function namespaceOf(label: string): { key: string; kind: PackageNode["kind"] } {
  if (label === REMAINDER_LABEL) return { key: REMAINDER_LABEL, kind: "remainder" };
  if (label === ROOT_LABEL) return { key: ROOT_LABEL, kind: "artifact" };
  if (label.startsWith(".")) return { key: "(absolute path)", kind: "artifact" };
  const segments = label.split(".");
  while (segments.length > 1 && BUILD.has(segments[0])) segments.shift();
  const key = segments.length > 1 && DOMAIN.has(segments[0])
    ? segments.slice(0, 2).join(".")
    : segments[0] || ROOT_LABEL;
  return { key, kind: "package" };
}

/** Packages, with the two facts the room needs per node: its namespace and whether it shares its community. */
export function toPackages(metagraph: Metagraph, summary: CommunitySummary): PackageNode[] {
  const perCommunity = new Map<number, number>();
  for (const node of metagraph.nodes) {
    perCommunity.set(node.community_id, (perCommunity.get(node.community_id) ?? 0) + 1);
  }
  // One Map lookup per package — avoid O(packages × communities) from communityLabel().
  const labelsById = new Map<number, string>();
  for (const entry of summary.communities) {
    labelsById.set(entry.id, entry.label || String(entry.id));
  }
  return metagraph.nodes.map((node) => {
    const { key, kind } = namespaceOf(node.label);
    return {
      id: node.id,
      label: node.label,
      size: node.size,
      functions: node.functions,
      classes: node.classes,
      communityId: node.community_id,
      communityLabel: labelsById.get(node.community_id) ?? String(node.community_id),
      namespace: kind === "package" ? key : kind === "remainder" ? REMAINDER_LABEL : key,
      kind,
      sharesCommunity: (perCommunity.get(node.community_id) ?? 0) > 1,
    };
  });
}

/** Namespace volumes — only where a rule-consistent group actually holds ≥2 packages. */
export function groupNamespaces(packages: PackageNode[]): {
  volumes: NamespaceVolume[];
  standalone: PackageNode[];
} {
  const groups = new Map<string, PackageNode[]>();
  for (const p of packages) {
    if (p.kind !== "package") continue;
    const list = groups.get(p.namespace);
    if (list) list.push(p);
    else groups.set(p.namespace, [p]);
  }
  const volumes: NamespaceVolume[] = [];
  const standalone: PackageNode[] = [];
  for (const [key, list] of groups) {
    if (list.length < 2) {
      standalone.push(...list);
      continue;
    }
    volumes.push({
      key,
      packages: list,
      members: list.reduce((n, p) => n + p.size, 0),
      functions: list.reduce((n, p) => n + p.functions, 0),
    });
  }
  volumes.sort((a, b) => b.members - a.members || a.key.localeCompare(b.key));
  standalone.sort((a, b) => b.size - a.size || a.label.localeCompare(b.label));
  return { volumes, standalone };
}

/**
 * Communities as an aggregation of the same packages — the "collapse" view.
 *
 * This is a re-presentation, not a second measure: the member sum equals the package sum by construction,
 * and each community's `label` is the producer's majority-vote name, which is sometimes a Java modifier.
 */
export function aggregateCommunities(
  packages: PackageNode[],
  summary: CommunitySummary,
): CommunityGroup[] {
  const byId = new Map<number, CommunityGroup>();
  for (const entry of summary.communities) {
    byId.set(entry.id, {
      id: entry.id,
      label: entry.label || String(entry.id),
      members: 0,
      packageCount: 0,
      functions: 0,
      shared: false,
      containsBucket: false,
    });
  }
  for (const p of packages) {
    const group = byId.get(p.communityId);
    if (!group) continue;
    group.members += p.size;
    group.functions += p.functions;
    group.packageCount += 1;
    group.shared = group.packageCount > 1;
    if (p.kind !== "package") group.containsBucket = true;
  }
  return [...byId.values()].sort((a, b) => b.members - a.members || a.id - b.id);
}

/**
 * Coverage, from whichever form of the evidence the bundle carries.
 *
 * `member_indices` (per-node) and `member_count` (per-community) are the same accounting in two forms — on
 * every corpus measured they agree exactly — so this returns **one** figure and names the form used. It is
 * `null` when neither exists, because a missing field must never render as zero.
 */
export function coverageOf(metagraph: Metagraph, summary: CommunitySummary | null): Coverage | null {
  const declared = metagraph.source_node_count ?? 0;
  const withIndices = metagraph.nodes.filter((n) => Array.isArray(n.member_indices));
  if (withIndices.length > 0) {
    const covered = withIndices.reduce((n, node) => n + (node.member_indices?.length ?? 0), 0);
    return { covered, declared, uncovered: Math.max(0, declared - covered), source: "member_indices" };
  }
  // Out-of-line membership: the sidecar's own declared total, not a sum over nodes that carry no arrays.
  if (typeof metagraph.members_total === "number" && metagraph.members_total > 0) {
    const covered = metagraph.members_total;
    return { covered, declared, uncovered: Math.max(0, declared - covered), source: "member_count" };
  }
  if (summary && summary.communities.some((c) => typeof c.member_count === "number")) {
    const covered = summary.communities.reduce((n, c) => n + (c.member_count ?? 0), 0);
    return { covered, declared, uncovered: Math.max(0, declared - covered), source: "member_count" };
  }
  return null;
}

/** How many distinct community pairs the package edges actually form, and how many edges touch a bucket. */
export function couplingOf(
  packages: PackageNode[],
  edges: MetaEdge[],
): { communityPairs: number; artifactEdges: number } {
  const byId = new Map(packages.map((p) => [p.id, p]));
  const pairs = new Set<string>();
  let artifactEdges = 0;
  for (const edge of edges) {
    const from = byId.get(edge.source);
    const to = byId.get(edge.target);
    if (!from || !to) continue;
    if (from.kind !== "package" || to.kind !== "package") {
      artifactEdges += 1;
      continue;
    }
    if (from.communityId === to.communityId) continue;
    pairs.add(from.communityId < to.communityId
      ? `${from.communityId}|${to.communityId}`
      : `${to.communityId}|${from.communityId}`);
  }
  return { communityPairs: pairs.size, artifactEdges };
}

/** Height from member count on a log scale — the distribution is skewed 360:1 against the median. */
export function heightScale(sizes: number[]): (size: number) => number {
  const positive = sizes.filter((s) => s > 0);
  const low = Math.min(...positive);
  const high = Math.max(...positive);
  const span = Math.max(1e-6, Math.log(high) - Math.log(low));
  return (size) => 6 + 120 * ((Math.log(Math.max(size, low)) - Math.log(low)) / span);
}

/** Footprint from function count, sqrt-compressed so one 85,098-function bucket is not the whole picture. */
export function footprintScale(functionCounts: number[]): (functions: number) => number {
  const sorted = [...functionCounts].sort((a, b) => a - b);
  const median = sorted[Math.floor(sorted.length / 2)] || 1;
  return (functions) => Math.min(34, 14 + 6 * Math.sqrt((functions ?? 0) / median));
}

/**
 * Clamp a footprint to its own nearest neighbour.
 *
 * Two solids closer than their width shimmer as the camera turns, so each is held to `0.85 × nn`. The clamp
 * binds where the layout is tight, and how many it binds is reported by the room: a clamped channel is a
 * changed channel.
 */
export function clampFootprints<T extends { side: number; pos: [number, number, number] }>(
  items: T[],
): { bound: number; tightest: number } {
  let bound = 0;
  let tightest = 1;
  for (const item of items) {
    let nearest = Infinity;
    for (const other of items) {
      if (other === item) continue;
      const d = Math.hypot(item.pos[0] - other.pos[0], item.pos[2] - other.pos[2]);
      if (d < nearest) nearest = d;
    }
    const fits = nearest === Infinity ? 1 : (0.85 * nearest) / (item.side || 1);
    const factor = Math.min(1, Number.isFinite(fits) ? fits : 1);
    (item as T & { foot?: number }).foot = item.side * factor;
    if (factor < 0.999) bound += 1;
    tightest = Math.min(tightest, factor);
  }
  return { bound, tightest };
}

/**
 * Golden-angle hue: distinct by construction, and rank order separates the biggest structures most.
 *
 * Community / package colouring also folds ids through {@link communityHueIndex} so the two
 * Universe layers stay in lockstep (`id % 97` spreads sparse Fast-Leiden ids).
 */
export function hueFor(index: number): number {
  return (index * 137.508) % 360;
}

/** Stable index into {@link hueFor} for a Fast-Leiden / package community id. */
export function communityHueIndex(communityId: number): number {
  return communityId % 97;
}

export interface LayoutResult {
  coreRadius: number;
  extentRadius: number;
  /** Volume centres and radii, in draw order. */
  volumes: { key: string; centre: [number, number]; radius: number }[];
  /** Bands outside the mapped structure, where objects that are not architecture sit. */
  artRadius: number;
  artifactRadius: number;
}

/**
 * Volume layout: seat every namespace volume on a ring with an arc proportional to its footprint, then open
 * each volume's packages into a sunflower inside it. Bands outside the ring hold what is not architecture.
 */
export function layoutVolumes(
  volumes: NamespaceVolume[],
  standalones: number,
  options: { seatRadius: (n: number) => number; pad: number },
): LayoutResult {
  const radii = volumes.map((v) => options.seatRadius(v.packages.length) + options.pad);
  const total = radii.reduce((a, b) => a + b, 0);
  const largest = Math.max(...radii, 1);
  const ring = Math.max((1.25 * total) / Math.PI, largest * 1.9, 320);

  const placed: LayoutResult["volumes"] = [];
  let cursor = 0;
  volumes.forEach((volume, index) => {
    const radius = radii[index];
    const share = (2 * radius) / (2 * Math.max(total, 1e-6));
    const angle = cursor * Math.PI * 2 + share * Math.PI;
    cursor += share;
    placed.push({
      key: volume.key,
      centre: [Math.cos(angle) * ring, Math.sin(angle) * ring],
      radius,
    });
  });

  const coreRadius = placed.reduce((m, v) => Math.max(m, Math.hypot(v.centre[0], v.centre[1]) + v.radius), ring);
  const artRadius = coreRadius + options.pad * 6 + Math.max(0, standalones) * 0;
  const artifactRadius = artRadius + options.pad * 8;
  return { coreRadius, extentRadius: artifactRadius + 240, volumes: placed, artRadius, artifactRadius };
}

/** A sunflower seat inside a volume, or on a band, for index `i` of `n`. */
export function sunflower(centre: [number, number], radius: number, i: number, n: number): [number, number, number] {
  const rr = radius * Math.sqrt((i + 0.5) / Math.max(1, n));
  const angle = i * 2.39996;
  return [centre[0] + rr * Math.cos(angle), 0, centre[1] + rr * Math.sin(angle)];
}

/** Frames the camera should fit, and how far out it may go. */
export function framing(layout: LayoutResult): { fit: number; maxDistance: number } {
  return { fit: layout.coreRadius * 1.5, maxDistance: Math.max(9000, layout.extentRadius * 8) };
}

/** Community labels shown as the producer's own strings — never cleaned, never treated as package names. */
export function communityLabel(summary: CommunitySummary | null, id: number): string {
  const entry: CommunityEntry | undefined = summary?.communities.find((c) => c.id === id);
  return entry?.label || String(id);
}

/** The three counts that disagree, each reported with the file it came from. */
export function disagreementOf(
  metagraph: Metagraph,
  summary: CommunitySummary | null,
  migrationCount: number | null,
): { packages: number; communities: number; migration: number | null } {
  return {
    packages: metagraph.nodes.length,
    communities: summary?.communities.length ?? 0,
    migration: migrationCount,
  };
}
