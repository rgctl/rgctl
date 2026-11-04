/**
 * Migration room — deterministic schedule from coupling, centrality and blast radius.
 *
 * Self-contained: tuning + three.js community graph + roadmap table. No Focus / Architecture links.
 */
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { MigrationPlanStep, MigrationWeights } from "../../migration/types";
import {
  DEFAULT_WEIGHTS,
  MIGRATION_PRESETS,
  useMigrationSchedule,
} from "../hooks/useMigrationSchedule";
import type { MigrationLayoutNode } from "../migrationLayout";
import { Chip, Panel, Unavailable } from "../shell";
import type { MigrationScene } from "../universe/MigrationScene";

const DISCOVER_HINT =
  "`rgctl discover . --with-universe --with-harmonic` (harmonic feeds the β term; without it scores stay near zero)";

export function MigrationRoom() {
  const {
    state,
    weights,
    presetId,
    orderMode,
    setWeights,
    applyPreset,
    setOrderMode,
  } = useMigrationSchedule();

  const [hover, setHover] = useState<MigrationLayoutNode | null>(null);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [showEdges, setShowEdges] = useState(true);
  const [sceneFailed, setSceneFailed] = useState<string | null>(null);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const sceneRef = useRef<MigrationScene | null>(null);

  useEffect(() => {
    if (state.kind !== "ready") return;
    const layout = showEdges
      ? state.layout
      : { ...state.layout, edges: [] as typeof state.layout.edges };
    const host = hostRef.current;
    if (!host) return;
    let disposed = false;
    let scene: MigrationScene | null = null;
    let observer: ResizeObserver | null = null;

    void (async () => {
      let module: typeof import("../universe/MigrationScene");
      try {
        module = await import("../universe/MigrationScene");
      } catch (error) {
        setSceneFailed((error as Error).message);
        return;
      }
      if (disposed) return;
      if (!module.webglAvailable()) {
        setSceneFailed("this browser could not create a WebGL context");
        return;
      }
      try {
        scene = new module.MigrationScene(host);
      } catch (error) {
        setSceneFailed((error as Error).message);
        return;
      }
      scene.setCallbacks(setHover, (n) => setSelectedId(n?.communityId ?? null));
      scene.attachPointer(host);
      scene.setLayout(layout);
      scene.start();
      sceneRef.current = scene;
      observer = new ResizeObserver(() => {
        if (!host || !scene) return;
        scene.resize(host.clientWidth, host.clientHeight);
      });
      observer.observe(host);
    })();

    return () => {
      disposed = true;
      observer?.disconnect();
      scene?.dispose();
      sceneRef.current = null;
    };
  }, [state, showEdges]);

  if (state.kind === "loading") {
    return (
      <Panel title="Migration" subtitle="reading the bundle">
        <p className="text-[11px] text-zinc-500">Reading `migration_graph.json`…</p>
      </Panel>
    );
  }

  if (state.kind === "missing") {
    return (
      <Panel title="Migration" subtitle="no migration graph in this bundle">
        <Unavailable
          tag="not exported"
          what="This bundle carries no migration graph"
          why={`The export produced no usable \`migration_graph.json\`, so there is nothing to schedule. It is written by ${DISCOVER_HINT}. A bundle indexed without it shows this absence rather than an empty table.`}
        />
      </Panel>
    );
  }

  if (state.kind === "empty") {
    return (
      <Panel title="Migration" subtitle="graph present, no communities">
        <Unavailable
          tag="no communities"
          what="The migration graph has zero communities"
          why={`\`migration_graph.json\` is present (schema ${state.graph.schema_version}, mode ${state.graph.mode ?? "—"}) but \`communities\` is empty — the index ran, yet Fast-Leiden / package-macro export produced no migration rows. Re-run ${DISCOVER_HINT} on a corpus with analyzable packages.`}
        />
      </Panel>
    );
  }

  const { graph, plan, layout } = state;

  return (
    <div className="flex min-h-0 flex-1 flex-col gap-2" data-testid="migration-room">
      <Panel
        title="Migration"
        subtitle="what order should we move it?"
        right={<Chip tone="zinc">{graph.mode ?? "package_macro"}</Chip>}
      >
        <p className="text-[11.5px] leading-relaxed text-zinc-400">
          A deterministic total order over{" "}
          <b className="text-zinc-200">{graph.communities.length}</b> communities from coupling
          edges, PageRank (α), harmonic centrality (β), and blast radius (γ). Weights recompute the
          plan in the browser — no re-discover. Harmonic zeros mean discover ran without{" "}
          <code className="text-zinc-500">--with-harmonic</code>.
        </p>
      </Panel>

      <div className="grid min-h-0 flex-1 gap-2 lg:grid-cols-[minmax(0,1fr)_320px]">
        <div className="flex min-h-0 flex-col gap-2">
          <div className="relative min-h-[420px] flex-1 overflow-hidden rounded-lg border border-white/8 bg-[#060a12]">
            <div ref={hostRef} className="absolute inset-0" data-testid="migration-canvas" />
            {sceneFailed && (
              <div className="absolute inset-0 flex items-center justify-center p-4">
                <Unavailable
                  tag="drawing unavailable"
                  what="The drawing could not be started"
                  why={`${sceneFailed}. Tuning and the roadmap table below are unaffected.`}
                />
              </div>
            )}
            <div className="pointer-events-none absolute left-2 top-2 flex gap-1">
              <Chip tone="blue">{orderMode === "scheduled" ? "schedule order" : "priority rank"}</Chip>
              <Chip tone="zinc">{layout.nodes.length} nodes</Chip>
              <Chip tone="zinc">{showEdges ? `${layout.edges.length} edges` : "edges hidden"}</Chip>
            </div>
            <div className="absolute bottom-2 right-2 flex gap-1">
              <Toggle on={showEdges} onClick={() => setShowEdges((v) => !v)}>
                edges {showEdges ? "on" : "off"}
              </Toggle>
            </div>
            {hover && (
              <div className="pointer-events-none absolute bottom-2 left-2 max-w-[420px] rounded border border-white/12 bg-black/80 p-2">
                <div className="text-[11px] font-semibold text-zinc-100">{hover.label}</div>
                <div className="text-[10.5px] text-zinc-400">
                  band {hover.orderBand} · score {hover.score.toFixed(3)} · members {hover.memberCount}
                </div>
              </div>
            )}
          </div>

          <RoadmapTable
            steps={plan.steps}
            orderMode={orderMode}
            selectedId={selectedId}
            onSelect={setSelectedId}
          />
        </div>

        <div className="flex min-h-0 flex-col gap-2 overflow-y-auto">
          <Tuning
            weights={weights}
            presetId={presetId}
            orderMode={orderMode}
            onWeights={setWeights}
            onPreset={applyPreset}
            onOrderMode={setOrderMode}
          />
        </div>
      </div>
    </div>
  );
}

