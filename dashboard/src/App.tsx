import { useEffect, useState } from "preact/hooks";
import { DataflowView } from "./DataflowView";
import { BlastView } from "./BlastView";
import { SliceView } from "./SliceView";
import { TaintView } from "./TaintView";
import { CfgView } from "./CfgView";
import { FunctionsView } from "./FunctionsView";
import { GraphView } from "./GraphView";
import { loadManifest, type DashboardManifest, type EngineReady } from "./types";
import { useEngineWorker } from "./useEngineWorker";

const TABS = [
  { id: "graph", label: "Graph Visualization" },
  { id: "functions", label: "Functions" },
  { id: "cfg", label: "CFG / PDG Analysis" },
  { id: "dataflow", label: "Dataflow" },
  { id: "taint", label: "Taint Analysis" },
  { id: "guide", label: "Query Guide" },
  { id: "slice", label: "Program Slicing" },
  { id: "blast", label: "Blast Radius" },
] as const;

type TabId = (typeof TABS)[number]["id"];

export function App() {
  const [manifest, setManifest] = useState<DashboardManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [tab, setTab] = useState<TabId>("graph");
  const { engine, error: workerError, expand, listNodes, computeSlice, blastRadius, computeDataflow, wasmReady } =
    useEngineWorker();

  useEffect(() => {
    loadManifest()
      .then(setManifest)
      .catch((e) => setManifestError(e instanceof Error ? e.message : String(e)));
  }, []);

  const error = manifestError ?? workerError;
  const m = manifest?.metrics;
  const phases = manifest?.phases ?? {};
  const view = manifest?.view;

  return (
    <div class={`rb-app container-fluid px-3 px-md-4 ${tab === "graph" ? "rb-app--graph-focus py-2" : "py-3"}`}>
      <header class={`flex-shrink-0 ${tab === "graph" ? "mb-2" : "mb-3"}`}>
        <div class="d-flex align-items-center gap-2 mb-1">
          <span class="rb-header-icon" aria-hidden="true">
            ⎇
          </span>
          <h1 class={`mb-0 fw-semibold text-primary ${tab === "graph" ? "h5" : "h4"}`}>rBuilder Analysis Dashboard</h1>
        </div>
        {tab !== "graph" && (
          <p class="text-muted small mb-0">Comprehensive code analysis visualization</p>
        )}
      </header>

      {error && (
        <div class="alert alert-danger py-2 small" role="alert">
          {error}
        </div>
      )}

      <div class={`card shadow-sm ${tab === "graph" ? "rb-engine-bar mb-2" : "mb-3"}`}>
        <div class="card-body py-2 small d-flex flex-wrap gap-3">
          <span>
            Engine:{" "}
            <strong>
              {engine ? (engine.wasm ? "WASM ✓" : "JS fallback") : "loading…"}
            </strong>
          </span>
          {engine && (
            <>
              <span>Nodes: {engine.nodeCount.toLocaleString()}</span>
              <span>Edges: {engine.edgeCount.toLocaleString()}</span>
            </>
          )}
          {view && (
            <span>
              Metanodes: {view.metanode_count} · Metaedges: {view.metaedge_count}
            </span>
          )}
          {manifest && tab !== "graph" && (
            <span class="text-success">
              Phases:{" "}
              {Object.entries(phases)
                .map(([k, v]) => `${k}=${v}`)
                .join(", ")}
            </span>
          )}
        </div>
      </div>

      {tab !== "graph" && (
      <div class="row row-cols-2 row-cols-md-4 g-3 mb-3 flex-shrink-0 rb-stats-row">
        <StatCard label="Total Nodes" value={manifest?.graph.node_count ?? "—"} />
        <StatCard label="Total Edges" value={manifest?.graph.edge_count ?? "—"} />
        <StatCard label="Functions" value={m?.function_count ?? "—"} />
        <StatCard label="Avg Complexity" value={m ? m.avg_complexity.toFixed(1) : "—"} />
        <StatCard label="Classes" value={m?.class_count ?? "—"} />
        <StatCard label="Call Edges" value={m?.calls_count ?? "—"} />
        <StatCard label="High Blast Radius" value={m?.high_blast_radius_count ?? "—"} />
        <StatCard label="Filtered Nodes" value={manifest?.graph.node_count ?? "—"} />
      </div>
      )}

      <div class="rb-tab-workspace">
        <ul class="nav nav-tabs mb-0 flex-shrink-0">
          {TABS.map((t) => (
            <li class="nav-item" key={t.id}>
              <button
                type="button"
                class={`nav-link ${tab === t.id ? "active" : ""}`}
                onClick={() => setTab(t.id)}
              >
                {t.label}
              </button>
            </li>
          ))}
        </ul>

        <div
          class={`card shadow-sm border-top-0 rounded-top-0 rb-tab-panel-card ${
            tab === "graph" || tab === "cfg" || tab === "slice" || tab === "blast" || tab === "dataflow"
              ? "graph-panel p-0"
              : "p-0"
          }`}
        >
          <div
            class={`rb-tab-panel-body ${
              tab === "graph"
                ? ""
                : tab === "cfg"
                  ? "rb-tab-panel-body--cfg p-3"
                  : tab === "slice"
                    ? "rb-tab-panel-body--cfg p-3"
                    : tab === "blast"
                      ? "rb-tab-panel-body--cfg p-3"
                      : tab === "dataflow"
                        ? "rb-tab-panel-body--cfg p-3"
                        : tab === "taint"
                          ? "rb-tab-panel-body--scroll p-3"
                          : "rb-tab-panel-body--scroll p-4"
            }`}
          >
            <TabPanel
              id={tab}
              manifest={manifest}
              engine={engine}
              wasmReady={wasmReady}
              expand={expand}
              listNodes={listNodes}
              computeSlice={computeSlice}
              blastRadius={blastRadius}
              computeDataflow={computeDataflow}
            />
          </div>
        </div>
      </div>

      <footer class={`text-muted small flex-shrink-0 ${tab === "graph" ? "mt-2 d-none d-lg-block" : "mt-3"}`}>
        <code>docs/dashboard-design.md</code> · manifest v{manifest?.schema_version ?? "?"} ·{" "}
        {manifest?.graph.payload_format ?? "columnar_v2"}
      </footer>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <div class="col">
      <div class="card stat-card shadow-sm h-100">
        <div class="card-body py-3">
          <div class="stat-value">{value}</div>
          <div class="stat-label">{label}</div>
        </div>
      </div>
    </div>
  );
}

