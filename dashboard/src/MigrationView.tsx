import Graph from "graphology";
import Sigma from "sigma";
import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import { bundleDataUrl } from "./bundleUrl";
import { ColumnHelp } from "./ColumnHelp";
import { communityColorHex } from "./graphColors";
import { layoutForceAtlas2 } from "./graphLayout";
import { GraphZoomControls } from "./GraphZoomControls";
import { mountSigmaWhenReady } from "./sigmaMount";
import { SIGMA_NODE_PROGRAM_CLASSES } from "./sigmaPrograms";
import { computeMigrationPlan } from "./migration/engine";
import { addLouvainLayoutEdges, migrationEdgeLayoutWeight } from "./migration/layoutWeights";
import { MIGRATION_PRESETS, matchPreset, presetById } from "./migration/presets";
import type {
  MigrationGraphPayload,
  MigrationOrderMode,
  MigrationPlanPayload,
  MigrationPlanStep,
  MigrationPresetId,
  MigrationWeights,
} from "./migration/types";

const MIN_NODE_SIZE = 4;
const MAX_NODE_SIZE = 28;
const GRAPH_UPDATE_DEBOUNCE_MS = 200;
const MIGRATION_PAGE_SIZE = 25;

const MIGRATION_COLUMN_TOOLTIPS = {
  step: "Row number in the current roadmap sort (Scheduled step or Priority rank).",
  schedule_step:
    "Dependency-aware order: callees are scheduled before callers. Lower numbers migrate earlier.",
  priority_rank:
    "Score-only rank: highest priority score is rank 1. Ignores call dependencies.",
  label: "Package / module label derived from source file paths (Java package, Rust module, etc.).",
  priority_score:
    "Weighted score: α·PageRank + β·Harmonic − γ·Max blast (each metric min–max normalized across packages).",
  avg_pagerank: "Average PageRank centrality of functions in this package.",
  avg_harmonic: "Average harmonic centrality of functions in this package.",
  max_blast: "Maximum blast radius among functions in this package (higher = more downstream impact).",
} as const;

function useDebouncedValue<T>(value: T, delayMs: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), delayMs);
    return () => clearTimeout(t);
  }, [value, delayMs]);
  return debounced;
}

