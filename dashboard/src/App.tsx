import { useEffect, useState } from "preact/hooks";
import { FunctionsView } from "./FunctionsView";
import { GraphView } from "./GraphView";
import { loadManifest, type DashboardManifest, type EngineReady } from "./types";
import { useEngineWorker } from "./useEngineWorker";

const TABS = [
  { id: "graph", label: "Graph Visualization" },
  { id: "functions", label: "Functions" },
  { id: "cfg", label: "CFG / Dominance" },
  { id: "slice", label: "Program Slicing" },
  { id: "blast", label: "Blast Radius" },
  { id: "guide", label: "Query Guide" },
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
    <div class="app">
      <header class="header">
        <h1>rBuilder Analysis Dashboard</h1>
        <p class="subtitle">Phase 3 — LOD drill-down, bitmask filters, function table</p>
      </header>

      {error && (
        <div class="banner banner-error" role="alert">
          {error}
        </div>
      )}

      <section class="status-bar">
        <span>
          Engine:{" "}
          {engine
            ? engine.wasm
              ? "WASM ✓"
              : "JS header fallback"
            : "loading…"}
        </span>
        {engine && (
          <>
            <span>Nodes: {engine.nodeCount.toLocaleString()}</span>
            <span>Edges: {engine.edgeCount.toLocaleString()}</span>
            <span>Schema: v{engine.schemaVersion}</span>
          </>
        )}
        {view && (
          <span>
            Metanodes: {view.metanode_count} · Metaedges: {view.metaedge_count}
          </span>
        )}
        {manifest && (
          <span class="phases">
            Phases:{" "}
            {Object.entries(phases)
              .map(([k, v]) => `${k}=${v}`)
              .join(", ")}
          </span>
        )}
      </section>

      <section class="stats">
        <StatCard label="Total Nodes" value={manifest?.graph.node_count ?? "—"} />
        <StatCard label="Total Edges" value={manifest?.graph.edge_count ?? "—"} />
        <StatCard label="Functions" value={m?.function_count ?? "—"} />
        <StatCard label="Avg Complexity" value={m ? m.avg_complexity.toFixed(1) : "—"} />
        <StatCard label="Classes" value={m?.class_count ?? "—"} />
        <StatCard label="Call Edges" value={m?.calls_count ?? "—"} />
        <StatCard label="High Blast Radius" value={m?.high_blast_radius_count ?? "—"} />
      </section>

      <nav class="tabs" role="tablist">
        {TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            role="tab"
            aria-selected={tab === t.id}
            class={tab === t.id ? "tab active" : "tab"}
            onClick={() => setTab(t.id)}
          >
            {t.label}
          </button>
        ))}
      </nav>

      <main class={tab === "graph" ? "panel panel-graph" : "panel"}>
        <TabPanel
          id={tab}
          manifest={manifest}
          engine={engine}
          wasmReady={wasmReady}
          expand={expand}
          listNodes={listNodes}
        />
      </main>

      <footer class="footer">
        <code>docs/dashboard-design.md</code> — manifest v{manifest?.schema_version ?? "?"} ·{" "}
        {manifest?.graph.payload_format ?? "columnar_v2"}
      </footer>
    </div>
  );
}

function StatCard({ label, value }: { label: string; value: string | number }) {
  return (
    <div class="stat-card">
      <div class="stat-value">{value}</div>
      <div class="stat-label">{label}</div>
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

  const placeholders: Record<Exclude<TabId, "graph" | "functions">, string> = {
    cfg: "Phase 4: CFG + dominance from cfg_pdg.archive.bin",
    slice: "Phase 5: CodeMirror + WASM slice",
    blast: "Phase 6: blast engine + depth slider",
    guide: "CLI query reference (static markdown)",
  };

  return (
    <div class="tab-panel">
      <h2>{TABS.find((t) => t.id === id)?.label}</h2>
      <p class="placeholder">{placeholders[id as Exclude<TabId, "graph" | "functions">]}</p>
      {id === "guide" && (
        <pre class="guide">
          {`rbuilder discover .
rbuilder -f json blast-radius OrderService::process --depth 5
rbuilder gql "MATCH (n:Function) RETURN n LIMIT 10"`}
        </pre>
      )}
    </div>
  );
}
