/**
 * Def-use (PDG data edges) for one function — drawn when the Taint room switches the
 * center canvas to DFG.
 *
 * Layout is deliberately simple: statements top→bottom by source line, data edges as
 * arcs. Control edges are optional and muted; the question is "what flows into what".
 */
import { useMemo } from "react";
import { Chip } from "../shell";

export interface DfgNode {
  id: string;
  line: number;
  label: string;
  kind?: string;
  defined?: string[];
  used?: string[];
}

export interface DfgEdge {
  source: string;
  target: string;
  kind: string;
  variable?: string | null;
}

export interface DfgDetail {
  function_id: string;
  name: string;
  nodes: DfgNode[];
  edges: DfgEdge[];
}

const ROW_H = 44;
const NODE_H = 28;
const NODE_W = 280;
const PAD_X = 40;
const PAD_Y = 28;

export function DfgGraph({
  dfg,
  highlightVariable,
  maxHeightClass = "max-h-[52vh]",
}: {
  dfg: DfgDetail;
  highlightVariable?: string | null;
  maxHeightClass?: string;
}) {
  const layout = useMemo(() => {
    const sorted = [...dfg.nodes].sort((a, b) => a.line - b.line || a.id.localeCompare(b.id));
    const height = Math.max(160, sorted.length * ROW_H + PAD_Y * 2);
    const width = Math.max(520, NODE_W + PAD_X * 2 + 80);
    const pos = new Map<string, { x: number; y: number }>();
    sorted.forEach((n, i) => {
      pos.set(n.id, { x: PAD_X + NODE_W / 2, y: PAD_Y + i * ROW_H + NODE_H / 2 });
    });
    return { sorted, pos, width, height };
  }, [dfg.nodes]);

  const dataEdges = dfg.edges.filter((e) => e.kind === "data" || !e.kind);
  const otherEdges = dfg.edges.filter((e) => e.kind && e.kind !== "data");

  return (
    <div className="flex flex-col gap-2" data-testid="taint-dfg-stage">
      <div className="flex flex-wrap items-center gap-2">
        <Chip tone="blue">{dfg.nodes.length} nodes</Chip>
        <Chip tone="zinc">{dataEdges.length} data edges</Chip>
        {highlightVariable && (
          <Chip tone="amber">
            var <code>{highlightVariable}</code>
          </Chip>
        )}
        {otherEdges.length > 0 && (
          <span className="text-[10px] text-zinc-600">{otherEdges.length} non-data edges muted</span>
        )}
      </div>
      <div className={`${maxHeightClass} overflow-auto rounded border border-white/8 bg-black/30`}>
        <svg width={layout.width} height={layout.height} className="block">
          <defs>
            <marker
              id="rgctl-dfg-arrow"
              viewBox="0 0 10 10"
              refX="9"
              refY="5"
              markerWidth="5"
              markerHeight="5"
              orient="auto-start-reverse"
            >
              <path d="M 0 0 L 10 5 L 0 10 z" fill="#38bdf8" />
            </marker>
          </defs>
          {otherEdges.map((e, i) => {
            const a = layout.pos.get(e.source);
            const b = layout.pos.get(e.target);
            if (!a || !b) return null;
            return (
              <path
                key={`c-${i}`}
                d={`M ${a.x + NODE_W / 2} ${a.y} C ${a.x + NODE_W / 2 + 48} ${a.y}, ${b.x + NODE_W / 2 + 48} ${b.y}, ${b.x + NODE_W / 2} ${b.y}`}
                fill="none"
                stroke="#52525b"
                strokeWidth={1}
                strokeDasharray="3 3"
                opacity={0.45}
              />
            );
          })}
          {dataEdges.map((e, i) => {
            const a = layout.pos.get(e.source);
            const b = layout.pos.get(e.target);
            if (!a || !b) return null;
            const hot = highlightVariable && e.variable === highlightVariable;
            const midX = Math.max(a.x, b.x) + NODE_W / 2 + 36;
            return (
              <g key={`d-${i}`}>
                <path
                  d={`M ${a.x + NODE_W / 2} ${a.y} C ${midX} ${a.y}, ${midX} ${b.y}, ${b.x + NODE_W / 2} ${b.y}`}
                  fill="none"
                  stroke={hot ? "#fbbf24" : "#38bdf8"}
                  strokeWidth={hot ? 2 : 1.4}
                  markerEnd="url(#rgctl-dfg-arrow)"
                  opacity={hot || !highlightVariable ? 1 : 0.35}
                />
                {e.variable && (
                  <text
                    x={midX + 4}
                    y={(a.y + b.y) / 2}
                    className="fill-zinc-500"
                    style={{ fontSize: 9, fontFamily: "ui-monospace, monospace" }}
                  >
                    {e.variable}
                  </text>
                )}
              </g>
            );
          })}
          {layout.sorted.map((n) => {
            const p = layout.pos.get(n.id)!;
            const hot =
              highlightVariable &&
              (n.defined?.includes(highlightVariable) || n.used?.includes(highlightVariable));
            return (
              <g key={n.id} transform={`translate(${p.x - NODE_W / 2}, ${p.y - NODE_H / 2})`}>
                <rect
                  width={NODE_W}
                  height={NODE_H}
                  rx={6}
                  className={
                    hot ? "fill-amber-500/15 stroke-amber-400/50" : "fill-white/[0.04] stroke-white/15"
                  }
                  strokeWidth={1}
                />
                <text
                  x={8}
                  y={12}
                  className="fill-zinc-500"
                  style={{ fontSize: 9, fontFamily: "ui-monospace, monospace" }}
                >
                  {n.line > 0 ? `L${n.line}` : "—"} · {n.kind ?? "stmt"}
                </text>
                <text
                  x={8}
                  y={23}
                  className="fill-zinc-100"
                  style={{ fontSize: 10, fontFamily: "ui-monospace, monospace" }}
                >
                  {(n.label || "—").length > 42 ? `${n.label.slice(0, 40)}…` : n.label || "—"}
                </text>
              </g>
            );
          })}
        </svg>
      </div>
    </div>
  );
}
