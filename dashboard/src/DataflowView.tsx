import { useEffect, useRef, useState } from "preact/hooks";
import Graph from "graphology";
import Sigma from "sigma";
import { bundleDataUrl } from "./bundleUrl";
import { listPdgVariables } from "./dataflowEngine";
import { mountSigmaWhenReady } from "./sigmaMount";
import type {
  DataflowFunctionEntry,
  DataflowGraphPayload,
  DataflowIndexPayload,
  SliceBundlePayload,
} from "./types";

export interface DataflowViewProps {
  computeDataflow: (
    functionId: string,
    variable: string | null,
    includeControl: boolean,
  ) => Promise<DataflowGraphPayload>;
}

export function DataflowView({ computeDataflow }: DataflowViewProps) {
  const [index, setIndex] = useState<DataflowIndexPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [variables, setVariables] = useState<string[]>([]);
  const [variable, setVariable] = useState<string>("");
  const [includeControl, setIncludeControl] = useState(true);
  const [graph, setGraph] = useState<DataflowGraphPayload | null>(null);
  const [source, setSource] = useState<string>("");
  const [computing, setComputing] = useState(false);

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
      setVariables([]);
      setSource("");
      setGraph(null);
      return;
    }
    let cancelled = false;
    fetch(bundleDataUrl(`${index.detail_dir}/${selectedId}.json`))
      .then((r) => {
        if (!r.ok) throw new Error(`PDG bundle HTTP ${r.status}`);
        return r.json();
      })
      .then((bundle: SliceBundlePayload) => {
        if (cancelled) return;
        setSource(bundle.source);
        setVariables(listPdgVariables(bundle.pdg.nodes));
        setVariable("");
        setGraph(null);
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, index?.available, index?.detail_dir]);

  const runDataflow = async () => {
    if (!selectedId) return;
    setComputing(true);
    setError(null);
    try {
      const payload = await computeDataflow(
        selectedId,
        variable || null,
        includeControl,
      );
      setGraph(payload);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setGraph(null);
    } finally {
      setComputing(false);
    }
  };

  if (error && !index) {
    return <div class="alert alert-danger py-2 small mb-0">{error}</div>;
  }

  if (!index) {
    return <p class="text-muted mb-0">Loading dataflow index…</p>;
  }

  if (!index.available) {
    return (
      <div>
        <h2 class="h5 mb-2">Dataflow (PDG)</h2>
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
    <div class="dataflow-view d-flex flex-column h-100 min-h-0 gap-3">
      <div class="d-flex flex-wrap align-items-end gap-2 flex-shrink-0">
        <FunctionSelect
          functions={index.functions}
          value={selectedId}
          onChange={setSelectedId}
          count={index.function_count}
        />
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
        <button
          type="button"
          class="btn btn-primary btn-sm"
          disabled={!selectedId || computing}
          onClick={() => void runDataflow()}
        >
          {computing ? "Building…" : "Show dataflow"}
        </button>
      </div>

      {error && <div class="alert alert-warning py-2 small mb-0 flex-shrink-0">{error}</div>}

      {graph && (
        <div class="row g-3 flex-grow-1 min-h-0">
          <div class="col-lg-8 d-flex flex-column min-h-0">
            <PdgGraph graph={graph} />
          </div>
          <div class="col-lg-4 d-flex flex-column min-h-0">
            <SourcePanel source={source} lines={graph.lines} graph={graph} />
          </div>
        </div>
      )}

      {!selectedId && (
        <p class="text-muted small mb-0">
          Select a function to explore PDG data dependencies (def→use edges from the CFG/PDG
          archive).
        </p>
      )}
    </div>
  );
}

function FunctionSelect({
  functions,
  value,
  onChange,
  count,
}: {
  functions: DataflowFunctionEntry[];
  value: string | null;
  onChange: (id: string | null) => void;
  count: number;
}) {
  return (
    <div style={{ minWidth: "280px", maxWidth: "100%" }}>
      <label class="form-label small mb-1" for="df-fn">
        Function ({count} with PDG)
      </label>
      <select
        id="df-fn"
        class="form-select form-select-sm"
        value={value ?? ""}
        onChange={(e) => {
          const v = (e.target as HTMLSelectElement).value;
          onChange(v || null);
        }}
      >
        <option value="">Select function…</option>
        {functions.map((f) => (
          <option key={f.function_id} value={f.function_id}>
            {f.name} ({f.data_edges} data edges)
          </option>
        ))}
      </select>
    </div>
  );
}