export function MigrationView() {
  const [graph, setGraph] = useState<MigrationGraphPayload | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [preset, setPreset] = useState<MigrationPresetId>("hybrid_default");
  const [weights, setWeights] = useState<MigrationWeights>(presetById("hybrid_default").weights);
  const [orderMode, setOrderMode] = useState<MigrationOrderMode>("scheduled");

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("migration_graph.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`migration_graph.json HTTP ${r.status}`);
        return r.json() as Promise<MigrationGraphPayload>;
      })
      .then((data) => {
        if (!cancelled) setGraph(data);
      })
      .catch((e) => {
        if (!cancelled) {
          setLoadError(e instanceof Error ? e.message : String(e));
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const plan: MigrationPlanPayload | null = useMemo(() => {
    if (!graph || graph.communities.length === 0) return null;
    return computeMigrationPlan(graph, preset, weights, orderMode);
  }, [graph, preset, weights, orderMode]);

  const applyPreset = useCallback((id: MigrationPresetId) => {
    if (id === "custom") return;
    const p = presetById(id);
    setPreset(id);
    setWeights({ ...p.weights });
  }, []);

  const updateWeight = useCallback(
    (key: keyof MigrationWeights, value: number) => {
      const next = { ...weights, [key]: value };
      setWeights(next);
      setPreset(matchPreset(next));
    },
    [weights],
  );

  if (loadError) {
    return (
      <div class="p-4">
        <div class="alert alert-warning mb-0">{loadError}</div>
      </div>
    );
  }

  if (!graph) {
    return <div class="p-4 text-muted small">Loading migration graph…</div>;
  }

  if (graph.communities.length === 0) {
    return (
      <div class="p-4">
        <p class="text-muted mb-0">
          No community migration data available. Re-run <code>rbuilder discover</code> on this
          repository.
        </p>
      </div>
    );
  }

  return (
    <div class="migration-view d-flex flex-column h-100 min-h-0">
      <div class="flex-grow-1 min-h-0 overflow-auto">
        <div class="migration-roadmap p-3 pb-4">
          <MigrationTuningPanel
            preset={preset}
            weights={weights}
            orderMode={orderMode}
            onPresetChange={applyPreset}
            onWeightChange={updateWeight}
            onOrderModeChange={setOrderMode}
          />

          <MigrationGraphPanel
            graph={graph}
            weights={weights}
            preset={preset}
            orderMode={orderMode}
          />

          {plan && (
            <MigrationRoadmapTable plan={plan} orderMode={orderMode} />
          )}
        </div>
      </div>
    </div>
  );
}

function OrderModeControl({
  orderMode,
  onChange,
}: {
  orderMode: MigrationOrderMode;
  onChange: (mode: MigrationOrderMode) => void;
}) {
  return (
    <select
      class="form-select form-select-sm"
      value={orderMode}
      onChange={(e) => onChange((e.target as HTMLSelectElement).value as MigrationOrderMode)}
      aria-label="Roadmap sort order"
    >
      <option value="scheduled">Scheduled step (dependency-aware)</option>
      <option value="priority">Priority rank (score only)</option>
    </select>
  );
}

function MigrationTuningPanel({
  preset,
  weights,
  orderMode,
  onPresetChange,
  onWeightChange,
  onOrderModeChange,
}: {
  preset: MigrationPresetId;
  weights: MigrationWeights;
  orderMode: MigrationOrderMode;
  onPresetChange: (id: MigrationPresetId) => void;
  onWeightChange: (key: keyof MigrationWeights, value: number) => void;
  onOrderModeChange: (mode: MigrationOrderMode) => void;
}) {
  return (
    <section class="migration-section migration-tuning mb-4">
      <h2 class="h6 fw-semibold mb-3">Metrics &amp; tuning</h2>
      <div class="row g-3 align-items-end">
        <div class="col-12 col-lg-3">
          <label class="form-label small mb-1">Roadmap sort</label>
          <OrderModeControl orderMode={orderMode} onChange={onOrderModeChange} />
        </div>
        <div class="col-12 col-lg-3">
          <label class="form-label small mb-1">Strategy preset</label>
          <select
            class="form-select form-select-sm"
            value={preset}
            aria-label="Strategy preset"
            onChange={(e) => onPresetChange((e.target as HTMLSelectElement).value as MigrationPresetId)}
          >
            {MIGRATION_PRESETS.map((p) => (
              <option key={p.id} value={p.id}>
                {p.label}
              </option>
            ))}
            <option value="custom">Custom</option>
          </select>
        </div>
        <div class="col-12 col-lg-6">
          <div class="row g-2">
            <WeightSlider label="Alpha (PageRank)" value={weights.alpha} onInput={(v) => onWeightChange("alpha", v)} />
            <WeightSlider label="Beta (Harmonic)" value={weights.beta} onInput={(v) => onWeightChange("beta", v)} />
            <WeightSlider
              label="Gamma (Blast Radius)"
              value={weights.gamma}
              onInput={(v) => onWeightChange("gamma", v)}
            />
          </div>
        </div>
      </div>
      <p class="text-muted small mt-3 mb-0">
        Priority = α·PageRank + β·Harmonic − γ·Blast (normalized per community). Adjust weights to
        explore strategies; the graph and table update live.
      </p>
    </section>
  );
}

function WeightSlider({
  label,
  value,
  onInput,
}: {
  label: string;
  value: number;
  onInput: (v: number) => void;
}) {
  return (
    <div class="col-12 col-md-4">
      <label class="form-label small mb-1 d-flex justify-content-between">
        <span>{label}</span>
        <span class="text-muted">{value.toFixed(2)}</span>
      </label>
      <input
        type="range"
        class="form-range"
        min={0}
        max={1}
        step={0.01}
        value={value}
        onInput={(e) => onInput(Number((e.target as HTMLInputElement).value))}
      />
    </div>
  );
}

function MigrationColumnHeader({
  label,
  tooltip,
  active = false,
}: {
  label: string;
  tooltip: string;
  active?: boolean;
}) {
  return (
    <th scope="col" class={active ? "table-primary" : undefined}>
      <div class="functions-sort-header-inner">
        <span>{label}</span>
        <ColumnHelp text={tooltip} placement="below" />
      </div>
    </th>
  );
}

function migrationRowTooltip(column: keyof typeof MIGRATION_COLUMN_TOOLTIPS, row: MigrationPlanStep): string {
  const base = MIGRATION_COLUMN_TOOLTIPS[column];
  switch (column) {
    case "step":
      return `${base} Value: ${row.step}.`;
    case "schedule_step":
      return `${base} Value: ${row.schedule_step}.`;
    case "priority_rank":
      return `${base} Value: ${row.priority_rank}.`;
    case "label":
      return `${base} ${row.label}.`;
    case "priority_score":
      return `${base} Value: ${row.priority_score.toFixed(6)}.`;
    case "avg_pagerank":
      return `${base} Value: ${row.avg_pagerank.toFixed(6)}.`;
    case "avg_harmonic":
      return `${base} Value: ${row.avg_harmonic.toFixed(6)}.`;
    case "max_blast":
      return `${base} Value: ${row.max_blast.toFixed(2)}.`;
    default:
      return base;
  }
}

function MigrationRoadmapTable({
  plan,
  orderMode,
}: {
  plan: MigrationPlanPayload;
  orderMode: MigrationOrderMode;
}) {
  const [page, setPage] = useState(0);

  useEffect(() => {
    setPage(0);
  }, [plan.preset, plan.order_mode, orderMode, plan.steps.length]);

  const pageCount = Math.max(1, Math.ceil(plan.steps.length / MIGRATION_PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const rangeStart = plan.steps.length === 0 ? 0 : safePage * MIGRATION_PAGE_SIZE + 1;
  const rangeEnd = Math.min(plan.steps.length, (safePage + 1) * MIGRATION_PAGE_SIZE);
  const pageRows = plan.steps.slice(safePage * MIGRATION_PAGE_SIZE, rangeEnd);

  return (
    <section class="migration-section migration-table-section">
      <div class="d-flex flex-wrap align-items-center justify-content-between gap-2 mb-2">
        <h2 class="h6 fw-semibold mb-0">{plan.preset_label} packages</h2>
        <span class="badge text-bg-secondary">{plan.steps.length.toLocaleString()} packages</span>
      </div>
      <div class="table-responsive border rounded">
        <table class="table table-sm table-hover align-middle mb-0">
          <thead class="table-light">
            <tr>
              <MigrationColumnHeader label="#" tooltip={MIGRATION_COLUMN_TOOLTIPS.step} />
              <MigrationColumnHeader
                label="Sched."
                tooltip={MIGRATION_COLUMN_TOOLTIPS.schedule_step}
                active={orderMode === "scheduled"}
              />
              <MigrationColumnHeader
                label="Rank"
                tooltip={MIGRATION_COLUMN_TOOLTIPS.priority_rank}
                active={orderMode === "priority"}
              />
              <MigrationColumnHeader label="Package" tooltip={MIGRATION_COLUMN_TOOLTIPS.label} />
              <MigrationColumnHeader label="Priority" tooltip={MIGRATION_COLUMN_TOOLTIPS.priority_score} />
              <MigrationColumnHeader label="Avg PR" tooltip={MIGRATION_COLUMN_TOOLTIPS.avg_pagerank} />
              <MigrationColumnHeader label="Avg Harm" tooltip={MIGRATION_COLUMN_TOOLTIPS.avg_harmonic} />
              <MigrationColumnHeader label="Max Blast" tooltip={MIGRATION_COLUMN_TOOLTIPS.max_blast} />
            </tr>
          </thead>
          <tbody>
            {pageRows.map((row) => (
              <tr key={row.community_id}>
                <td title={migrationRowTooltip("step", row)}>{row.step}</td>
                <td
                  class={orderMode === "scheduled" ? "fw-semibold" : "text-muted"}
                  title={migrationRowTooltip("schedule_step", row)}
                >
                  {row.schedule_step}
                </td>
                <td
                  class={orderMode === "priority" ? "fw-semibold" : "text-muted"}
                  title={migrationRowTooltip("priority_rank", row)}
                >
                  {row.priority_rank}
                </td>
                <td title={migrationRowTooltip("label", row)}>{row.label}</td>
                <td title={migrationRowTooltip("priority_score", row)}>{row.priority_score.toFixed(4)}</td>
                <td title={migrationRowTooltip("avg_pagerank", row)}>{row.avg_pagerank.toFixed(4)}</td>
                <td title={migrationRowTooltip("avg_harmonic", row)}>{row.avg_harmonic.toFixed(4)}</td>
                <td title={migrationRowTooltip("max_blast", row)}>{row.max_blast.toFixed(1)}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {plan.steps.length > 0 && (
          <div class="function-list-pagination px-3 py-2 border-top d-flex align-items-center justify-content-between gap-2">
            <button
              type="button"
              class="btn btn-sm btn-outline-secondary"
              disabled={safePage <= 0}
              aria-label="Previous page"
              onClick={() => setPage((p) => Math.max(0, p - 1))}
            >
              Prev
            </button>
            <span class="small text-muted text-center flex-grow-1">
              {rangeStart}–{rangeEnd} of {plan.steps.length.toLocaleString()}
              {pageCount > 1 && ` · ${safePage + 1}/${pageCount}`}
            </span>
            <button
              type="button"
              class="btn btn-sm btn-outline-secondary"
              disabled={safePage >= pageCount - 1}
              aria-label="Next page"
              onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
            >
              Next
            </button>
          </div>
        )}
      </div>
    </section>
  );
}

function scoreSize(score: number, minScore: number, maxScore: number): number {
  const t = maxScore > minScore ? (score - minScore) / (maxScore - minScore) : 0.5;
  return MIN_NODE_SIZE + t * (MAX_NODE_SIZE - MIN_NODE_SIZE);
}

function MigrationGraphPanel({
  graph,
  weights,
  preset,
  orderMode,
}: {
  graph: MigrationGraphPayload;
  weights: MigrationWeights;
  preset: MigrationPresetId;
  orderMode: MigrationOrderMode;
}) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);
  const graphRef = useRef<Graph | null>(null);
  const [graphBusy, setGraphBusy] = useState(false);

  const tuningKey = useMemo(
    () => JSON.stringify({ weights, preset, orderMode }),
    [weights, preset, orderMode],
  );
  const debouncedTuningKey = useDebouncedValue(tuningKey, GRAPH_UPDATE_DEBOUNCE_MS);

  const livePlan = useMemo(() => {
    return computeMigrationPlan(graph, preset, weights, orderMode);
  }, [graph, preset, weights, orderMode]);

  const scoreByCommunity = useMemo(() => {
    const map = new Map<number, number>();
    for (const step of livePlan.steps) {
      map.set(step.community_id, step.priority_score);
    }
    return map;
  }, [livePlan]);

  const scoreRange = useMemo(() => {
    const scores = [...scoreByCommunity.values()];
    return {
      min: scores.length ? Math.min(...scores) : 0,
      max: scores.length ? Math.max(...scores) : 1,
    };
  }, [scoreByCommunity]);

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    let disposed = false;
    setGraphBusy(true);

    return mountSigmaWhenReady(container, () => {
      if (disposed) return;

      sigmaRef.current?.kill();
      sigmaRef.current = null;
      graphRef.current = null;

      const nodeById = new Map(graph.communities.map((n) => [n.id, n]));

      const g = new Graph();
      for (const node of graph.communities) {
        const score = scoreByCommunity.get(node.id) ?? 0;
        g.addNode(String(node.id), {
          label: node.label,
          x: Math.random(),
          y: Math.random(),
          size: scoreSize(score, scoreRange.min, scoreRange.max),
          color: communityColorHex(node.louvain_community_id ?? node.id),
        });
      }

      for (const edge of graph.edges) {
        const key = `${edge.source}->${edge.target}`;
        if (g.hasNode(String(edge.source)) && g.hasNode(String(edge.target)) && !g.hasEdge(key)) {
          const source = nodeById.get(edge.source);
          const target = nodeById.get(edge.target);
          const layoutWeight = migrationEdgeLayoutWeight(source, target, edge.weight);
          g.addEdgeWithKey(key, String(edge.source), String(edge.target), {
            size: Math.max(0.5, edge.weight * 0.15),
            color: "#c8cdd3",
            weight: layoutWeight,
          });
        }
      }

      addLouvainLayoutEdges(g, graph.communities);

      layoutForceAtlas2(g, graph.communities.length > 120 ? 160 : graph.communities.length > 40 ? 200 : 140);

      const sigma = new Sigma(g, container, {
        renderEdgeLabels: false,
        labelFont: "system-ui, sans-serif",
        labelSize: 11,
        defaultNodeColor: "#0d6efd",
        defaultEdgeColor: "#c8cdd3",
        labelColor: { color: "#212529" },
        labelRenderedSizeThreshold: 6,
        nodeProgramClasses: SIGMA_NODE_PROGRAM_CLASSES,
        edgeReducer: (_edge, data) => (data.layoutOnly ? { ...data, size: 0, color: "transparent" } : data),
      });

      graphRef.current = g;
      sigmaRef.current = sigma;
      setGraphBusy(false);
      requestAnimationFrame(() => sigma.getCamera().animatedReset({ duration: 400 }));

      return () => {
        disposed = true;
        sigma.kill();
        sigmaRef.current = null;
        graphRef.current = null;
      };
    });
  }, [graph]);

  useEffect(() => {
    const g = graphRef.current;
    const sigma = sigmaRef.current;
    if (!g || !sigma) return;

    setGraphBusy(true);
    try {
      g.forEachNode((nodeId) => {
        const cid = Number(nodeId);
        const score = scoreByCommunity.get(cid) ?? 0;
        g.setNodeAttribute(nodeId, "size", scoreSize(score, scoreRange.min, scoreRange.max));
      });
      sigma.refresh();
    } finally {
      setGraphBusy(false);
    }
  }, [debouncedTuningKey, scoreByCommunity, scoreRange.min, scoreRange.max]);

  return (
    <section class="migration-section migration-graph-section mb-4">
      <h2 class="h6 fw-semibold mb-2">Package graph</h2>
      <p class="small text-muted mb-2">
        Node color = Louvain cluster · size = priority score · strong layout pull within clusters,
        weak pull across clusters · sizes update live when weights change.
      </p>
      <div class="migration-graph-panel analysis-graph-canvas-wrap border rounded">
        <div class="graph-canvas-wrap position-relative">
          {graphBusy && (
            <div class="position-absolute top-0 start-0 m-2 badge text-bg-secondary" style="z-index: 5">
              Updating graph…
            </div>
          )}
          <div class="sigma-host" ref={containerRef} />
          <GraphZoomControls sigmaRef={sigmaRef} />
        </div>
      </div>
    </section>
  );
}
