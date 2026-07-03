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
import { componentColors, layoutForceAtlas2, shortGraphLabel } from "./graphLayout";
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
];

export function GraphView({
  communityOnly,
  sourceNodeCount,
  wasmReady,
  expand,
}: GraphViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);
  const highlightRef = useRef<{ hover: string | null; selected: string | null }>({
    hover: null,
    selected: null,
  });
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
  const [inspectorOpen, setInspectorOpen] = useState(true);

  const refreshHighlight = useCallback((hoverId: string | null, selectedId: string | null) => {
    highlightRef.current = { hover: hoverId, selected: selectedId };
    sigmaRef.current?.refresh();
  }, []);

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
        return renderMetagraph(filteredMeta, container, sigmaRef, highlightRef, showCalls, {
          setHover: (n) => {
            setHover(n);
            refreshHighlight(n ? String(n.id) : null, highlightRef.current.selected);
          },
          setSelected: (n) => {
            setSelected(n);
            refreshHighlight(highlightRef.current.hover, n ? String(n.id) : null);
          },
          onDrill: (m) => void drillInto(m),
        });
      }
      if (level === "subgraph" && subgraph) {
        return renderSubgraph(
          subgraph,
          container,
          sigmaRef,
          highlightRef,
          (n) => {
            setSubHover(n);
            refreshHighlight(n ? String(n.index) : null, highlightRef.current.selected);
          },
          showCalls,
        );
      }
    });
  }, [filteredMeta, level, subgraph, showCalls, search, refreshHighlight]);

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
        highlightRef.current = { hover: null, selected: null };
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
    highlightRef.current = { hover: null, selected: null };
  };

  const fitView = () => {
    sigmaRef.current?.getCamera().animatedReset({ duration: 300 });
  };

  const zoom = (ratio: number) => {
    const cam = sigmaRef.current?.getCamera();
    if (!cam) return;
    cam.animate({ ratio: cam.ratio * ratio }, { duration: 200 });
  };

  return (
    <div class="graph-panel h-100">
      <div class="graph-toolbar border-bottom bg-white px-2 px-md-3 py-2 flex-shrink-0">
        <div class="d-flex flex-wrap align-items-center gap-2">
          <div class="d-flex align-items-center gap-2 flex-shrink-0">
            {level === "subgraph" ? (
              <button type="button" class="btn btn-outline-secondary btn-sm" onClick={backToMeta}>
                ← Packages
              </button>
            ) : (
              <span class="badge text-bg-primary">
                {communityOnly ? "Community view" : "Package graph"}
              </span>
            )}
            <div class="btn-group btn-group-sm" role="group" aria-label="Zoom">
              <button type="button" class="btn btn-outline-secondary" onClick={() => zoom(0.75)} title="Zoom in">
                +
              </button>
              <button type="button" class="btn btn-outline-secondary" onClick={fitView} title="Fit view">
                ⊡
              </button>
              <button type="button" class="btn btn-outline-secondary" onClick={() => zoom(1.33)} title="Zoom out">
                −
              </button>
            </div>
          </div>
          <input
            type="search"
            class="form-control form-control-sm graph-search"
            placeholder="Filter packages…"
            value={search}
            onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
          />
          <div class="graph-toolbar-filters flex-grow-1 min-w-0">
            <NodeTypeFilter mask={typeMask} onChange={setTypeMask} disabled={!wasmReady || level === "metagraph"} />
          </div>
          <div class="d-flex align-items-center gap-2 flex-shrink-0">
            <div class="form-check form-switch mb-0">
              <input
                class="form-check-input"
                type="checkbox"
                id="show-calls"
                checked={showCalls}
                onChange={(e) => setShowCalls((e.target as HTMLInputElement).checked)}
              />
              <label class="form-check-label small" for="show-calls">
                Calls
              </label>
            </div>
            <button
              type="button"
              class={`btn btn-sm ${inspectorOpen ? "btn-outline-secondary" : "btn-primary"}`}
              onClick={() => setInspectorOpen((v) => !v)}
            >
              {inspectorOpen ? "Hide panel" : "Inspector"}
            </button>
          </div>
        </div>
        <p class="small text-muted mb-0 mt-1">
          {level === "subgraph" && subgraph
            ? `${drillLabel} · ${subgraph.nodes.length} nodes · ${subgraph.edges.length} edges · scroll/zoom to navigate`
            : filteredMeta
              ? `${filteredMeta.nodes.length} packages · ${filteredMeta.edges.length} cross-package calls · ${sourceNodeCount.toLocaleString()} nodes · hover for labels`
              : loadState}
          {expanding ? " · expanding…" : ""}
        </p>
      </div>

      {error && (
        <div class="alert alert-danger py-2 small mx-2 mt-2 mb-0" role="alert">
          {error}
        </div>
      )}

      <div class="graph-body flex-grow-1 min-h-0">
        <div class={`graph-stage ${inspectorOpen ? "graph-stage--with-inspector" : ""}`}>
          <div class="graph-canvas-wrap">
            <div class="sigma-host" ref={containerRef} />
          </div>
          <div class="graph-legend px-2 py-1 bg-white small d-flex flex-wrap gap-2 gap-md-3 border-top">
            {level === "metagraph" ? (
              <span class="text-muted">Colors = connected package clusters</span>
            ) : (
              LEGEND.map((item) => (
                <span key={item.label}>
                  <span class="legend-dot" style={{ background: item.color }} />
                  {item.label}
                </span>
              ))
            )}
          </div>
        </div>
        {inspectorOpen && (
          <aside class="graph-inspector border-start bg-white">
            <div class="graph-inspector-inner p-3">
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
                    : "Hover or click a package. Double-click or Drill down to expand members."}
                </p>
              )}
            </div>
          </aside>
        )}
      </div>
    </div>
  );
}

