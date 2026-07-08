import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import { bundleDataUrl } from "./bundleUrl";
import {
  computeDataflowGraph,
  computeDominatorGraph,
  highlightLinesForGraphNode,
  listPdgVariables,
  type DataflowViewMode,
} from "./dataflowEngine";
import { FunctionListLayout, FunctionListSidebar } from "./FunctionListSidebar";
import { dataflowEntryToListItem } from "./functionListUtils";
import { layoutForceAtlas2 } from "./graphLayout";
import { GraphZoomControls } from "./GraphZoomControls";
import { mountSigmaInWrap } from "./sigmaMount";
import { ViewLegend } from "./ViewLegend";
import {
  DOMINATOR_EDGE_LEGEND,
  DOMINATOR_NODE_LEGEND,
  PDG_EDGE_COLORS,
  PDG_EDGE_LEGEND,
  PDG_NODE_COLORS,
  PDG_NODE_LEGEND,
} from "./viewLegendData";
import type {
  CfgDetailPayload,
  DataflowGraphPayload,
  DataflowIndexPayload,
  SliceBundlePayload,
  SlicePdgNode,
} from "./types";

export function DataflowView() {
  const [index, setIndex] = useState<DataflowIndexPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<SliceBundlePayload | null>(null);
  const [cfg, setCfg] = useState<CfgDetailPayload | null>(null);
  const [variables, setVariables] = useState<string[]>([]);
  const [variable, setVariable] = useState<string>("");
  const [includeControl, setIncludeControl] = useState(true);
  const [includeCfg, setIncludeCfg] = useState(true);
  const [viewMode, setViewMode] = useState<DataflowViewMode>("dataflow");
  const [graph, setGraph] = useState<DataflowGraphPayload | null>(null);
  const [selectedGraphNodeId, setSelectedGraphNodeId] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("dataflow_index.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`dataflow_index.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: DataflowIndexPayload) => {
        if (!cancelled) setIndex(data);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!selectedId || !index?.available) {
      setBundle(null);
      setCfg(null);
      setVariables([]);
      setGraph(null);
      setSelectedGraphNodeId(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    setError(null);
    setSelectedGraphNodeId(null);

    const sliceUrl = bundleDataUrl(`${index.detail_dir}/${selectedId}.json`);
    const cfgUrl = bundleDataUrl(`cfg/${selectedId}.json`);

    Promise.all([
      fetch(sliceUrl).then((r) => {
        if (!r.ok) throw new Error(`PDG bundle HTTP ${r.status}`);
        return r.json() as Promise<SliceBundlePayload>;
      }),
      fetch(cfgUrl)
        .then((r) => (r.ok ? r.json() : null))
        .catch(() => null) as Promise<CfgDetailPayload | null>,
    ])
      .then(([sliceBundle, cfgDetail]) => {
        if (cancelled) return;
        setBundle(sliceBundle);
        setCfg(cfgDetail);
        setVariables(listPdgVariables(sliceBundle.pdg.nodes, sliceBundle.pdg.edges));
        setVariable("");
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });

    return () => {
      cancelled = true;
    };
  }, [selectedId, index?.available, index?.detail_dir]);

  useEffect(() => {
    setSelectedGraphNodeId(null);
  }, [viewMode, variable, includeControl, includeCfg]);

  useEffect(() => {
    if (!bundle) {
      setGraph(null);
      return;
    }
    if (viewMode === "dominator") {
      if (!cfg || cfg.blocks.length === 0) {
        setGraph(null);
        return;
      }
      setGraph(computeDominatorGraph(cfg));
      return;
    }
    setGraph(
      computeDataflowGraph(bundle.pdg.nodes, bundle.pdg.edges, cfg, {
        variable: variable || null,
        includeControl,
        includeCfg,
      }),
    );
  }, [bundle, cfg, variable, includeControl, includeCfg, viewMode]);

  const source = bundle?.source ?? "";
  const pdgNodes = bundle?.pdg.nodes ?? [];

  if (error && !index) {
    return <div class="alert alert-danger py-2 small mb-0">{error}</div>;
  }

  if (!index) {
    return <p class="text-muted mb-0">Loading dataflow index…</p>;
  }

  if (!index.available) {
    return (
      <div>
        <h2 class="h5 mb-2">Dataflow (CFG + PDG)</h2>
        <p class="text-muted mb-2">
          Dataflow visualization requires CFG/PDG analysis. Run discover with{" "}
          <code>--cfg</code>:
        </p>
        <pre class="bg-light border rounded p-3 small mb-0">
          rbuilder discover . --languages java --cfg
        </pre>
      </div>
    );
  }

  return (
    <FunctionListLayout
      sidebar={
        <FunctionListSidebar
          count={index.function_count}
          items={index.functions.map(dataflowEntryToListItem)}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />
      }
    >
      <div class="dataflow-view d-flex flex-column flex-grow-1 min-h-0 p-3">
        <div class="d-flex flex-wrap align-items-end gap-2 flex-shrink-0">
          <div style={{ minWidth: "200px" }}>
            <label class="form-label small mb-1" for="df-view">
              View
            </label>
            <select
              id="df-view"
              class="form-select form-select-sm"
              value={viewMode}
              disabled={!selectedId}
              onChange={(e) =>
                setViewMode((e.target as HTMLSelectElement).value as DataflowViewMode)
              }
            >
              <option value="dataflow">Data Flow (CFG + PDG)</option>
              <option value="dominator">Dominator Tree</option>
            </select>
          </div>
          {viewMode === "dataflow" && (
            <>
              <div style={{ minWidth: "160px" }}>
                <label class="form-label small mb-1" for="df-var">
                  Variable
                </label>
                <select
                  id="df-var"
                  class="form-select form-select-sm"
                  value={variable}
                  onChange={(e) => setVariable((e.target as HTMLSelectElement).value)}
                  disabled={!selectedId}
                >
                  <option value="">All data edges</option>
                  {variables.map((v) => (
                    <option key={v} value={v}>
                      {v}
                    </option>
                  ))}
                </select>
              </div>
              <div class="form-check form-switch mb-0">
                <input
                  class="form-check-input"
                  type="checkbox"
                  id="df-control"
                  checked={includeControl}
                  onChange={(e) => setIncludeControl((e.target as HTMLInputElement).checked)}
                />
                <label class="form-check-label small" for="df-control">
                  Control deps
                </label>
              </div>
              <div class="form-check form-switch mb-0">
                <input
                  class="form-check-input"
                  type="checkbox"
                  id="df-cfg"
                  checked={includeCfg}
                  onChange={(e) => setIncludeCfg((e.target as HTMLInputElement).checked)}
                />
                <label class="form-check-label small" for="df-cfg">
                  CFG edges
                </label>
              </div>
            </>
          )}
        </div>

        {error && <div class="alert alert-warning py-2 small mb-0 flex-shrink-0">{error}</div>}
        {loading && <p class="text-muted small mb-0 flex-shrink-0">Loading function…</p>}

        {graph && !loading && bundle && (
          <div class="analysis-graph-stage d-flex flex-grow-1 min-h-0 overflow-hidden gap-3">
            <div class="analysis-graph-primary d-flex flex-column min-h-0">
              <DataflowGraphPanel
                graph={graph}
                selectedNodeId={selectedGraphNodeId}
                onNodeSelect={setSelectedGraphNodeId}
              />
            </div>
            <div class="analysis-graph-side d-flex flex-column min-h-0">
              <SourcePanel
                source={source}
                pdgNodes={pdgNodes}
                cfg={cfg}
                graph={graph}
                viewMode={viewMode}
                selectedGraphNodeId={selectedGraphNodeId}
                onStatementSelect={setSelectedGraphNodeId}
              />
            </div>
          </div>
        )}

        {selectedId && !graph && !loading && viewMode === "dominator" && (
          <p class="text-muted small mb-0">No dominator tree data for this function.</p>
        )}

        {selectedId && !graph && !loading && viewMode === "dataflow" && (
          <p class="text-muted small mb-0">No PDG nodes to display for this function.</p>
        )}

        {!selectedId && (
          <p class="text-muted small mb-0">
            Select a function to visualize CFG control flow, PDG data dependencies, and dominance
            structure.
          </p>
        )}
      </div>
    </FunctionListLayout>
  );
}

