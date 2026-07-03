import { useEffect, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import type { MetagraphPayload, Metanode } from "./types";

export interface GraphViewProps {
  communityOnly: boolean;
  sourceNodeCount: number;
}

export function GraphView({ communityOnly, sourceNodeCount }: GraphViewProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);
  const [meta, setMeta] = useState<MetagraphPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [hover, setHover] = useState<Metanode | null>(null);
  const [selected, setSelected] = useState<Metanode | null>(null);
  const [loadState, setLoadState] = useState<string>("loading");

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
    if (!meta || !containerRef.current) return;

    const graph = new Graph();
    for (const n of meta.nodes) {
      graph.addNode(String(n.id), {
        label: n.label,
        x: n.x,
        y: n.y,
        size: Math.max(4, Math.log(n.size + 1) * 5),
        functions: n.functions,
        classes: n.classes,
        avgComplexity: n.avg_complexity,
        nodeSize: n.size,
        color: "#58a6ff",
      });
    }
    for (const e of meta.edges) {
      const key = `${e.source}-${e.target}`;
      if (!graph.hasEdge(key) && graph.hasNode(String(e.source)) && graph.hasNode(String(e.target))) {
        graph.addEdge(String(e.source), String(e.target), {
          size: Math.max(0.5, Math.log(e.weight + 1)),
          weight: e.weight,
          color: "#484f58",
        });
      }
    }

    if (sigmaRef.current) {
      sigmaRef.current.kill();
      sigmaRef.current = null;
    }

    const sigma = new Sigma(graph, containerRef.current, {
      renderEdgeLabels: false,
      labelFont: "system-ui, sans-serif",
      labelSize: 11,
      labelWeight: "500",
      defaultNodeColor: "#58a6ff",
      defaultEdgeColor: "#484f58",
      minCameraRatio: 0.08,
      maxCameraRatio: 10,
    });

    sigma.on("enterNode", ({ node }) => {
      const attrs = graph.getNodeAttributes(node);
      setHover({
        id: Number(node),
        label: attrs.label as string,
        size: attrs.nodeSize as number,
        functions: attrs.functions as number,
        classes: attrs.classes as number,
        avg_complexity: attrs.avgComplexity as number,
        x: 0,
        y: 0,
      });
    });
    sigma.on("leaveNode", () => setHover(null));
    sigma.on("clickNode", ({ node }) => {
      const attrs = graph.getNodeAttributes(node);
      setSelected({
        id: Number(node),
        label: attrs.label as string,
        size: attrs.nodeSize as number,
        functions: attrs.functions as number,
        classes: attrs.classes as number,
        avg_complexity: attrs.avgComplexity as number,
        x: 0,
        y: 0,
      });
    });
    sigma.on("clickStage", () => setSelected(null));

    sigmaRef.current = sigma;
    return () => {
      sigma.kill();
      sigmaRef.current = null;
    };
  }, [meta]);

  return (
    <div class="graph-view">
      <div class="graph-toolbar">
        <span class="graph-mode-badge">
          {communityOnly ? "Community-only (≥50k nodes)" : "Package metagraph"}
        </span>
        <span class="graph-meta">
          {meta
            ? `${meta.nodes.length} metanodes · ${meta.edges.length} cross-package edges · ${sourceNodeCount.toLocaleString()} source nodes`
            : loadState}
        </span>
      </div>

      {error && <div class="banner banner-error">{error}</div>}

      <div class="graph-layout">
        <div class="graph-canvas-wrap" ref={containerRef} />

        <aside class="graph-inspector">
          <h3>Inspector</h3>
          {(selected ?? hover) ? (
            <MetanodeDetail node={selected ?? hover!} isSelected={!!selected} />
          ) : (
            <p class="placeholder">Hover or click a metanode for package breakdown.</p>
          )}
          {selected && (
            <p class="hint">Double-click drill-down expands in Phase 3 (node-level LOD).</p>
          )}
        </aside>
      </div>
    </div>
  );
}

function MetanodeDetail({ node, isSelected }: { node: Metanode; isSelected: boolean }) {
  return (
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
    </dl>
  );
}
