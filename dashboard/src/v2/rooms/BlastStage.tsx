/**
 * Blast-radius canvas for the Taint room.
 *
 * Depth is the encoding: concentric shells are the search radius; callers from the
 * WASM expand sit on the ring matching their hop count.
 */
import { Chip, Unavailable } from "../shell";
import type { BlastFunctionScore } from "../data/loadTaint";
import type { BlastRadiusState } from "../hooks/useBlastRadius";

export interface BlastCallerPlot {
  name: string;
  depth: number;
}

/** Cap dots so a dense impact zone stays readable. */
const MAX_CALLERS_DRAWN = 80;

export function BlastStage({
  seedName,
  depth,
  onDepthChange,
  score,
  blast,
}: {
  seedName: string;
  depth: number;
  onDepthChange: (d: number) => void;
  score: BlastFunctionScore | null;
  blast: BlastRadiusState;
}) {
  const size = 360;
  const cx = size / 2;
  const cy = size / 2;
  const maxR = size / 2 - 28;
  const ringStep = depth > 0 ? maxR / depth : maxR;

  const payload = blast.kind === "ready" ? blast.payload : null;
  const liveScore = payload
    ? {
        index: payload.seed_index,
        score: payload.score,
        direct: payload.direct_caller_count,
        zone: payload.impact_zone_count,
      }
    : score;

  const callers: BlastCallerPlot[] = (payload?.callers ?? [])
    .filter((c) => c.depth >= 1 && c.depth <= depth)
    .slice(0, MAX_CALLERS_DRAWN)
    .map((c) => ({ name: c.name, depth: c.depth }));

  const byDepth = new Map<number, BlastCallerPlot[]>();
  for (const c of callers) {
    if (!byDepth.has(c.depth)) byDepth.set(c.depth, []);
    byDepth.get(c.depth)!.push(c);
  }

  const truncated =
    payload != null && payload.callers.filter((c) => c.depth >= 1 && c.depth <= depth).length >
      MAX_CALLERS_DRAWN;

  return (
    <div className="flex min-h-[200px] flex-1 flex-col gap-3" data-testid="taint-blast-stage">
      <div className="flex flex-wrap items-center gap-3">
        <label className="flex items-center gap-2 text-[11px] text-zinc-400">
          <span className="shrink-0">Caller depth</span>
          <input
            type="range"
            min={1}
            max={10}
            step={1}
            value={depth}
            onChange={(e) => onDepthChange(Number(e.target.value))}
            data-testid="taint-blast-depth"
            className="w-36"
          />
          <span className="w-5 tabular-nums text-zinc-200">{depth}</span>
        </label>
        {liveScore && (
          <>
            <Chip tone="amber">score {liveScore.score.toFixed(3)}</Chip>
            <Chip tone="zinc">direct {liveScore.direct}</Chip>
            <Chip tone="zinc">zone {liveScore.zone}</Chip>
          </>
        )}
        {blast.kind === "loading" && <Chip tone="blue">loading engine…</Chip>}
      </div>

      {blast.kind === "failed" && (
        <Unavailable tag="blast engine" what="Could not expand blast radius" why={blast.reason} />
      )}
      {blast.kind === "absent" && (
        <Unavailable tag="no join" what="No blast seed for this function" why={blast.reason} />
      )}

      <div className="flex min-h-0 flex-1 flex-col items-center justify-center overflow-auto rounded-lg border border-white/8 bg-[#060a12] p-3">
        <svg width={size} height={size} viewBox={`0 0 ${size} ${size}`} className="block max-w-full">
          {Array.from({ length: depth }, (_, i) => {
            const d = i + 1;
            const r = ringStep * d;
            return (
              <g key={d}>
                <circle
                  cx={cx}
                  cy={cy}
                  r={r}
                  fill="none"
                  stroke={d === 1 ? "rgba(251,191,36,0.35)" : "rgba(255,255,255,0.12)"}
                  strokeWidth={d === 1 ? 1.5 : 1}
                  strokeDasharray={d === depth ? undefined : "4 4"}
                />
                <text
                  x={cx + r - 4}
                  y={cy - 4}
                  textAnchor="end"
                  className="fill-zinc-600"
                  style={{ fontSize: 9 }}
                >
                  d{d}
                </text>
              </g>
            );
          })}
          <circle cx={cx} cy={cy} r={22} className="fill-sky-500/20 stroke-sky-400/60" strokeWidth={1.5} />
          <text
            x={cx}
            y={cy + 3}
            textAnchor="middle"
            className="fill-sky-100"
            style={{ fontSize: 10, fontFamily: "ui-monospace, monospace" }}
          >
            {(payload?.seed_name || seedName).length > 12
              ? `${(payload?.seed_name || seedName).slice(0, 10)}…`
              : payload?.seed_name || seedName}
          </text>

          {[...byDepth.entries()].map(([d, list]) => {
            const r = ringStep * d;
            return list.map((c, i) => {
              const angle = -Math.PI / 2 + ((i + 0.5) / list.length) * Math.PI * 2;
              const x = cx + Math.cos(angle) * r;
              const y = cy + Math.sin(angle) * r;
              return (
                <g key={`${d}-${c.name}-${i}`}>
                  <circle
                    cx={x}
                    cy={y}
                    r={10}
                    className="fill-amber-500/25 stroke-amber-400/50"
                    strokeWidth={1}
                  />
                  <text
                    x={x}
                    y={y + 18}
                    textAnchor="middle"
                    className="fill-zinc-300"
                    style={{ fontSize: 8, fontFamily: "ui-monospace, monospace" }}
                  >
                    {c.name.length > 14 ? `${c.name.slice(0, 12)}…` : c.name}
                  </text>
                </g>
              );
            });
          })}
        </svg>

        {blast.kind === "loading" && (
          <p className="mt-2 max-w-md text-center text-[11px] leading-relaxed text-zinc-500">
            Loading `graph_payload.bin` into the blast engine (one-time)…
          </p>
        )}
        {blast.kind === "ready" && callers.length === 0 && (
          <p className="mt-2 max-w-md text-center text-[11px] leading-relaxed text-zinc-500">
            No callers within depth {depth} for {payload?.seed_name || seedName}.
          </p>
        )}
        {blast.kind === "ready" && truncated && (
          <p className="mt-1 text-[10px] text-zinc-600">
            Showing first {MAX_CALLERS_DRAWN} callers — raise filters or lower depth to focus.
          </p>
        )}
      </div>
    </div>
  );
}
