import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import { GraphSidebar } from "./GraphSidebar";
import { NodeTypeFilter } from "./NodeTypeFilter";
import type {
  CommunitiesPayload,
  MetagraphPayload,
  Metanode,
  SubgraphNode,
  SubgraphPayload,
} from "./types";
import { DEFAULT_GRAPH_TYPE_MASK } from "./types";
import { bundleDataUrl } from "./bundleUrl";
import {
  buildUndirectedAdjacency,
  deterministicPositions,
  firstMatchingNodeId,
  neighborhoodIds,
  passesFilters,
  type CategoryFilter,
  type GraphFilterState,
} from "./graphExplore";
import { communityColor, layoutForceAtlas2, shortGraphLabel } from "./graphLayout";
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
  const adjacencyRef = useRef<Map<string, Set<string>>>(new Map());
  const filterRef = useRef<GraphFilterState>({
    search: "",
    communityId: null,
    category: "all",
    soloCommunity: false,
  });
  const highlightRef = useRef<{ hover: string | null; selected: string | null }>({
    hover: null,
    selected: null,
  });

  const [meta, setMeta] = useState<MetagraphPayload | null>(null);
  const [communities, setCommunities] = useState<CommunitiesPayload | null>(null);
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
  const [selectedCommunityId, setSelectedCommunityId] = useState<number | null>(null);
  const [category, setCategory] = useState<CategoryFilter>("all");
  const [soloCommunity, setSoloCommunity] = useState(false);

  const refreshGraph = useCallback(() => {
    sigmaRef.current?.refresh();
  }, []);

  const syncFilters = useCallback(() => {
    filterRef.current = {
      search,
      communityId: selectedCommunityId,
      category,
      soloCommunity,
    };
    refreshGraph();
  }, [search, selectedCommunityId, category, soloCommunity, refreshGraph]);

  useEffect(() => {
    syncFilters();
  }, [syncFilters]);

  const refreshHighlight = useCallback(
    (hoverId: string | null, selectedId: string | null) => {
      highlightRef.current = { hover: hoverId, selected: selectedId };
      refreshGraph();
    },
    [refreshGraph],
  );

  useEffect(() => {
    let cancelled = false;
    Promise.all([
      fetch(bundleDataUrl("metagraph.json")).then((r) => {
        if (!r.ok) throw new Error(`metagraph.json HTTP ${r.status}`);
        return r.json() as Promise<MetagraphPayload>;
      }),
      fetch(bundleDataUrl("communities.json"))
        .then((r) => (r.ok ? r.json() : null))
        .catch(() => null),
    ])
      .then(([metaData, communitiesData]) => {
        if (!cancelled) {
          setMeta(metaData);
          setCommunities(communitiesData as CommunitiesPayload | null);
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

  const visibleMetaCount =
    meta?.nodes.filter((n) => passesFilters(n, filterRef.current)).length ?? 0;

  useEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    if (level === "metagraph" && !meta) return;
    if (level === "subgraph" && !subgraph) return;

    return mountSigmaWhenReady(container, () => {
      if (level === "metagraph" && meta) {
        return mountMetagraph(
          meta,
          container,
          sigmaRef,
          adjacencyRef,
          highlightRef,
          filterRef,
          showCalls,
          {
            setHover: (n) => {
              setHover(n);
              refreshHighlight(n ? String(n.id) : null, highlightRef.current.selected);
            },
            setSelected: (n) => {
              setSelected(n);
              refreshHighlight(highlightRef.current.hover, n ? String(n.id) : null);
            },
            onDrill: (m) => void drillInto(m),
          },
        );
      }
      if (level === "subgraph" && subgraph) {
        return mountSubgraph(
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
  }, [meta, level, subgraph, showCalls, refreshHighlight]);

  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(() => refreshGraph());
    ro.observe(el);
    return () => ro.disconnect();
  }, [level, subgraph, meta, refreshGraph]);

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

  const flyToSearch = () => {
    if (!meta || !sigmaRef.current) return;
    const nodeId = firstMatchingNodeId(meta.nodes, filterRef.current);
    if (!nodeId) return;
    const graph = sigmaRef.current.getGraph();
    if (!graph.hasNode(nodeId)) return;
    const x = graph.getNodeAttribute(nodeId, "x") as number;
    const y = graph.getNodeAttribute(nodeId, "y") as number;
    highlightRef.current.selected = nodeId;
    const m = graph.getNodeAttribute(nodeId, "meta") as Metanode;
    setSelected(m);
    refreshHighlight(highlightRef.current.hover, nodeId);
    sigmaRef.current.getCamera().animate({ x, y, ratio: 0.45 }, { duration: 450 });
  };

  return (
    <div class="graph-panel h-100">
      <div class="graph-toolbar graph-toolbar--slim border-bottom bg-white px-2 px-md-3 py-1 flex-shrink-0">
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
          <div class="d-flex flex-grow-1 min-w-0 gap-1">
            <input
              type="search"
              class="form-control form-control-sm graph-search"
              placeholder="Search packages…"
              value={search}
              onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") flyToSearch();
              }}
            />
            <button
              type="button"
              class="btn btn-outline-secondary btn-sm flex-shrink-0"
              title="Fly to first search match"
              onClick={flyToSearch}
            >
              Go
            </button>
          </div>
          {level === "subgraph" && (
            <div class="graph-toolbar-filters flex-grow-1 min-w-0">
              <NodeTypeFilter mask={typeMask} onChange={setTypeMask} disabled={!wasmReady} />
            </div>
          )}
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
          </div>
        </div>
        <p class="small text-muted mb-0 mt-1 graph-toolbar-hint">
          {level === "subgraph" && subgraph
            ? `${drillLabel} · ${subgraph.nodes.length} nodes · ${subgraph.edges.length} edges`
            : meta
              ? `${visibleMetaCount} / ${meta.nodes.length} packages · ${meta.edges.length} cross-package calls · ${sourceNodeCount.toLocaleString()} nodes`
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
        <div class="graph-stage graph-stage--fullbleed">
          <div class="graph-canvas-wrap">
            <div class="sigma-host" ref={containerRef} />
          </div>
          <div class="graph-legend px-2 py-1 bg-white small d-flex flex-wrap gap-2 gap-md-3 border-top">
            {level === "metagraph" ? (
              <span class="text-muted">Colors = Louvain communities · hover highlights neighborhood</span>
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
        <GraphSidebar
          level={level}
          communities={communities}
          selectedCommunityId={selectedCommunityId}
          onSelectCommunity={setSelectedCommunityId}
          category={category}
          onCategoryChange={setCategory}
          soloCommunity={soloCommunity}
          onSoloCommunityChange={setSoloCommunity}
          visibleCount={visibleMetaCount}
          totalCount={meta?.nodes.length ?? 0}
          drillLabel={drillLabel}
          onBack={backToMeta}
          hover={hover}
          selected={selected}
          subHover={subHover}
          onDrill={wasmReady && level === "metagraph" ? () => void drillInto(selected ?? hover!) : undefined}
          drilling={expanding}
        />
      </div>
    </div>
  );
}

function mountMetagraph(
  meta: MetagraphPayload,
  container: HTMLDivElement,
  sigmaRef: { current: Sigma | null },
  adjacencyRef: { current: Map<string, Set<string>> },
  highlightRef: { current: { hover: string | null; selected: string | null } },
  filterRef: { current: GraphFilterState },
  showCalls: boolean,
  handlers: {
    setHover: (n: Metanode | null) => void;
    setSelected: (n: Metanode | null) => void;
    onDrill: (n: Metanode) => void;
  },
) {
  const graph = new Graph();

  for (const n of meta.nodes) {
    const cid = n.community_id ?? 0;
    graph.addNode(String(n.id), {
      label: shortGraphLabel(n.label),
      fullLabel: n.label,
      x: n.x,
      y: n.y,
      size: Math.max(6, Math.log(n.size + 1) * 4.5),
      meta: n,
      color: communityColor(cid),
      baseColor: communityColor(cid),
    });
  }

  const edgeList: Array<{ source: string; target: string; weight?: number }> = [];
  if (showCalls) {
    for (const e of meta.edges) {
      edgeList.push({ source: String(e.source), target: String(e.target), weight: e.weight });
    }
    addAggregatedEdges(graph, edgeList);
  }

  adjacencyRef.current = buildUndirectedAdjacency(edgeList);
  layoutForceAtlas2(graph);

  if (sigmaRef.current) {
    sigmaRef.current.kill();
    sigmaRef.current = null;
  }

  const sigma = new Sigma(
    graph,
    container,
    metaSigmaOptions(graph, adjacencyRef, highlightRef, filterRef, showCalls),
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

function mountSubgraph(
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

  const positions = deterministicPositions(payload.nodes.length, payload.nodes[0]?.index ?? 0);
  payload.nodes.forEach((node, i) => {
    const pos = positions[i] ?? { x: 0, y: 0 };
    graph.addNode(String(node.index), {
      label: shortGraphLabel(node.name),
      fullLabel: node.name,
      x: pos.x,
      y: pos.y,
      size: Math.max(4, Math.log(node.complexity + 2) * 2.5),
      meta: node,
      color: typeColors[node.node_type] ?? "#6c757d",
      baseColor: typeColors[node.node_type] ?? "#6c757d",
    });
  });

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
    subgraphSigmaOptions(highlightRef),
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

function metaSigmaOptions(
  graph: Graph,
  adjacencyRef: { current: Map<string, Set<string>> },
  highlightRef: { current: { hover: string | null; selected: string | null } },
  filterRef: { current: GraphFilterState },
  showCalls: boolean,
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
      const meta = data.meta as Metanode;
      const filters = filterRef.current;
      const visible = passesFilters(meta, filters);
      const { hover, selected } = highlightRef.current;
      const focus = hover ?? selected;
      const hood = neighborhoodIds(focus, adjacencyRef.current);
      const inFocus = focus ? hood.has(node) : true;
      const showLabel = visible && (!focus || node === focus || node === hover);

      let color = (data.baseColor as string) ?? (data.color as string);
      if (focus && visible) {
        color = inFocus ? color : fadeColor(color, 0.18);
      }
      if (!visible) {
        return { ...data, hidden: true, label: "" };
      }

      return {
        ...data,
        color,
        label: showLabel ? (data.label as string) : "",
        zIndex: node === focus ? 3 : node === hover ? 2 : 0,
        borderColor: node === focus || node === selected ? "#212529" : undefined,
      };
    },
    edgeReducer(edge: string, data: Record<string, unknown>) {
      if (!showCalls) return { ...data, hidden: true };
      const [source, target] = graph.extremities(edge);
      const filters = filterRef.current;
      const srcMeta = graph.getNodeAttribute(source, "meta") as Metanode;
      const tgtMeta = graph.getNodeAttribute(target, "meta") as Metanode;
      const visible =
        passesFilters(srcMeta, filters) && passesFilters(tgtMeta, filters);
      if (!visible) return { ...data, hidden: true };
      if (filters.soloCommunity && filters.communityId !== null) {
        const cid = filters.communityId;
        if (srcMeta.community_id !== cid || tgtMeta.community_id !== cid) {
          return { ...data, hidden: true };
        }
      }
      const { hover, selected } = highlightRef.current;
      const focus = hover ?? selected;
      if (focus) {
        const hood = neighborhoodIds(focus, adjacencyRef.current);
        if (!hood.has(source) || !hood.has(target)) {
          return { ...data, hidden: true };
        }
      }
      return { ...data, size: (data.size as number) * 0.85, color: "#d0d5db" };
    },
  };
}

function subgraphSigmaOptions(
  highlightRef: { current: { hover: string | null; selected: string | null } },
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
      return {
        ...data,
        label: show ? (data.label as string) : "",
        zIndex: show ? 2 : 0,
        borderColor: show ? "#212529" : undefined,
      };
    },
    edgeReducer(_edge: string, data: Record<string, unknown>) {
      return { ...data, size: (data.size as number) * 0.85, color: "#d0d5db" };
    },
  };
}

function fadeColor(hsl: string, alpha: number): string {
  const m = /hsl\((\d+)\s+([\d.]+%)\s+([\d.]+%)\)/.exec(hsl);
  if (!m) return hsl;
  return `hsla(${m[1]} ${m[2]} ${m[3]} / ${alpha})`;
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