function TabPanel({
  id,
  manifest,
  engine,
  wasmReady,
  expand,
  listNodes,
  computeSlice,
  blastRadius,
  computeDataflow,
}: {
  id: TabId;
  manifest: DashboardManifest | null;
  engine: EngineReady | null;
  wasmReady: boolean;
  expand: (indices: number[], typeMask: number) => Promise<import("./types").SubgraphPayload>;
  listNodes: (
    typeMask: number,
    offset: number,
    limit: number,
  ) => Promise<import("./types").NodeListPayload>;
  computeSlice: (
    functionId: string,
    line: number,
    variable: string,
    direction: import("./types").SliceDirection,
  ) => Promise<import("./types").SliceResultPayload>;
  blastRadius: (
    nodeIndex: number,
    maxDepth: number,
  ) => Promise<import("./types").BlastRadiusPayload>;
  computeDataflow: (
    functionId: string,
    variable: string | null,
    includeControl: boolean,
  ) => Promise<import("./types").DataflowGraphPayload>;
}) {
  if (id === "graph") {
    return (
      <GraphView
        communityOnly={manifest?.view?.community_only ?? false}
        sourceNodeCount={manifest?.graph.node_count ?? engine?.nodeCount ?? 0}
        wasmReady={wasmReady}
        expand={expand}
      />
    );
  }

  if (id === "functions") {
    return (
      <FunctionsView
        wasmReady={wasmReady}
        functionCount={manifest?.metrics.function_count ?? 0}
        listNodes={listNodes}
      />
    );
  }

  if (id === "cfg") {
    return <CfgView />;
  }

  if (id === "dataflow") {
    return <DataflowView computeDataflow={computeDataflow} />;
  }

  if (id === "slice") {
    return <SliceView computeSlice={computeSlice} />;
  }

  if (id === "blast") {
    return (
      <BlastView
        wasmReady={wasmReady}
        functionCount={manifest?.metrics.function_count ?? 0}
        listNodes={listNodes}
        blastRadius={blastRadius}
      />
    );
  }

  if (id === "taint") {
    return <TaintView />;
  }

  const placeholders: Record<Exclude<TabId, "graph" | "functions" | "cfg" | "dataflow" | "slice" | "blast" | "taint">, string> = {
    guide: "CLI query reference",
  };

  return (
    <div>
      <h2 class="h5 mb-2">{TABS.find((t) => t.id === id)?.label}</h2>
      <p class="text-muted">{placeholders[id as Exclude<TabId, "graph" | "functions" | "cfg" | "dataflow" | "slice" | "blast" | "taint">]}</p>
      {id === "guide" && (
        <pre class="bg-light border rounded p-3 small mb-0">
          {`rbuilder discover .
rbuilder -f json blast-radius OrderService::process --depth 5
rbuilder gql "MATCH (n:Function) RETURN n LIMIT 10"`}
        </pre>
      )}
    </div>
  );
}
