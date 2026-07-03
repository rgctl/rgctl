import { useEffect, useRef, useState, useCallback } from "preact/hooks";
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

export interface GraphViewProps {
  communityOnly: boolean;
  sourceNodeCount: number;
  wasmReady: boolean;
  expand: (indices: number[], typeMask: number) => Promise<SubgraphPayload>;
}

type ViewLevel = "metagraph" | "subgraph";

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
  const [loadState, setLoadState] = useState<string>("loading");
  const [level, setLevel] = useState<ViewLevel>("metagraph");
  const [subgraph, setSubgraph] = useState<SubgraphPayload | null>(null);
  const [drillLabel, setDrillLabel] = useState<string | null>(null);
  const [typeMask, setTypeMask] = useState(DEFAULT_GRAPH_TYPE_MASK);
  const [expanding, setExpanding] = useState(false);
  const [subHover, setSubHover] = useState<SubgraphNode | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch("./metagraph.json")
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

  useEffect(() => {
    if (!containerRef.current) return;

    if (level === "metagraph") {
      if (!meta) return;
      return renderMetagraph(meta, containerRef.current, sigmaRef, {
        setHover,
        setSelected,
        onDrill: (m) => void drillInto(m),
      });
    }

    if (subgraph) {
      return renderSubgraph(subgraph, containerRef.current, sigmaRef, setSubHover);
    }
  }, [meta, level, subgraph]);

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

  const reExpand = async () => {
    if (selected && level === "metagraph") return;
    if (drillLabel && meta) {
      const node = meta.nodes.find((n) => n.label === drillLabel);
      if (node) await drillInto(node);
    }
  };

  return (
    <div class="graph-view">
      <div class="graph-toolbar">
        {level === "subgraph" ? (
          <button type="button" class="graph-back-btn" onClick={backToMeta}>
            ← Packages
          </button>
        ) : (
          <span class="graph-mode-badge">
            {communityOnly ? "Community view (drill-down enabled)" : "Package metagraph"}
          </span>
        )}
        <NodeTypeFilter
          mask={typeMask}
          onChange={(m) => {
            setTypeMask(m);
          }}
          disabled={!wasmReady}
        />
        {level === "subgraph" && wasmReady && (
          <button type="button" class="graph-refresh-btn" onClick={() => void reExpand()}>
            Re-filter
          </button>
        )}
        <span class="graph-meta">
          {level === "subgraph" && subgraph
            ? `${drillLabel} · ${subgraph.nodes.length} nodes · ${subgraph.edges.length} call edges`
            : meta
              ? `${meta.nodes.length} metanodes · ${meta.edges.length} cross-package edges · ${sourceNodeCount.toLocaleString()} source nodes`
              : loadState}
          {expanding ? " · expanding…" : ""}
        </span>
      </div>

      {error && <div class="banner banner-error">{error}</div>}

      <div class="graph-layout">
        <div class="graph-canvas-wrap" ref={containerRef} />

        <aside class="graph-inspector">
          <h3>Inspector</h3>
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
            <p class="placeholder">
              {level === "subgraph"
                ? "Hover a node for details."
                : "Hover or click a metanode. Double-click or use Drill down to expand."}
            </p>
          )}
        </aside>
      </div>
    </div>
  );
}

function renderMetagraph(
  meta: MetagraphPayload,
  container: HTMLDivElement,
  sigmaRef: { current: Sigma | null },
  handlers: {
    setHover: (n: Metanode | null) => void;
    setSelected: (n: Metanode | null) => void;
    onDrill: (n: Metanode) => void;
  },
) {
  const graph = new Graph();
  for (const n of meta.nodes) {
    graph.addNode(String(n.id), {
      label: n.label,
      x: n.x,
      y: n.y,
      size: Math.max(4, Math.log(n.size + 1) * 5),
      meta: n,
      color: "#58a6ff",
    });
  }
  for (const e of meta.edges) {
    const key = `${e.source}-${e.target}`;
    if (!graph.hasEdge(key) && graph.hasNode(String(e.source)) && graph.hasNode(String(e.target))) {
      graph.addEdge(String(e.source), String(e.target), {
        size: Math.max(0.5, Math.log(e.weight + 1)),
        color: "#484f58",
      });
    }
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
) {
  const graph = new Graph();
  const n = payload.nodes.length || 1;
  const radius = Math.max(30, Math.sqrt(n) * 8);

  for (let i = 0; i < payload.nodes.length; i++) {
    const node = payload.nodes[i];
    const angle = (2 * Math.PI * i) / n;
    graph.addNode(String(node.index), {
      label: node.name,
      x: Math.cos(angle) * radius,
      y: Math.sin(angle) * radius,
      size: Math.max(3, Math.log(node.complexity + 2) * 2),
      meta: node,
      color: node.node_type === 0 ? "#3fb950" : "#58a6ff",
    });
  }
  for (const e of payload.edges) {
    const key = `${e.source}-${e.target}`;
    if (!graph.hasEdge(key) && graph.hasNode(String(e.source)) && graph.hasNode(String(e.target))) {
      graph.addEdge(String(e.source), String(e.target), { size: 0.5, color: "#484f58" });
    }
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
    defaultNodeColor: "#58a6ff",
    defaultEdgeColor: "#484f58",
    minCameraRatio: 0.08,
    maxCameraRatio: 10,
  };
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
      <dl class="inspector-dl">
        <dt>{isSelected ? "Selected" : "Hover"}</dt>
        <dd>
          <code>{node.label}</code>
        </dd>
        <dt>Members</dt>
        <dd>{node.size.toLocaleString()}</dd>
        <dt>Functions</dt>
        <dd>{node.functions.toLocaleString()}</dd>
        <dt>Classes</dt>
        <dd>{node.classes.toLocaleString()}</dd>
        <dt>Avg complexity</dt>
        <dd>{node.avg_complexity.toFixed(1)}</dd>
        <dt>Payload indices</dt>
        <dd>{(node.member_indices?.length ?? 0).toLocaleString()}</dd>
      </dl>
      {onDrill && (
        <button type="button" class="drill-btn" disabled={drilling} onClick={onDrill}>
          {drilling ? "Expanding…" : "Drill down"}
        </button>
      )}
    </>
  );
}

function SubgraphDetail({ node }: { node: SubgraphNode }) {
  return (
    <dl class="inspector-dl">
      <dt>Name</dt>
      <dd>
        <code>{node.name}</code>
      </dd>
      <dt>Type</dt>
      <dd>{node.node_type_name}</dd>
      <dt>Complexity</dt>
      <dd>{node.complexity.toFixed(1)}</dd>
      <dt>Index</dt>
      <dd>{node.index}</dd>
      {node.file_path && (
        <>
          <dt>File</dt>
          <dd class="file-path">{node.file_path}</dd>
        </>
      )}
    </dl>
  );
}
