import { useEffect, useLayoutEffect, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import { bundleDataUrl } from "./bundleUrl";
import { FunctionListLayout, FunctionListSidebar } from "./FunctionListSidebar";
import { cfgEntryToListItem, shortPath } from "./functionListUtils";
import { GraphZoomControls } from "./GraphZoomControls";
import { mountSigmaInWrap } from "./sigmaMount";
import type { CfgDetailPayload, CfgIndexPayload } from "./types";
import { ViewLegend } from "./ViewLegend";
import {
  CFG_EDGE_COLORS,
  CFG_EDGE_LEGEND,
  CFG_NODE_COLORS,
  CFG_NODE_LEGEND,
} from "./viewLegendData";

export function CfgView() {
  const [index, setIndex] = useState<CfgIndexPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [detail, setDetail] = useState<CfgDetailPayload | null>(null);
  const [loadingDetail, setLoadingDetail] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("cfg_index.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`cfg_index.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: CfgIndexPayload) => {
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
    if (!selectedId) {
      setDetail(null);
      return;
    }
    if (index?.detail_mode === "archive_only") {
      setDetail(null);
      setError(null);
      return;
    }
    let cancelled = false;
    setLoadingDetail(true);
    fetch(bundleDataUrl(`cfg/${selectedId}.json`))
      .then((r) => {
        if (!r.ok) throw new Error(`cfg detail HTTP ${r.status}`);
        return r.json();
      })
      .then((data: CfgDetailPayload) => {
        if (!cancelled) setDetail(data);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingDetail(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, index?.detail_mode]);

  if (error) {
    return <div class="alert alert-danger py-2 small mb-0">{error}</div>;
  }

  if (!index) {
    return <p class="text-muted mb-0">Loading CFG index…</p>;
  }

  if (!index.available) {
    return (
      <div>
        <h2 class="h5 mb-2">CFG / Dominance</h2>
        <p class="text-muted mb-2">
          No CFG archive in this bundle. Run discover with CFG analysis enabled:
        </p>
        <pre class="bg-light border rounded p-3 small mb-0">
          rbuilder discover . --languages java --cfg
        </pre>
        <p class="text-muted small mt-2 mb-0">
          Previews are exported from <code>cfg_pdg.archive.bin</code> into{" "}
          <code>cfg_index.json</code> and per-function JSON under <code>cfg/</code>.
        </p>
      </div>
    );
  }

  const listItems = index.functions.map(cfgEntryToListItem);

  return (
    <FunctionListLayout
      sidebar={
        <FunctionListSidebar
          count={index.function_count}
          items={listItems}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />
      }
    >
      <div class="cfg-view d-flex flex-column h-100 min-h-0 p-3 gap-2">
        {loadingDetail && <p class="text-muted small mb-0 flex-shrink-0">Loading CFG…</p>}

        {detail && !loadingDetail && (
          <div class="analysis-graph-stage cfg-detail d-flex flex-column flex-lg-row gap-3 flex-grow-1 min-h-0">
            <div class="cfg-graph-col flex-grow-1 min-h-0 d-flex flex-column">
              <CfgGraph detail={detail} />
            </div>
            <div class="cfg-dom-col min-h-0 d-flex flex-column">
              <DominancePanel detail={detail} />
            </div>
          </div>
        )}

        {!selectedId && (
          <p class="text-muted small mb-0">
            Pick a function to render its control-flow graph and dominator tree.
          </p>
        )}

        {selectedId && index.detail_mode === "archive_only" && (
          <p class="text-muted small mb-0">
            CFG topology for this repository is stored in{" "}
            <code>{index.archive_path ?? "cfg_pdg.archive.bin"}</code> only (large-repo
            mode). Block/edge counts are listed in the sidebar; use a smaller repo or
            re-export with fewer than 5,000 functions for inline CFG previews.
          </p>
        )}
      </div>
    </FunctionListLayout>
  );
}

function CfgGraph({ detail }: { detail: CfgDetailPayload }) {
  const wrapRef = useRef<HTMLDivElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const sigmaRef = useRef<Sigma | null>(null);

  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    const el = containerRef.current;
    if (!wrap || !el) return;

    const cleanup = mountSigmaInWrap(wrap, el, () => {
      sigmaRef.current?.kill();
      sigmaRef.current = null;

      const g = new Graph();
      const positions = layoutCfg(detail);

      for (const block of detail.blocks) {
        const isEntry = block.id === detail.entry;
        const isExit = detail.exits.includes(block.id);
        g.addNode(String(block.id), {
          label: block.label,
          x: positions[block.id]?.x ?? 0,
          y: positions[block.id]?.y ?? 0,
          size: isEntry || isExit ? 14 : 10,
          color: isEntry
            ? CFG_NODE_COLORS.entry
            : isExit
              ? CFG_NODE_COLORS.exit
              : CFG_NODE_COLORS.block,
        });
      }

      for (const edge of detail.edges) {
        const key = `${edge.from}->${edge.to}:${edge.edge_type}`;
        if (!g.hasEdge(key)) {
          g.addEdgeWithKey(key, String(edge.from), String(edge.to), {
            color: CFG_EDGE_COLORS[edge.edge_type] ?? "#adb5bd",
            size: 2,
          });
        }
      }

      const sigma = new Sigma(g, el, {
        renderEdgeLabels: false,
        labelSize: 11,
        labelWeight: "500",
        defaultEdgeColor: "#adb5bd",
        minCameraRatio: 0.08,
        maxCameraRatio: 10,
      });
      sigmaRef.current = sigma;
      sigma.getCamera().animatedReset({ duration: 0 });
      return { sigma };
    });

    return () => {
      cleanup();
      sigmaRef.current = null;
    };
  }, [detail]);

  return (
    <div class="cfg-graph-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">
        CFG — {detail.name}
        {detail.file_path && (
          <span class="text-muted fw-normal ms-2">{shortPath(detail.file_path)}</span>
        )}
      </div>
      <div ref={wrapRef} class="cfg-graph-wrap analysis-graph-canvas-wrap flex-grow-1">
        <div ref={containerRef} class="sigma-host" />
        <GraphZoomControls sigmaRef={sigmaRef} />
      </div>
      <ViewLegend
        hint="Nodes"
        items={CFG_NODE_LEGEND}
        class="border-top-0 border-bottom"
      />
      <ViewLegend hint="Edges" items={CFG_EDGE_LEGEND} />
    </div>
  );
}

function DominancePanel({ detail }: { detail: CfgDetailPayload }) {
  const selectedBlock = detail.entry;
  const hasDominance =
    detail.idom != null && detail.dominance_frontiers != null;

  return (
    <div class="cfg-dom-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">Dominance</div>
      <div class="flex-grow-1 min-h-0 overflow-auto small">
        {!hasDominance ? (
          <p class="text-muted p-2 mb-0">
            Dominance preview omitted in compact export. CFG blocks and edges are still
            shown in the graph.
          </p>
        ) : (
        <table class="table table-sm table-striped mb-0">
          <thead>
            <tr>
              <th>Block</th>
              <th>idom</th>
              <th>Frontier</th>
            </tr>
          </thead>
          <tbody>
            {detail.blocks.map((b) => (
              <tr key={b.id} class={b.id === selectedBlock ? "table-success" : ""}>
                <td>
                  <code>{b.label}</code>
                </td>
                <td>{detail.idom?.[b.id]?.toString() ?? "—"}</td>
                <td>{(detail.dominance_frontiers?.[b.id] ?? []).join(", ") || "—"}</td>
              </tr>
            ))}
          </tbody>
        </table>
        )}
        {detail.blocks.some((b) => b.statements.length > 0) && (
          <div class="p-2 border-top">
            <div class="fw-semibold mb-1">Entry block preview</div>
            <pre class="bg-light rounded p-2 mb-0" style={{ fontSize: "0.75rem" }}>
              {detail.blocks.find((b) => b.id === detail.entry)?.statements.join("\n") ??
                "(empty)"}
            </pre>
          </div>
        )}
      </div>
    </div>
  );
}

function layoutCfg(detail: CfgDetailPayload): Record<number, { x: number; y: number }> {
  const layers = new Map<number, number>();
  const adj = new Map<number, number[]>();
  for (const e of detail.edges) {
    if (!adj.has(e.from)) adj.set(e.from, []);
    adj.get(e.from)!.push(e.to);
  }

  const queue: number[] = [detail.entry];
  const depth = new Map<number, number>();
  depth.set(detail.entry, 0);

  while (queue.length) {
    const n = queue.shift()!;
    const d = depth.get(n)!;
    for (const next of adj.get(n) ?? []) {
      if (!depth.has(next)) {
        depth.set(next, d + 1);
        queue.push(next);
      }
    }
  }

  for (const b of detail.blocks) {
    const d = depth.get(b.id) ?? 0;
    const y = layers.get(d) ?? 0;
    layers.set(d, y + 1);
  }

  const layerCounts = new Map<number, number>();
  const out: Record<number, { x: number; y: number }> = {};

  for (const b of detail.blocks) {
    const d = depth.get(b.id) ?? 0;
    const idx = layerCounts.get(d) ?? 0;
    layerCounts.set(d, idx + 1);
    const total = layers.get(d) ?? 1;
    out[b.id] = {
      x: d * 120,
      y: (idx - (total - 1) / 2) * 80,
    };
  }

  return out;
}