function edgeColor(kind: DataflowGraphPayload["edges"][number]["kind"]): string {
  return PDG_EDGE_COLORS[kind];
}

function edgeSize(kind: DataflowGraphPayload["edges"][number]["kind"]): number {
  return kind === "data" ? 2 : 1.5;
}

function nodeBaseStyle(
  graph: DataflowGraphPayload,
  node: DataflowGraphPayload["nodes"][number],
): { color: string; size: number } {
  const hasFrontier = graph.view_mode === "dominator" && (node.frontier_size ?? 0) > 0;
  return {
    color: hasFrontier ? PDG_NODE_COLORS.frontier : PDG_NODE_COLORS.statement,
    size: hasFrontier ? 14 : graph.view_mode === "dominator" ? 11 : 10,
  };
}

function applyGraphSelection(
  sigma: Sigma,
  graph: DataflowGraphPayload,
  nodeId: string | null,
): void {
  const g = sigma.getGraph();
  const nodeMeta = new Map(graph.nodes.map((n) => [n.id, n]));

  g.forEachNode((id) => {
    const meta = nodeMeta.get(id);
    const base = meta ? nodeBaseStyle(graph, meta) : { color: PDG_NODE_COLORS.statement, size: 10 };
    const selected = id === nodeId;
    g.setNodeAttribute(id, "color", selected ? "#0d6efd" : base.color);
    g.setNodeAttribute(id, "size", selected ? base.size + 4 : base.size);
  });
  sigma.refresh();
}

