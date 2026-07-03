import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import { NodeTypeFilter } from "./NodeTypeFilter";
import type {
  MetagraphPayload,
  Metanode,
  SubgraphNode,
  SubgraphPayload,
} from "./types";
import { DEFAULT_GRAPH_TYPE_MASK } from "./types";
import { bundleDataUrl } from "./bundleUrl";
import { mountSigmaWhenReady } from "./sigmaMount";

export interface GraphViewProps {
  communityOnly: boolean;
  sourceNodeCount: number;
  wasmReady: boolean;
  expand: (indices: number[], typeMask: number) => Promise<SubgraphPayload>;
}

type ViewLevel = "metagraph" | "subgraph";

const LEGEND = [
  { label: "Function", color: "#0d6efd" },
  { label: "Class", color: "#d63384" },
  { label: "Interface", color: "#0dcaf0" },
  { label: "Module", color: "#dc3545" },
  { label: "Package", color: "#6f42c1" },
];

export function GraphView({
  communityOnly,
  sourceNodeCount,
  wasmReady,
  expand,
}: GraphViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);
  const [meta, setMeta] = useState<MetagraphPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<Metanode | null>(null);
  const [selected, setSelected] = useState<Metanode | null>(null);
  const [loadState, setLoadState] = useState("loading");
  const [level, setLevel] = useState<ViewLevel>("metagraph");
  const [subgraph, setSubgraph] = useState<SubgraphPayload | null>(null);
  const [drillLabel, setDrillLabel] = useState<string | null>(null);
  const [typeMask, setTypeMask] = useState(DEFAULT_GRAPH_TYPE_MASK);
  const [expanding, setExpanding] = useState(false);
  const [subHover, setSubHover] = useState<SubgraphNode | null>(null);
  const [search, setSearch] = useState("");
  const [showCalls, setShowCalls] = useState(true);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("metagraph.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`metagraph.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: MetagraphPayload) => {
        if (!cancelled) {
          setMeta(data);
          setLoadState("ready");
        }
      })
      .catch((e) => {
        if (!cancelled) {
          setError(e instanceof Error ? e.message : String(e));
          setLoadState("error");
        }
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const filteredMeta = meta
    ? {
        ...meta,
        nodes: search.trim()
          ? meta.nodes.filter((n) =>
              n.label.toLowerCase().includes(search.trim().toLowerCase()),
            )
          : meta.nodes,
      }
    : null;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    if (level === "metagraph" && !filteredMeta) return;
    if (level === "subgraph" && !subgraph) return;

    return mountSigmaWhenReady(container, () => {
      if (level === "metagraph" && filteredMeta) {
        return renderMetagraph(filteredMeta, container, sigmaRef, showCalls, {
          setHover,
          setSelected,
          onDrill: (m) => void drillInto(m),
        });
      }
      if (level === "subgraph" && subgraph) {
        return renderSubgraph(subgraph, container, sigmaRef, setSubHover, showCalls);
      }
    });
  }, [filteredMeta, level, subgraph, showCalls, search]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => {
      sigmaRef.current?.refresh();
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, [level, subgraph, filteredMeta]);

  const prevMask = useRef(typeMask);

  useEffect(() => {
    if (prevMask.current === typeMask) return;
    prevMask.current = typeMask;
    if (level !== "subgraph" || !drillLabel || !meta || !wasmReady) return;
    const node = meta.nodes.find((n) => n.label === drillLabel);
    if (node) void drillInto(node);
  }, [typeMask]);

  const drillInto = useCallback(
    async (node: Metanode) => {
      if (!wasmReady) {
        setError("WASM engine required for drill-down");
        return;
      }
      const indices = node.member_indices ?? [];
      if (indices.length === 0) {
        setError("No member indices for this package (re-run discover)");
        return;
      }
      setExpanding(true);
      setError(null);
      try {
        const payload = await expand(indices, typeMask);
        setSubgraph(payload);
        setDrillLabel(node.label);
        setLevel("subgraph");
        setSelected(null);
        setHover(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setExpanding(false);
      }
    },
    [wasmReady, expand, typeMask],
  );

  const backToMeta = () => {
    setLevel("metagraph");
    setSubgraph(null);
    setDrillLabel(null);
    setSubHover(null);
  };

  const resetView = () => {
    sigmaRef.current?.getCamera().animatedReset({ duration: 300 });
  };

  return (
    <div class="graph-panel h-100">
      <div class="border-bottom bg-white px-3 py-2 flex-shrink-0">
        <div class="row g-2 align-items-center">
          <div class="col-lg-auto">
            {level === "subgraph" ? (
              <button type="button" class="btn btn-outline-secondary btn-sm" onClick={backToMeta}>
                ← Packages
              </button>
            ) : (
              <span class="badge text-bg-primary">
                {communityOnly ? "Community view" : "Package metagraph"}
              </span>
            )}
          </div>
          <div class="col-lg-auto">
            <label class="form-label small text-muted mb-0 me-1">Search</label>
            <input
              type="search"
              class="form-control form-control-sm"
              placeholder="Search by name…"
              value={search}
              onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
              style="min-width: 160px"
            />
          </div>
          <div class="col-lg">
            <NodeTypeFilter mask={typeMask} onChange={setTypeMask} disabled={!wasmReady} />
          </div>
          <div class="col-lg-auto d-flex align-items-center gap-3">
            <div class="form-check form-switch mb-0">
              <input
                class="form-check-input"
                type="checkbox"
                id="show-calls"
                checked={showCalls}
                onChange={(e) => setShowCalls((e.target as HTMLInputElement).checked)}
              />
              <label class="form-check-label small" for="show-calls">
                Show Calls
              </label>
            </div>
            <button type="button" class="btn btn-primary btn-sm" onClick={resetView}>
              Reset View
            </button>
          </div>
        </div>
        <p class="small text-muted mb-0 mt-2">
          {level === "subgraph" && subgraph
            ? `${drillLabel} · ${subgraph.nodes.length} nodes · ${subgraph.edges.length} call edges`
            : filteredMeta
              ? `${filteredMeta.nodes.length} metanodes · ${filteredMeta.edges.length} cross-package edges · ${sourceNodeCount.toLocaleString()} source nodes`
              : loadState}
          {expanding ? " · expanding…" : ""}
        </p>
      </div>

      {error && (
        <div class="alert alert-danger py-2 small mx-3 mt-2 mb-0" role="alert">
          {error}
        </div>
      )}

      <div class="row g-0 graph-body flex-grow-1 h-100">
        <div class="col-lg-9 graph-main-col h-100">
          <div class="graph-canvas-wrap flex-grow-1" ref={containerRef} />
          <div class="graph-legend px-3 py-2 bg-white small d-flex flex-wrap gap-3 border-top">
            {LEGEND.map((item) => (
              <span key={item.label}>
                <span class="legend-dot" style={{ background: item.color }} />
                {item.label}
              </span>
            ))}
          </div>
        </div>
        <div class="col-lg-3 border-start bg-white h-100 overflow-hidden">
          <aside class="graph-inspector p-3 h-100">
            <h3 class="h6 mb-3">Inspector</h3>
            {level === "subgraph" && subHover ? (
              <SubgraphDetail node={subHover} />
            ) : (selected ?? hover) ? (
              <MetanodeDetail
                node={selected ?? hover!}
                isSelected={!!selected}
                onDrill={wasmReady ? () => void drillInto(selected ?? hover!) : undefined}
                drilling={expanding}
              />
            ) : (
              <p class="text-muted small mb-0">
                {level === "subgraph"
                  ? "Hover a node for details."
                  : "Hover or click a metanode. Double-click or Drill down to expand."}
              </p>
            )}
          </aside>
        </div>
      </div>
    </div>
  );
}

function renderMetagraph(
  meta: MetagraphPayload,
  container: HTMLDivElement,
  sigmaRef: { current: Sigma | null },
  showCalls: boolean,
  handlers: {
    setHover: (n: Metanode | null) => void;
    setSelected: (n: Metanode | null) => void;
    onDrill: (n: Metanode) => void;
  },
) {
  const graph = new Graph();
  const visibleIds = new Set(meta.nodes.map((n) => n.id));

  for (const n of meta.nodes) {
    graph.addNode(String(n.id), {
      label: n.label,
      x: n.x,
      y: n.y,
      size: Math.max(4, Math.log(n.size + 1) * 5),
      meta: n,
      color: "#6f42c1",
    });
  }
  if (showCalls) {
    addAggregatedEdges(
      graph,
      meta.edges
        .filter((e) => visibleIds.has(e.source) && visibleIds.has(e.target))
        .map((e) => ({ source: e.source, target: e.target, weight: e.weight })),
    );
  }

  if (sigmaRef.current) {
    sigmaRef.current.kill();
    sigmaRef.current = null;
  }

  const sigma = new Sigma(graph, container, sigmaOptions());
  sigma.on("enterNode", ({ node }) => {
    handlers.setHover(graph.getNodeAttribute(node, "meta") as Metanode);
  });
  sigma.on("leaveNode", () => handlers.setHover(null));
  sigma.on("clickNode", ({ node }) => {
    handlers.setSelected(graph.getNodeAttribute(node, "meta") as Metanode);
  });
  sigma.on("clickStage", () => handlers.setSelected(null));
  sigma.on("doubleClickNode", ({ node }) => {
    const m = graph.getNodeAttribute(node, "meta") as Metanode;
    handlers.setSelected(m);
    handlers.onDrill(m);
  });

  sigmaRef.current = sigma;
  return () => {
    sigma.kill();
    sigmaRef.current = null;
  };
}

function renderSubgraph(
  payload: SubgraphPayload,
  container: HTMLDivElement,
  sigmaRef: { current: Sigma | null },
  setSubHover: (n: SubgraphNode | null) => void,
  showCalls: boolean,
) {
  const graph = new Graph();
  const n = payload.nodes.length || 1;
  const radius = Math.max(30, Math.sqrt(n) * 8);

  const typeColors: Record<number, string> = {
    0: "#0d6efd",
    1: "#d63384",
    4: "#0dcaf0",
    5: "#dc3545",
  };

  for (let i = 0; i < payload.nodes.length; i++) {
    const node = payload.nodes[i];
    const angle = (2 * Math.PI * i) / n;
    graph.addNode(String(node.index), {
      label: node.name,
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      size: Math.max(3, Math.log(node.complexity + 2) * 2),
      meta: node,
      color: typeColors[node.node_type] ?? "#6c757d",
    });
  }
  if (showCalls) {
    addAggregatedEdges(
      graph,
      payload.edges.map((e) => ({ source: e.source, target: e.target, weight: 1 })),
    );
  }

  if (sigmaRef.current) {
    sigmaRef.current.kill();
    sigmaRef.current = null;
  }

  const sigma = new Sigma(graph, container, sigmaOptions());
  sigma.on("enterNode", ({ node }) => {
    setSubHover(graph.getNodeAttribute(node, "meta") as SubgraphNode);
  });
  sigma.on("leaveNode", () => setSubHover(null));
  sigma.getCamera().animatedReset({ duration: 300 });

  sigmaRef.current = sigma;
  return () => {
    sigma.kill();
    sigmaRef.current = null;
  };
}

function sigmaOptions() {
  return {
    renderEdgeLabels: false,
    labelFont: "system-ui, sans-serif",
    labelSize: 11,
    labelWeight: "500" as const,
    defaultNodeColor: "#0d6efd",
    defaultEdgeColor: "#adb5bd",
    minCameraRatio: 0.08,
    maxCameraRatio: 10,
  };
}

function addAggregatedEdges(
  graph: Graph,
  edges: Array<{ source: number; target: number; weight?: number }>,
) {
  const weights = new Map<string, number>();
  for (const e of edges) {
    const from = String(e.source);
    const to = String(e.target);
    if (!graph.hasNode(from) || !graph.hasNode(to) || from === to) continue;
    const k = `${from}\t${to}`;
    weights.set(k, (weights.get(k) ?? 0) + (e.weight ?? 1));
  }
  for (const [k, weight] of weights) {
    const [from, to] = k.split("\t");
    if (graph.hasEdge(from, to)) continue;
    graph.addEdge(from, to, {
      size: Math.max(0.5, Math.log(weight + 1)),
      color: "#adb5bd",
    });
  }
}

function MetanodeDetail({
  node,
  isSelected,
  onDrill,
  drilling,
}: {
  node: Metanode;
  isSelected: boolean;
  onDrill?: () => void;
  drilling?: boolean;
}) {
  return (
    <>
      <dl class="row small mb-0">
        <dt class="col-5 text-muted">{isSelected ? "Selected" : "Hover"}</dt>
        <dd class="col-7 mb-1">
          <code class="small">{node.label}</code>
        </dd>
        <dt class="col-5 text-muted">Members</dt>
        <dd class="col-7 mb-1">{node.size.toLocaleString()}</dd>
        <dt class="col-5 text-muted">Functions</dt>
        <dd class="col-7 mb-1">{node.functions.toLocaleString()}</dd>
        <dt class="col-5 text-muted">Classes</dt>
        <dd class="col-7 mb-1">{node.classes.toLocaleString()}</dd>
        <dt class="col-5 text-muted">Avg complexity</dt>
        <dd class="col-7 mb-1">{node.avg_complexity.toFixed(1)}</dd>
      </dl>
      {onDrill && (
        <button
          type="button"
          class="btn btn-primary btn-sm w-100 mt-3"
          disabled={drilling}
          onClick={onDrill}
        >
          {drilling ? "Expanding…" : "Drill down"}
        </button>
      )}
    </>
  );
}

function SubgraphDetail({ node }: { node: SubgraphNode }) {
  return (
    <dl class="row small mb-0">
      <dt class="col-5 text-muted">Name</dt>
      <dd class="col-7 mb-1">
        <code class="small">{node.name}</code>
      </dd>
      <dt class="col-5 text-muted">Type</dt>
      <dd class="col-7 mb-1">{node.node_type_name}</dd>
      <dt class="col-5 text-muted">Complexity</dt>
      <dd class="col-7 mb-1">{node.complexity.toFixed(1)}</dd>
      {node.file_path && (
        <>
          <dt class="col-5 text-muted">File</dt>
          <dd class="col-7 mb-1 text-break">{node.file_path}</dd>
        </>
      )}
    </dl>
  );
}