function renderMetagraph(
  meta: MetagraphPayload,
  container: HTMLDivElement,
  sigmaRef: { current: Sigma | null },
  highlightRef: { current: { hover: string | null; selected: string | null } },
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
      label: shortGraphLabel(n.label),
      fullLabel: n.label,
      x: n.x,
      y: n.y,
      size: Math.max(6, Math.log(n.size + 1) * 4.5),
      meta: n,
      color: "#6f42c1",
    });
  }

  const edgeList: Array<{ source: string; target: string; weight?: number }> = [];
  if (showCalls) {
    for (const e of meta.edges) {
      if (!visibleIds.has(e.source) || !visibleIds.has(e.target)) continue;
      edgeList.push({ source: String(e.source), target: String(e.target), weight: e.weight });
    }
    addAggregatedEdges(graph, edgeList);
  }

  const colors = componentColors(
    [...graph.nodes()],
    edgeList.map((e) => ({ source: e.source, target: e.target })),
  );
  graph.forEachNode((node) => {
    graph.setNodeAttribute(node, "color", colors.get(node) ?? "#6f42c1");
  });

  layoutForceAtlas2(graph);

  if (sigmaRef.current) {
    sigmaRef.current.kill();
    sigmaRef.current = null;
  }

  const sigma = new Sigma(
    graph,
    container,
    sigmaOptions(highlightRef, (node, data) => ({
      ...data,
      label: data.label as string,
    })),
  );
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
  requestAnimationFrame(() => sigma.getCamera().animatedReset({ duration: 400 }));

  return () => {
    sigma.kill();
    sigmaRef.current = null;
  };
}

function renderSubgraph(
  payload: SubgraphPayload,
  container: HTMLDivElement,
  sigmaRef: { current: Sigma | null },
  highlightRef: { current: { hover: string | null; selected: string | null } },
  setSubHover: (n: SubgraphNode | null) => void,
  showCalls: boolean,
) {
  const graph = new Graph();

  const typeColors: Record<number, string> = {
    0: "#0d6efd",
    1: "#d63384",
    4: "#0dcaf0",
    5: "#dc3545",
  };

  for (const node of payload.nodes) {
    graph.addNode(String(node.index), {
      label: shortGraphLabel(node.name),
      fullLabel: node.name,
      x: Math.random() * 10,
      y: Math.random() * 10,
      size: Math.max(4, Math.log(node.complexity + 2) * 2.5),
      meta: node,
      color: typeColors[node.node_type] ?? "#6c757d",
    });
  }

  const edgeList: Array<{ source: string; target: string; weight?: number }> = [];
  if (showCalls) {
    for (const e of payload.edges) {
      edgeList.push({ source: String(e.source), target: String(e.target), weight: 1 });
    }
    addAggregatedEdges(graph, edgeList);
  }

  layoutForceAtlas2(graph, payload.nodes.length > 200 ? 120 : 180);

  if (sigmaRef.current) {
    sigmaRef.current.kill();
    sigmaRef.current = null;
  }

  const sigma = new Sigma(
    graph,
    container,
    sigmaOptions(highlightRef, (node, data) => ({
      ...data,
      label: data.label as string,
    })),
  );
  sigma.on("enterNode", ({ node }) => {
    setSubHover(graph.getNodeAttribute(node, "meta") as SubgraphNode);
  });
  sigma.on("leaveNode", () => setSubHover(null));
  sigmaRef.current = sigma;
  requestAnimationFrame(() => sigma.getCamera().animatedReset({ duration: 400 }));

  return () => {
    sigma.kill();
    sigmaRef.current = null;
  };
}

function sigmaOptions(
  highlightRef: { current: { hover: string | null; selected: string | null } },
  labelOf: (node: string, data: Record<string, unknown>) => Record<string, unknown>,
) {
  return {
    renderEdgeLabels: false,
    labelFont: "system-ui, sans-serif",
    labelSize: 12,
    labelWeight: "600" as const,
    defaultNodeColor: "#0d6efd",
    defaultEdgeColor: "#c8cdd3",
    labelColor: { color: "#212529" },
    labelRenderedSizeThreshold: 8,
    minCameraRatio: 0.02,
    maxCameraRatio: 20,
    enableEdgeEvents: false,
    nodeReducer(node: string, data: Record<string, unknown>) {
      const { hover, selected } = highlightRef.current;
      const show = node === hover || node === selected;
      const base = labelOf(node, data);
      return {
        ...base,
        label: show ? (base.label as string) : "",
        zIndex: show ? 2 : 0,
        borderColor: show ? "#212529" : undefined,
      };
    },
    edgeReducer(_edge: string, data: Record<string, unknown>) {
      return { ...data, size: (data.size as number) * 0.85, color: "#d0d5db" };
    },
  };
}

function addAggregatedEdges(
  graph: Graph,
  edges: Array<{ source: string; target: string; weight?: number }>,
) {
  const weights = new Map<string, number>();
  for (const e of edges) {
    const from = e.source;
    const to = e.target;
    if (!graph.hasNode(from) || !graph.hasNode(to) || from === to) continue;
    const k = `${from}\t${to}`;
    weights.set(k, (weights.get(k) ?? 0) + (e.weight ?? 1));
  }
  for (const [k, weight] of weights) {
    const [from, to] = k.split("\t");
    if (graph.hasEdge(from, to)) continue;
    graph.addEdge(from, to, {
      size: Math.max(0.35, Math.log(weight + 1) * 0.45),
      color: "#c8cdd3",
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
          <code class="small text-break">{node.label}</code>
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
        <code class="small text-break">{node.name}</code>
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