function Tuning({
  weights,
  presetId,
  orderMode,
  onWeights,
  onPreset,
  onOrderMode,
}: {
  weights: MigrationWeights;
  presetId: string;
  orderMode: "scheduled" | "priority";
  onWeights: (w: MigrationWeights) => void;
  onPreset: (id: (typeof MIGRATION_PRESETS)[number]["id"]) => void;
  onOrderMode: (m: "scheduled" | "priority") => void;
}) {
  const setAxis = (key: keyof MigrationWeights, value: number) => {
    onWeights({ ...weights, [key]: value });
  };

  return (
    <Panel title="Tuning" subtitle="presets and weights">
      <div className="mb-2 flex flex-wrap gap-1">
        {MIGRATION_PRESETS.map((p) => (
          <button
            key={p.id}
            type="button"
            onClick={() => onPreset(p.id)}
            aria-pressed={presetId === p.id}
            className={`rounded border px-1.5 py-0.5 text-[10px] ${
              presetId === p.id
                ? "border-sky-500/40 bg-sky-500/12 text-sky-200"
                : "border-white/10 text-zinc-400 hover:text-zinc-200"
            }`}
          >
            {p.label}
          </button>
        ))}
        {presetId === "custom" && <Chip tone="amber">custom</Chip>}
      </div>

      <WeightSlider label="α PageRank" value={weights.alpha} onChange={(v) => setAxis("alpha", v)} />
      <WeightSlider label="β Harmonic" value={weights.beta} onChange={(v) => setAxis("beta", v)} />
      <WeightSlider label="γ Blast" value={weights.gamma} onChange={(v) => setAxis("gamma", v)} />

      <div className="mt-2 flex gap-1">
        <Toggle on={orderMode === "scheduled"} onClick={() => onOrderMode("scheduled")}>
          scheduled
        </Toggle>
        <Toggle on={orderMode === "priority"} onClick={() => onOrderMode("priority")}>
          priority
        </Toggle>
        <button
          type="button"
          className="ml-auto text-[10px] text-zinc-500 hover:text-zinc-300"
          onClick={() => onWeights({ ...DEFAULT_WEIGHTS })}
        >
          reset
        </button>
      </div>
    </Panel>
  );
}

