import { useEffect, useState } from "preact/hooks";
import {
  loadManifest,
  startEngineWorker,
  type DashboardManifest,
  type EngineReady,
  type WorkerOut,
} from "./types";

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
  const [engine, setEngine] = useState<EngineReady | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [tab, setTab] = useState<TabId>("graph");

  useEffect(() => {
    loadManifest()
      .then(setManifest)
      .catch((e) => setError(e instanceof Error ? e.message : String(e)));

    const worker = startEngineWorker();
    worker.onmessage = (ev: MessageEvent<WorkerOut>) => {
      const data = ev.data;
      if (data.type === "ready") {
        setEngine({
          nodeCount: data.nodeCount,
          edgeCount: data.edgeCount,
          schemaVersion: data.schemaVersion,
          digest: data.digest,
          wasm: data.wasm,
        });
      } else if (data.type === "error") {
        setError(data.message);
      }
    };
    worker.postMessage({ type: "init" });
    return () => worker.terminate();
  }, []);

  const m = manifest?.metrics;
  const phases = manifest?.phases ?? {};

  return (
    <div class="app">
      <header class="header">
        <h1>rBuilder Analysis Dashboard</h1>
        <p class="subtitle">Static bundle — Phase 0 shell + Phase 1 WASM loader</p>
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

      <main class="panel">
        <TabPanel id={tab} manifest={manifest} engine={engine} />
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
}: {
  id: TabId;
  manifest: DashboardManifest | null;
  engine: EngineReady | null;
}) {
  const placeholders: Record<TabId, string> = {
    graph: "Phase 2: Sigma.js community / exploration canvas",
    functions: "Phase 0: virtualized function table (from payload indices)",
    cfg: "Phase 4: CFG + dominance from cfg_pdg.archive.bin",
    slice: "Phase 5: CodeMirror + WASM slice",
    blast: "Phase 6: blast engine + depth slider",
    guide: "CLI query reference (static markdown)",
  };

  return (
    <div class="tab-panel">
      <h2>{TABS.find((t) => t.id === id)?.label}</h2>
      <p class="placeholder">{placeholders[id]}</p>
      {id === "graph" && engine && (
        <ul class="meta-list">
          <li>
            Payload digest: <code>{engine.digest || manifest?.graph.digest || "n/a"}</code>
          </li>
          <li>
            Format: <code>{manifest?.graph.payload_format}</code>
          </li>
        </ul>
      )}
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
