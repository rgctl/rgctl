/**
 * Compiler-IR view of one function.
 *
 * Every visual property encodes real data — none of it is decoration:
 *
 *   vertical position = dominator depth, walked from the entry block via `idom`
 *   edge colour       = control-flow edge type
 *   amber dot         = the block appears in a dominance frontier (where a φ would be placed)
 *   hover             = the block's dominator chain, i.e. what must execute before it
 *   mute on hover     = everything not on that chain, so the chain is read by exclusion
 *
 * That amber-dot line is the one the prototype got wrong, and the direction of the relation is the reason:
 * see `frontierBlockIds` below before touching it.
 *
 * The vertical axis is the whole point: the layout *is* the analysis. A block lower down is dominated by
 * everything on the amber path above it. Ported from the retired prototype, which is where this was
 * worked out; the prototype showed a static dataset, this draws a live one.
 *
 * Selection is controlled from the parent so the source pane and this pane share one selection rather
 * than each keeping its own — two selections for one Focus is the failure this design exists to avoid.
 *
 * Two homes, one drawing. The **Spine** lens puts it beside the source in a half-pane at `52vh`; the
 * **CFG·PDG** lens gives it a room, at `70vh`, with the dominance-frontier toggle and the stat row above.
 * Height and reading instructions are therefore props — the layout, the encoding and the interaction are
 * not, because those are what make it the same analysis in both places.
 */
import { useMemo, useState, type ReactNode } from "react";
import type { CfgDetail } from "../data";
import { Chip } from "../shell";

const EDGE_COLOR: Record<string, string> = {
  next: "#6b7280",
  if_true: "#22c55e",
  if_false: "#ef4444",
  return: "#3b82f6",
  exception: "#a855f7",
  jump: "#f59e0b",
};

const ROW_H = 40;
const NODE_H = 24;
const NODE_W = 158;
const PAD_X = 28;
const PAD_Y = 22;
const MAX_DEPTH_GUARD = 512;

/** A block with no source lines: `start_line: 0` is the absence of a line, not line zero. */
export function hasLines(b: { start_line: number; end_line: number }): boolean {
  return b.start_line > 0 || b.end_line > 0;
}

export function lineLabel(b: { start_line: number; end_line: number }): string {
  if (!hasLines(b)) return "—";
  return b.start_line === b.end_line ? `L${b.start_line}` : `L${b.start_line}–${b.end_line}`;
}

/**
 * Dominator depth per block id, walked from `idom`.
 *
 * Memoized, with a depth guard: `idom` can contain a cycle in malformed input, and a walk that trusts it
 * would hang the render rather than draw a wrong picture.
 *
 * Exported because the CFG·PDG lens reports the deepest level as a stat: the number must come from the
 * same walk that lays the graph out, or the receipt and the picture can disagree.
 */
export function dominatorDepths(idom: number[] | undefined, ids: number[]): Map<number, number> {
  const depth = new Map<number, number>();
  if (!idom) return depth;
  const walk = (x: number, guard = 0): number => {
    const known = depth.get(x);
    if (known !== undefined) return known;
    if (guard > MAX_DEPTH_GUARD) return 0;
    const parent = idom[x];
    const value = parent === undefined || parent === x || parent < 0 ? 0 : walk(parent, guard + 1) + 1;
    depth.set(x, value);
    return value;
  };
  for (const id of ids) walk(id);
  return depth;
}

/**
 * The blocks a φ would be placed in — the blocks that **appear** in some dominance frontier.
 *
 * The direction of `dominance_frontiers` is the trap here, and it was read backwards. The export writes
 * `out[i] = dom.frontier(block_i)` (`crates/rgctl-dashboard/src/cfg_export.rs`), and `DominatorTree::frontier`
 * is DF(i) — the standard relation, as its one consumer confirms: `pdg.rs` iterates `frontier(block_id)`
 * to attach control dependence from the *controller* in `block_id` to the *dependent* block it yields.
 *
 * So `row[i]` is "the joins where a φ for definitions made in block i belongs", and the block that *has* a
 * non-empty row is the block whose definitions flow into a join — not the place the φ goes. Building the
 * set from row indices put the amber dot on the branch instead of on the merge. Verified on a real bundle:
 * `toggleActive` has `DF(2) = DF(5) = [0]`, and block 0 is the only block with two predecessors.
 *
 * The φ site is therefore the union of the rows.
 */
export function frontierBlockIds(cfg: CfgDetail): Set<number> {
  const s = new Set<number>();
  cfg.dominance_frontiers?.forEach((list) => {
    for (const id of list ?? []) s.add(id);
  });
  return s;
}

/** The largest dominator depth in the graph: how many rows the layout will need. */
export function maxDominatorDepth(cfg: CfgDetail): number {
  const depths = dominatorDepths(
    cfg.idom,
    cfg.blocks.map((b) => b.id),
  );
  return Math.max(...[...depths.values()], 0);
}

