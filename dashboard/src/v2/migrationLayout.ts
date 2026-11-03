/**
 * Pure layout for the Migration room: place communities in world space from a plan.
 *
 * One sunflower disk for all nodes (order encoded as hue / Y), not one ring per schedule_step —
 * a total order of N communities would otherwise span O(N) rings and vanish on screen.
 */
import type {
  MigrationCommunityEdge,
  MigrationCommunityNode,
  MigrationOrderMode,
  MigrationPlanPayload,
} from "../migration/types";
import { sunflower } from "./architectureLayers";

export interface MigrationLayoutNode {
  communityId: number;
  label: string;
  /** schedule_step or priority_rank depending on order mode */
  orderBand: number;
  /** 0..1 along the active order (for hue continuum) */
  orderNorm: number;
  score: number;
  memberCount: number;
  position: [number, number, number];
  /** 0..1 relative size from member_count */
  sizeNorm: number;
  /** 0..1 relative height/importance from priority_score */
  scoreNorm: number;
}

export interface MigrationLayoutEdge {
  source: number;
  target: number;
  weight: number;
}

export interface MigrationLayout {
  nodes: MigrationLayoutNode[];
  edges: MigrationLayoutEdge[];
  orderMode: MigrationOrderMode;
  bandCount: number;
}

/** Seat spacing so ~256 communities fill the framed view (not a pea in the middle). */
const SEAT = 11;
/** Cap drawn coupling edges — metasfresh has ~2k; a full web is an unreadable hairball. */
export const MAX_LAYOUT_EDGES = 480;

/**
 * Build positions from plan steps + community metadata.
 */
export function layoutMigration(
  plan: MigrationPlanPayload,
  communities: MigrationCommunityNode[],
  graphEdges: MigrationCommunityEdge[],
): MigrationLayout {
  const byId = new Map(communities.map((c) => [c.id, c]));
  const maxMembers = Math.max(1, ...communities.map((c) => c.member_count));
  const scores = plan.steps.map((s) => s.priority_score);
  const minS = Math.min(...scores, 0);
  const maxS = Math.max(...scores, 1);
  const span = Math.max(1e-9, maxS - minS);

  const bandOf = (step: (typeof plan.steps)[0]) =>
    plan.order_mode === "scheduled" ? step.schedule_step : step.priority_rank;

  const ordered = [...plan.steps].sort((a, b) => {
    const d = bandOf(a) - bandOf(b);
    return d !== 0 ? d : a.community_id - b.community_id;
  });

  const n = ordered.length;
  const diskR = SEAT * Math.sqrt(Math.max(n, 1));
  const maxBand = Math.max(1, ...ordered.map(bandOf));
  const minBand = Math.min(...ordered.map(bandOf), 1);
  const bandSpan = Math.max(1e-9, maxBand - minBand);

  const nodes: MigrationLayoutNode[] = ordered.map((step, i) => {
    const meta = byId.get(step.community_id);
    const band = bandOf(step);
    const scoreNorm = (step.priority_score - minS) / span;
    const orderNorm = (band - minBand) / bandSpan;
    const [x, , z] = sunflower([0, 0], diskR, i, Math.max(n, 1));
    return {
      communityId: step.community_id,
      label: step.label,
      orderBand: band,
      orderNorm,
      score: step.priority_score,
      memberCount: meta?.member_count ?? 0,
      // Keep the disk flat — a large Y lift turns the sunflower into a hairball sphere.
      position: [x, (scoreNorm - 0.5) * diskR * 0.08, z],
      sizeNorm: Math.sqrt((meta?.member_count ?? 1) / maxMembers),
      scoreNorm,
    };
  });

  const placed = new Set(nodes.map((node) => node.communityId));
  const edges = graphEdges
    .filter((e) => placed.has(e.source) && placed.has(e.target))
    .map((e) => ({ source: e.source, target: e.target, weight: e.weight }))
    .sort((a, b) => b.weight - a.weight)
    .slice(0, MAX_LAYOUT_EDGES);

  const uniqueBands = new Set(nodes.map((node) => node.orderBand));

  return {
    nodes,
    edges,
    orderMode: plan.order_mode,
    bandCount: Math.max(uniqueBands.size, 1),
  };
}

/** Draw-model channels for tests (no WebGL). */
export function drawModelChannels(layout: MigrationLayout): {
  sizes: number[];
  hues: number[];
  edgeWeights: number[];
  /** Bounding radius of node positions in XZ — must stay O(sqrt(N)), not O(N). */
  extent: number;
} {
  let extent = 0;
  for (const n of layout.nodes) {
    extent = Math.max(extent, Math.hypot(n.position[0], n.position[2]));
  }
  return {
    sizes: layout.nodes.map((n) => meshRadius(n.sizeNorm)),
    hues: layout.nodes.map((n) => orderHue(n.orderNorm)),
    edgeWeights: layout.edges.map((e) => e.weight),
    extent,
  };
}

export function meshRadius(sizeNorm: number): number {
  return 1.8 + sizeNorm * 5.5;
}

/** Hue degrees along the order continuum (cyan → amber). */
export function orderHue(orderNorm: number): number {
  return 195 + orderNorm * 140;
}

/** @deprecated prefer orderHue(orderNorm); kept for call sites that only have the band index. */
export function waveHue(band: number): number {
  return (band * 47 + 200) % 360;
}