function WeightSlider({
  label,
  value,
  onChange,
}: {
  label: string;
  value: number;
  onChange: (v: number) => void;
}) {
  return (
    <label className="mb-1.5 flex items-center gap-2 text-[11px] text-zinc-400">
      <span className="w-24 shrink-0">{label}</span>
      <input
        type="range"
        min={0}
        max={1}
        step={0.01}
        value={value}
        onChange={(e) => onChange(Number(e.target.value))}
        className="min-w-0 flex-1"
      />
      <span className="w-10 tabular-nums text-zinc-300">{value.toFixed(2)}</span>
    </label>
  );
}

function RoadmapTable({
  steps,
  orderMode,
  selectedId,
  onSelect,
}: {
  steps: MigrationPlanStep[];
  orderMode: "scheduled" | "priority";
  selectedId: number | null;
  onSelect: (id: number) => void;
}) {
  return (
    <Panel title="Roadmap" subtitle={`${steps.length} steps · ${orderMode}`}>
      <div className="overflow-x-auto">
        <table className="w-full border-collapse text-left text-[11px]" data-testid="migration-roadmap">
          <thead>
            <tr className="text-[10px] uppercase tracking-wider text-zinc-600">
              <th className="px-1 py-1">#</th>
              <th className="px-1 py-1">label</th>
              <th className="px-1 py-1">sched</th>
              <th className="px-1 py-1">prio</th>
              <th className="px-1 py-1">score</th>
              <th className="px-1 py-1">PR</th>
              <th className="px-1 py-1">harm</th>
              <th className="px-1 py-1">blast</th>
            </tr>
          </thead>
          <tbody>
            {steps.map((s) => (
              <tr
                key={s.community_id}
                onClick={() => onSelect(s.community_id)}
                className={`cursor-pointer border-t border-white/5 ${
                  selectedId === s.community_id ? "bg-sky-500/10 text-sky-100" : "text-zinc-300 hover:bg-white/[0.03]"
                }`}
              >
                <td className="px-1 py-1 tabular-nums text-zinc-500">{s.step}</td>
                <td className="max-w-[140px] truncate px-1 py-1 font-mono" title={s.label}>
                  {s.label}
                </td>
                <td className="px-1 py-1 tabular-nums">{s.schedule_step}</td>
                <td className="px-1 py-1 tabular-nums">{s.priority_rank}</td>
                <td className="px-1 py-1 tabular-nums">{s.priority_score.toFixed(3)}</td>
                <td className="px-1 py-1 tabular-nums text-zinc-500">{s.avg_pagerank.toFixed(3)}</td>
                <td className="px-1 py-1 tabular-nums text-zinc-500">{s.avg_harmonic.toFixed(3)}</td>
                <td className="px-1 py-1 tabular-nums text-zinc-500">{s.max_blast.toFixed(2)}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </Panel>
  );
}

function Toggle({ on, onClick, children }: { on: boolean; onClick: () => void; children: ReactNode }) {
  return (
    <button
      type="button"
      onClick={onClick}
      aria-pressed={on}
      className={`rounded border px-1.5 py-0.5 text-[10px] leading-none ${
        on ? "border-sky-500/40 bg-sky-500/12 text-sky-200" : "border-white/10 text-zinc-400 hover:text-zinc-200"
      }`}
    >
      {children}
    </button>
  );
}
