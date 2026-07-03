import { useEffect, useState } from "preact/hooks";
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
  const { engine, error: workerError, expand, listNodes, wasmReady } = useEngineWorker();

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
    <div class="rb-app container-fluid py-3 px-4">
      <header class="mb-3 flex-shrink-0">
        <div class="d-flex align-items-center gap-2 mb-1">
          <span class="rb-header-icon" aria-hidden="true">
            ⎇
          </span>
          <h1 class="h4 mb-0 fw-semibold text-primary">rBuilder Analysis Dashboard</h1>
        </div>
        <p class="text-muted small mb-0">Comprehensive code analysis visualization</p>
      </header>

      {error && (
        <div class="alert alert-danger py-2 small" role="alert">
          {error}
        </div>
      )}

      <div class="card mb-3 shadow-sm">
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
          {manifest && (
            <span class="text-success">
              Phases:{" "}
              {Object.entries(phases)
                .map(([k, v]) => `${k}=${v}`)
                .join(", ")}
            </span>
          )}
        </div>
      </div>

      <div class="row row-cols-2 row-cols-md-4 g-3 mb-3 flex-shrink-0">
        <StatCard label="Total Nodes" value={manifest?.graph.node_count ?? "—"} />
        <StatCard label="Total Edges" value={manifest?.graph.edge_count ?? "—"} />
        <StatCard label="Functions" value={m?.function_count ?? "—"} />
        <StatCard label="Avg Complexity" value={m ? m.avg_complexity.toFixed(1) : "—"} />
        <StatCard label="Classes" value={m?.class_count ?? "—"} />
        <StatCard label="Call Edges" value={m?.calls_count ?? "—"} />
        <StatCard label="High Blast Radius" value={m?.high_blast_radius_count ?? "—"} />
        <StatCard label="Filtered Nodes" value={manifest?.graph.node_count ?? "—"} />
      </div>

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
            tab === "graph" || tab === "cfg" ? "graph-panel p-0" : "p-0"
          }`}
        >
          <div
            class={`rb-tab-panel-body ${
              tab === "graph"
                ? ""
                : tab === "cfg"
                  ? "rb-tab-panel-body--cfg p-3"
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
            />
          </div>
        </div>
      </div>

      <footer class="text-muted small mt-3 flex-shrink-0">
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

  const placeholders: Record<Exclude<TabId, "graph" | "functions" | "cfg">, string> = {
    dataflow: "Phase 5+: dataflow from archive",
    taint: "Phase 7: taint from archive",
    guide: "CLI query reference",
    slice: "Phase 5: CodeMirror + WASM slice",
    blast: "Phase 6: blast engine + depth slider",
  };

  return (
    <div>
      <h2 class="h5 mb-2">{TABS.find((t) => t.id === id)?.label}</h2>
      <p class="text-muted">{placeholders[id as Exclude<TabId, "graph" | "functions" | "cfg">]}</p>
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