export function CfgGraph({
  cfg,
  selected,
  onSelect,
  showFrontier = true,
  onToggleFrontier,
  maxHeightClass = "max-h-[52vh]",
  footnote,
}: {
  cfg: CfgDetail;
  selected: number | null;
  onSelect: (id: number | null) => void;
  /**
   * Draw the amber frontier dots. The *control* belongs to the caller: the lens offers it as a labelled
   * toggle because it is a claim about the picture, while the Spine keeps the graph's default reading.
   */
  showFrontier?: boolean;
  onToggleFrontier?: () => void;
  /** The graph has two homes — half a pane in the Spine, most of a room in the lens — so its height is a prop. */
  maxHeightClass?: string;
  /** The reading instructions differ with the neighbour: the Spine's source pane, or nothing. */
  footnote?: ReactNode;
}) {
  const [hover, setHover] = useState<number | null>(null);

  const layout = useMemo(() => {
    const ids = cfg.blocks.map((b) => b.id);
    const depth = dominatorDepths(cfg.idom, ids);

    const rows = new Map<number, typeof cfg.blocks>();
    for (const b of cfg.blocks) {
      const d = depth.get(b.id) ?? 0;
      if (!rows.has(d)) rows.set(d, []);
      rows.get(d)!.push(b);
    }

    const maxDepth = Math.max(...rows.keys(), 0);
    const widest = Math.max(...[...rows.values()].map((r) => r.length), 1);
    const width = Math.max(880, widest * (NODE_W + 26) + PAD_X * 2);
    const height = (maxDepth + 1) * ROW_H + PAD_Y * 2;

    const pos = new Map<number, { x: number; y: number }>();
    for (const [d, list] of rows) {
      list.sort((a, b) => a.id - b.id);
      const usable = width - PAD_X * 2;
      list.forEach((b, i) => {
        pos.set(b.id, { x: PAD_X + (usable * (i + 0.5)) / list.length, y: PAD_Y + d * ROW_H });
      });
    }
    return { pos, width, height, maxDepth };
  }, [cfg]);

  /** Blocks that must execute before `hover`: the reverse of the idom walk. */
  const chain = useMemo(() => {
    const s = new Set<number>();
    if (hover === null || !cfg.idom) return s;
    let x: number | undefined = hover;
    let guard = 0;
    while (x !== undefined && !s.has(x) && guard++ < MAX_DEPTH_GUARD) {
      s.add(x);
      // Explicitly typed: inside the loop `cfg.idom` is optional, and letting TypeScript infer from a
      // self-referential reassignment makes it `any` rather than `number | undefined`.
      const parent: number | undefined = cfg.idom[x];
      if (parent === x || parent === undefined || parent < 0) break;
      x = parent;
    }
    return s;
  }, [hover, cfg]);

  /** Blocks in a dominance frontier — indexed by block id, matching `idom`. */
  const frontier = useMemo(() => frontierBlockIds(cfg), [cfg]);

  const edges = cfg.edges ?? [];

  const edgeKinds = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const e of edges) counts[e.edge_type] = (counts[e.edge_type] ?? 0) + 1;
    return counts;
  }, [edges]);

  const withLines = cfg.blocks.filter(hasLines).length;

  return (
    <div className="flex flex-col gap-2">
      <div className="flex flex-wrap items-center gap-x-3 gap-y-1.5">
        {edges.length === 0 && (
          <span className="text-[10px] text-amber-300/90">
            no control-flow edges in this bundle — blocks are drawn unconnected
          </span>
        )}
        {Object.entries(edgeKinds).map(([kind, n]) => (
          <span key={kind} className="flex items-center gap-1.5">
            <span className="h-0.5 w-5 rounded" style={{ background: EDGE_COLOR[kind] ?? "#888" }} />
            <span className="text-[10px] text-zinc-400">
              {kind} <span className="text-zinc-600">{n}</span>
            </span>
          </span>
        ))}
        {onToggleFrontier ? (
          /* A labelled toggle, not a bare count: the dots encode a claim ("a φ belongs here") and a reader
             has to be able to switch them off to see the graph without it. */
          <button
            type="button"
            onClick={onToggleFrontier}
            aria-pressed={showFrontier}
            className={`ml-auto rounded border px-2 py-0.5 text-[10px] ${
              showFrontier
                ? "border-amber-500/30 bg-amber-500/10 text-amber-300"
                : "border-white/10 text-zinc-500 hover:text-zinc-300"
            }`}
          >
            dominance frontiers {showFrontier ? "on" : "off"}
          </button>
        ) : (
          <Chip tone="amber">dominance frontier {frontier.size}</Chip>
        )}
        <span className={`${onToggleFrontier ? "" : "ml-auto"} text-[10px] text-zinc-600`}>
          depth {layout.maxDepth} · {withLines}/{cfg.blocks.length} blocks carry source lines
        </span>
      </div>

      <div className={`${maxHeightClass} overflow-auto rounded border border-white/8 bg-black/30`}>
        <svg width={layout.width} height={layout.height} className="block">
          <defs>
            {Object.entries(EDGE_COLOR).map(([kind, color]) => (
              <marker
                key={kind}
                id={`rgctl-arrow-${kind}`}
                viewBox="0 0 10 10"
                refX="9"
                refY="5"
                markerWidth="5"
                markerHeight="5"
                orient="auto-start-reverse"
              >
                <path d="M 0 0 L 10 5 L 0 10 z" fill={color} />
              </marker>
            ))}
          </defs>

          {edges.map((e, i) => {
            const from = layout.pos.get(e.from);
            const to = layout.pos.get(e.to);
            if (!from || !to) return null;
            const y1 = from.y + NODE_H;
            const y2 = to.y;
            const dim = hover !== null && !chain.has(e.from) && !chain.has(e.to);
            return (
              <path
                key={i}
                d={
                  y2 > y1
                    ? `M ${from.x} ${y1} C ${from.x - 70} ${y1 + 26}, ${to.x - 70} ${y2 - 26}, ${to.x} ${y2}`
                    : `M ${from.x} ${y1} C ${from.x} ${y1 + (y2 - y1) / 2}, ${to.x} ${y2 - (y2 - y1) / 2}, ${to.x} ${y2}`
                }
                fill="none"
                stroke={EDGE_COLOR[e.edge_type] ?? "#888"}
                strokeWidth={1}
                strokeDasharray={e.edge_type === "exception" ? "2 3" : undefined}
                markerEnd={`url(#rgctl-arrow-${e.edge_type})`}
                opacity={dim ? 0.1 : 0.7}
              />
            );
          })}

          {cfg.blocks.map((b) => {
            const p = layout.pos.get(b.id);
            if (!p) return null;
            const isEntry = b.id === cfg.entry;
            const isExit = cfg.exits.includes(b.id);
            const onChain = chain.has(b.id);
            const isSelected = selected === b.id;
            const dim = hover !== null && !onChain && !isSelected;
            return (
              <g
                key={b.id}
                transform={`translate(${p.x - NODE_W / 2}, ${p.y})`}
                onMouseEnter={() => setHover(b.id)}
                onMouseLeave={() => setHover(null)}
                onClick={() => onSelect(isSelected ? null : b.id)}
                style={{ cursor: "pointer" }}
                opacity={dim ? 0.2 : 1}
              >
                <rect
                  width={NODE_W}
                  height={NODE_H}
                  rx={5}
                  fill={isSelected ? "#1b2836" : onChain ? "#2a2313" : "#141a21"}
                  stroke={isSelected ? "#7dd3fc" : onChain ? "#f59e0b" : isEntry ? "#22c55e" : isExit ? "#3b82f6" : "#2b323b"}
                  strokeWidth={isSelected || onChain ? 1.4 : 1}
                />
                <text x={9} y={16} className="fill-zinc-300" style={{ fontSize: 10, fontFamily: "ui-monospace, monospace" }}>
                  B{b.id}
                </text>
                <text x={44} y={16} className="fill-zinc-500" style={{ fontSize: 10, fontFamily: "ui-monospace, monospace" }}>
                  {/* `lineLabel` already renders `—` for a block with no source lines, so the special
                      case here was both redundant and too long for a 158px block: "no source line" ran into
                      the entry/exit chip. The role chips carry the structural meaning; the line field only
                      has to say whether there is a line. */}
                  {lineLabel(b)}
                  {` · ${b.stmts_total ?? b.statements.length} stmt`}
                </text>
                {isEntry && (
                  <text x={NODE_W - 30} y={16} className="fill-emerald-400" style={{ fontSize: 9 }}>
                    entry
                  </text>
                )}
                {isExit && !isEntry && (
                  <text x={NODE_W - 24} y={16} className="fill-sky-400" style={{ fontSize: 9 }}>
                    exit
                  </text>
                )}
                {showFrontier && frontier.has(b.id) && (
                  <circle cx={NODE_W - 8} cy={7} r={2.6} className="fill-amber-400" />
                )}
              </g>
            );
          })}
        </svg>
      </div>

      {footnote ?? (
        <p className="text-[10px] leading-relaxed text-zinc-600">
          Vertical position is dominator depth from <code className="text-zinc-500">idom</code> — the layout
          is the dominance relation, not a force simulation. Hover a block to light up its dominator chain;
          click to read its statements and highlight its lines.
        </p>
      )}
    </div>
  );
}