function PdgGraph({ graph }: { graph: DataflowGraphPayload }) {
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const el = containerRef.current;
    if (!el || graph.nodes.length === 0) return;

    return mountSigmaWhenReady(el, () => {
      const g = new Graph();
      const positions = layoutPdg(graph);

      for (const node of graph.nodes) {
        g.addNode(node.id, {
          label: `L${node.line}`,
          x: positions[node.id]?.x ?? 0,
          y: positions[node.id]?.y ?? 0,
          size: 10,
          color: "#0d6efd",
        });
      }

      for (const edge of graph.edges) {
        const key = `${edge.source}->${edge.target}:${edge.kind}`;
        if (!g.hasEdge(key)) {
          g.addEdgeWithKey(key, edge.source, edge.target, {
            color: edge.kind === "data" ? "#198754" : "#fd7e14",
            size: edge.kind === "data" ? 2 : 1,
          });
        }
      }

      const sigma = new Sigma(g, el, {
        renderEdgeLabels: false,
        labelSize: 10,
        defaultEdgeColor: "#adb5bd",
        minCameraRatio: 0.08,
        maxCameraRatio: 10,
      });

      const ro = new ResizeObserver(() => sigma.refresh());
      ro.observe(el);

      return () => {
        ro.disconnect();
        sigma.kill();
      };
    });
  }, [graph]);

  return (
    <div class="dataflow-graph-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small flex-shrink-0">
        <span class="fw-semibold">PDG dataflow</span>
        <span class="text-muted ms-2">
          {graph.data_edge_count} data · {graph.control_edge_count} control
          {graph.variable ? ` · var ${graph.variable}` : ""}
        </span>
      </div>
      <div class="dataflow-graph-wrap flex-grow-1 min-h-0">
        {graph.nodes.length === 0 ? (
          <p class="text-muted small p-3 mb-0">No PDG nodes match this filter.</p>
        ) : (
          <div ref={containerRef} class="sigma-host" />
        )}
      </div>
      <div class="border-top py-1 px-3 small d-flex gap-3 flex-shrink-0">
        <span>
          <span class="d-inline-block rounded me-1" style={{ width: 10, height: 10, background: "#198754" }} />
          data
        </span>
        <span>
          <span class="d-inline-block rounded me-1" style={{ width: 10, height: 10, background: "#fd7e14" }} />
          control
        </span>
      </div>
    </div>
  );
}

function SourcePanel({
  source,
  lines,
  graph,
}: {
  source: string;
  lines: number[];
  graph: DataflowGraphPayload;
}) {
  const lineSet = new Set(lines);
  const sourceLines = source.split("\n");

  return (
    <div class="dataflow-source-panel d-flex flex-column flex-grow-1 min-h-0 border rounded bg-white">
      <div class="border-bottom py-2 px-3 small fw-semibold flex-shrink-0">Statements in flow</div>
      <div class="flex-grow-1 min-h-0 overflow-auto small font-monospace p-2">
        {graph.nodes.length === 0 ? (
          <p class="text-muted mb-0">No statements.</p>
        ) : (
          <table class="table table-sm mb-0">
            <tbody>
              {graph.nodes.map((n) => (
                <tr key={n.id} class={lineSet.has(n.line) ? "table-primary" : ""}>
                  <td class="text-muted text-end pe-2">{n.line}</td>
                  <td class="text-break">{n.label}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
        {sourceLines.length > 0 && graph.nodes.length > 0 && (
          <details class="mt-2">
            <summary class="text-muted">Full file ({sourceLines.length} lines)</summary>
            <pre class="bg-light rounded p-2 mt-1 mb-0" style={{ fontSize: "0.75rem" }}>
              {sourceLines
                .map((line, i) => {
                  const ln = i + 1;
                  return lineSet.has(ln) ? `→ ${ln}: ${line}` : `  ${ln}: ${line}`;
                })
                .join("\n")}
            </pre>
          </details>
        )}
      </div>
    </div>
  );
}

function layoutPdg(graph: DataflowGraphPayload): Record<string, { x: number; y: number }> {
  const byLine = new Map<number, string[]>();
  for (const n of graph.nodes) {
    if (!byLine.has(n.line)) byLine.set(n.line, []);
    byLine.get(n.line)!.push(n.id);
  }

  const out: Record<string, { x: number; y: number }> = {};
  for (const [line, ids] of byLine) {
    ids.forEach((id, i) => {
      out[id] = {
        x: line * 40,
        y: (i - (ids.length - 1) / 2) * 60,
      };
    });
  }
  return out;
}
