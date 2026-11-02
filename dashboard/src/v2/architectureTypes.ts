/**
 * The shapes `metagraph.json` and `communities.json` actually have, and nothing else.
 *
 * Read off a real bundle — metasfresh, 838,801 nodes, `mode: "community_only"` — not assumed. The things
 * that surprised us are in the comments, because each one would otherwise become a silent wrong answer:
 *
 *   * `member_indices` is **absent** above 50,000 nodes (`include_member_indices = source_node_count <
 *     COMMUNITY_ONLY_THRESHOLD`, metagraph.rs) — above it the same indices live in
 *     `metanode_members.bin` and are addressed by `member_offset` / `member_count`, so a coverage figure
 *     must consult both forms and must not read the empty array as zero.
 *   * `modularity` is `null` on the metagraph and present on the summary. It lives in one file only.
 *   * A metagraph node is a **package** — a directory — not a community. Its `community_id` is a majority
 *     vote of its members, and the vote counts are not exported.
 *   * `size` is members = functions + classes + doc-modules + files; other node kinds are not members.
 */

/** One metagraph node: a package bucket. */
export interface MetaNode {
  id: number;
  label: string;
  /** Members: functions + classes + doc modules + files. */
  size: number;
  functions: number;
  classes: number;
  /** Exported, and 0 for every node on every corpus measured so far — not a channel. */
  avg_complexity: number;
  /** The producer's "layout": `layout_circle` — a constant-radius ring, not a plan. */
  x: number;
  y: number;
  /** Majority vote of the bucket's members. The vote counts never leave the exporter. */
  community_id: number;
  /** Inline membership, below `threshold_community_only` only. */
  member_indices?: number[];
  /** Byte offset of this package's run in `metanode_members.bin` (little-endian `u32`). */
  member_offset?: number;
  /** How many `u32` indices that run holds. Zero means the package has no exported members. */
  member_count?: number;
}

export interface MetaEdge {
  source: number;
  target: number;
  /** Count of underlying relations, not a normalised strength. */
  weight: number;
  kind: string;
}

export interface Metagraph {
  schema_version?: number;
  /** Bundle-relative path to the member-index sidecar, when membership was exported out of line. */
  members_path?: string;
  /** Total `u32` indices in that sidecar. */
  members_total?: number;
  /** Element format of the sidecar — `u32le`. Stated rather than inferred from the byte length. */
  members_format?: string;
  /** `"community_only"` above 50,000 nodes, `"package_metagraph"` below. */
  mode?: string;
  community_only?: boolean;
  threshold_community_only?: number;
  /** The graph's own node count — the denominator coverage is measured against. */
  source_node_count?: number;
  nodes: MetaNode[];
  edges: MetaEdge[];
}

export interface CommunityEntry {
  id: number;
  label: string;
  /** Present, and NOT unique: 214 communities carry 160 distinct values on metasfresh. */
  color?: string;
  /** Members in this community — the aggregate form of the same accounting as `member_indices`. */
  member_count?: number;
  package_count?: number;
}

export interface CommunitySummary {
  schema_version?: number;
  /** Fast-Leiden modularity. The only file that carries it. */
  modularity?: number | null;
  communities: CommunityEntry[];
}

/** The migration graph agrees with neither count, and is read only to quote the disagreement. */
export interface MigrationSummary {
  mode?: string;
  communities?: unknown[];
  nodes?: unknown[];
}

/** A namespace group: a volume the room derives from the labels, never an exported object. */
export interface NamespaceVolume {
  key: string;
  packages: PackageNode[];
  members: number;
  functions: number;
}

export interface PackageNode {
  id: number;
  label: string;
  size: number;
  functions: number;
  classes: number;
  communityId: number;
  /** Producer community label from `communities.json` (falls back to the id string). */
  communityLabel: string;
  /** Derived namespace key — see `namespaceOf`. */
  namespace: string;
  /** A package, the producer's remainder bucket, or a path-heuristic bucket. */
  kind: "package" | "remainder" | "artifact";
  /** Whether another package shares this package's community (17 of 214 communities do). */
  sharesCommunity: boolean;
}

export interface CommunityGroup {
  id: number;
  label: string;
  members: number;
  /** Metagraph nodes in this community — packages, and possibly a bucket. */
  packageCount: number;
  functions: number;
  /** More than one node shares this community (17 of 214 on the captured corpus). */
  shared: boolean;
  /** Whether a member is one of the producer's buckets rather than a package. */
  containsBucket: boolean;
}

export interface Coverage {
  /** Nodes the exported structure accounts for. */
  covered: number;
  /** The graph's own node count. */
  declared: number;
  uncovered: number;
  /** Which form of the evidence the bundle supplied — they count the same members. */
  source: "member_indices" | "member_count";
}

export interface Disagreement {
  /** `metagraph.nodes.length` */
  packages: number;
  /** `communities.communities.length` */
  communities: number;
  /** Entry count in `migration_graph.json`, when present. */
  migration: number | null;
}

export interface ArchitectureView {
  /** `null` when the bundle carries no member accounting at all — never zero. */
  coverage: Coverage | null;
  volumes: NamespaceVolume[];
  /** Packages that are their own namespace: drawn, never wrapped in a volume. */
  standalone: PackageNode[];
  packages: PackageNode[];
  /** Path-heuristic buckets (`root`, absolute-path packages). */
  artifacts: PackageNode[];
  /** The `MAX_METANODES` tail bucket, when the export produced one. */
  remainder: PackageNode | null;
  communities: CommunityGroup[];
  modularity: number | null;
  mode: string;
  /** Community pairs the package edges collapse to. */
  communityPairs: number;
  /** Every exported package edge, in the shape the canvas draws. */
  edgeList: { source: number; target: number; weight: number }[];
  edges: number;
  /** Edges touching the remainder or a path bucket — named, not drawn. */
  artifactEdges: number;
  disagreement: Disagreement;
}

/** Why the room cannot draw, when it cannot. A discriminated union, never a nullable field. */
export type ArchitectureState =
  | { kind: "no-metagraph" }
  | { kind: "unreadable"; reason: string }
  | { kind: "no-communities"; view: ArchitectureView }
  | { kind: "ready"; view: ArchitectureView };