function DataflowGraphPanel({
  graph,
  selectedNodeId,
  onNodeSelect,
}: {
  graph: DataflowGraphPayload;
  selectedNodeId: string | null;
  onNodeSelect: (nodeId: string | null) => void;
}) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);
  const onNodeSelectRef = useRef(onNodeSelect);
  onNodeSelectRef.current = onNodeSelect;

  const title = useMemo(() => {
    if (graph.view_mode === "dominator") {
      return `Dominator tree · ${graph.nodes.length} blocks`;
    }
    const parts = [
      `${graph.data_edge_count} data`,
      graph.control_edge_count > 0 ? `${graph.control_edge_count} control` : null,
      graph.cfg_edge_count > 0 ? `${graph.cfg_edge_count} CFG` : null,
    ].filter(Boolean);
    return `Data flow (CFG + PDG) · ${parts.join(" · ")}`;
  }, [graph]);

  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    const el = containerRef.current;
    if (!wrap || !el || graph.nodes.length === 0) return;

    return mountSigmaInWrap(wrap, el, () => {
      const g = new Graph({ multi: true, type: "directed" });

      for (const node of graph.nodes) {
        const base = nodeBaseStyle(graph, node);
        g.addNode(node.id, {
          label: node.display_label,
          x: Math.random() * 100,
          y: Math.random() * 100,
          size: base.size,
          color: base.color,
        });
      }

      for (const edge of graph.edges) {
        const key = `${edge.source}->${edge.target}:${edge.kind}`;
        if (!g.hasEdge(key) && g.hasNode(edge.source) && g.hasNode(edge.target)) {
          g.addEdgeWithKey(key, edge.source, edge.target, {
            color: edgeColor(edge.kind),
            size: edgeSize(edge.kind),
          });
        }
      }

      layoutForceAtlas2(g, graph.nodes.length > 80 ? 220 : 160);

      const sigma = new Sigma(g, el, {
        renderEdgeLabels: false,
        labelSize: 10,
        labelWeight: "500",
        defaultEdgeColor: PDG_EDGE_COLORS.cfg,
        minCameraRatio: 0.08,
        maxCameraRatio: 10,
      });

      sigma.on("clickNode", ({ node }) => onNodeSelectRef.current(node));
      sigma.on("clickStage", () => onNodeSelectRef.current(null));

      sigmaRef.current = sigma;
      sigma.getCamera().animatedReset({ duration: 0 });
      return { sigma };
    });
  }, [graph]);

  useLayoutEffect(() => {
    return () => {
      sigmaRef.current = null;
    };
  }, [graph]);

  useEffect(() => {
    const sigma = sigmaRef.current;
    if (!sigma) return;
    applyGraphSelection(sigma, graph, selectedNodeId);
  }, [graph, selectedNodeId]);

  return (
    <div class="dataflow-graph-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small flex-shrink-0">
        <span class="fw-semibold">{title}</span>
        {graph.view_mode === "dataflow" && graph.variable && (
          <span class="text-muted ms-2">var {graph.variable}</span>
        )}
      </div>
      <div ref={wrapRef} class="dataflow-graph-wrap analysis-graph-canvas-wrap flex-grow-1">
        {graph.nodes.length === 0 ? (
          <p class="text-muted small p-3 mb-0">No nodes match this filter.</p>
        ) : (
          <>
            <div ref={containerRef} class="sigma-host" />
            <GraphZoomControls sigmaRef={sigmaRef} />
          </>
        )}
      </div>
      {graph.view_mode === "dominator" ? (
        <>
          <ViewLegend hint="Nodes" items={DOMINATOR_NODE_LEGEND} class="border-top-0 border-bottom" />
          <ViewLegend hint="Edges" items={DOMINATOR_EDGE_LEGEND} />
        </>
      ) : (
        <>
          <ViewLegend hint="Nodes" items={PDG_NODE_LEGEND} class="border-top-0 border-bottom" />
          <ViewLegend hint="Edges" items={PDG_EDGE_LEGEND} />
        </>
      )}
    </div>
  );
}

function SourcePanel({
  source,
  pdgNodes,
  cfg,
  graph,
  viewMode,
  selectedGraphNodeId,
  onStatementSelect,
}: {
  source: string;
  pdgNodes: SlicePdgNode[];
  cfg: CfgDetailPayload | null;
  graph: DataflowGraphPayload;
  viewMode: DataflowViewMode;
  selectedGraphNodeId: string | null;
  onStatementSelect: (graphNodeId: string | null) => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const highlightLines = useMemo(
    () => highlightLinesForGraphNode(pdgNodes, cfg, graph, selectedGraphNodeId),
    [pdgNodes, cfg, graph, selectedGraphNodeId],
  );

  const statements = useMemo(
    () => [...pdgNodes].sort((a, b) => a.line - b.line || a.id.localeCompare(b.id)),
    [pdgNodes],
  );

  const sourceLines = source.split("\n");

  useEffect(() => {
    if (highlightLines.size === 0) return;
    const firstLine = [...highlightLines].sort((a, b) => a - b)[0];
    const row = scrollRef.current?.querySelector(`[data-line="${firstLine}"]`);
    row?.scrollIntoView({ block: "nearest", behavior: "smooth" });
  }, [highlightLines, selectedGraphNodeId]);

  const panelTitle =
    graph.view_mode === "dominator" ? "Statements (click a block or row)" : "Statements in flow";

  const selectStatement = (node: SlicePdgNode) => {
    if (viewMode === "dominator") {
      const blockId = node.block_index ?? cfg?.blocks.find(
        (b) => b.start_line > 0 && node.line >= b.start_line && node.line <= b.end_line,
      )?.id;
      onStatementSelect(blockId != null ? `block_${blockId}` : node.id);
      return;
    }
    onStatementSelect(node.id);
  };

  return (
    <div
      class="dataflow-source-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white"
      data-selected-id={selectedGraphNodeId ?? ""}
    >
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">{panelTitle}</div>
      <div ref={scrollRef} class="flex-grow-1 min-h-0 overflow-auto small font-monospace p-2">
        {statements.length === 0 ? (
          <p class="text-muted mb-0">No statements.</p>
        ) : (
          <table class="table table-sm mb-0">
            <tbody>
              {statements.map((n) => (
                <tr
                  key={n.id}
                  data-line={n.line}
                  class={`${highlightLines.has(n.line) ? "table-primary" : ""} dataflow-source-row`}
                  style={{ cursor: "pointer" }}
                  onClick={() => selectStatement(n)}
                >
                  <td class="text-muted text-end pe-2">{n.line}</td>
                  <td class="text-break">{n.label}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {sourceLines.length > 0 && statements.length > 0 && (
          <details class="mt-2">
            <summary class="text-muted">Full file ({sourceLines.length} lines)</summary>
            <pre class="bg-light rounded p-2 mt-1 mb-0" style={{ fontSize: "0.75rem" }}>
              {sourceLines
                .map((line, i) => {
                  const ln = i + 1;
                  return highlightLines.has(ln) ? `→ ${ln}: ${line}` : `  ${ln}: ${line}`;
                })
                .join("\n")}
            </pre>
          </details>
        )}
      </div>
    </div>
  );
}
