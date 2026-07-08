import { useEffect, useState } from "preact/hooks";
import { DataflowView } from "./DataflowView";
import { BlastView } from "./BlastView";
import { SliceView } from "./SliceView";
import { TaintView } from "./TaintView";
import { CfgView } from "./CfgView";
import { FunctionsView } from "./FunctionsView";
import { GraphView } from "./GraphView";
import { NotificationMenu } from "./NotificationMenu";
import { TabPanelStack } from "./TabDocPanel";
import { DASHBOARD_TABS } from "./tabIcons";
import { type TabId } from "./tabDocs";
import { loadManifest, type DashboardManifest, type EngineReady } from "./types";
import { useEngineWorker } from "./useEngineWorker";

export function App() {
  const [manifest, setManifest] = useState<DashboardManifest | null>(null);
  const [manifestError, setManifestError] = useState<string | null>(null);
  const [tab, setTab] = useState<TabId>("graph");
  const { engine, error: workerError, expand, listNodes, computeSlice, blastRadius, wasmReady } =
    useEngineWorker();

  useEffect(() => {
    loadManifest()
      .then(setManifest)
      .catch((e) => setManifestError(e instanceof Error ? e.message : String(e)));
  }, []);

  const error = manifestError ?? workerError;
  const m = manifest?.metrics;

  return (
    <div class={`rb-app container-fluid px-3 px-md-4 ${tab === "graph" ? "rb-app--graph-focus py-2" : "py-3"}`}>
      <header class={`flex-shrink-0 ${tab === "graph" ? "mb-2" : "mb-3"}`}>
        <div class="d-flex align-items-start justify-content-between gap-3">
          <div class="min-w-0">
            <div class="d-flex align-items-center gap-2 mb-1">
              <span class="rb-header-icon" aria-hidden="true">
                ⎇
              </span>
              <h1 class={`mb-0 fw-semibold text-primary ${tab === "graph" ? "h5" : "h4"}`}>
                rBuilder Analysis Dashboard
              </h1>
            </div>
            {tab !== "graph" && (
              <p class="text-muted small mb-0">Comprehensive code analysis visualization</p>
            )}
          </div>
          <NotificationMenu
            manifest={manifest}
            engine={engine}
            wasmReady={wasmReady}
            manifestError={manifestError}
            workerError={workerError}
          />
        </div>
      </header>

      {error && (
        <div class="alert alert-danger py-2 small" role="alert">
          {error}
        </div>
      )}

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

      <div class="rb-tab-workspace rb-tab-workspace--analysis">
        <ul class="nav nav-tabs mb-0 flex-shrink-0 rb-main-tabs">
          {DASHBOARD_TABS.map((t) => (
            <li class="nav-item" key={t.id}>
              <button
                type="button"
                class={`nav-link d-inline-flex align-items-center gap-1 ${tab === t.id ? "active" : ""}`}
                onClick={() => setTab(t.id)}
              >
                <i class={`bi ${t.icon} rb-tab-icon`} aria-hidden="true" />
                <span>{t.label}</span>
              </button>
            </li>
          ))}
        </ul>

        <div
          class={`card shadow-sm border-top-0 rounded-top-0 rb-tab-panel-card ${
            tab === "graph" || tab === "cfg" || tab === "slice" || tab === "blast" || tab === "dataflow" || tab === "taint"
              ? "graph-panel p-0"
              : "p-0"
          }`}
        >
          <div
            class={`rb-tab-panel-body ${
              tab === "graph"
                ? ""
                : tab === "cfg"
                  ? "rb-tab-panel-body--cfg p-0"
                  : tab === "slice"
                    ? "rb-tab-panel-body--cfg p-0"
                    : tab === "blast"
                      ? "rb-tab-panel-body--cfg p-0"
                      : tab === "dataflow"
                        ? "rb-tab-panel-body--cfg p-0"
                        : tab === "taint"
                          ? "rb-tab-panel-body--cfg p-0"
                          : "rb-tab-panel-body--scroll p-0"
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
}) {
  if (id === "graph") {
    return (
      <TabPanelStack tabId={id}>
        <GraphView
          communityOnly={manifest?.view?.community_only ?? false}
          sourceNodeCount={manifest?.graph.node_count ?? engine?.nodeCount ?? 0}
          wasmReady={wasmReady}
          expand={expand}
        />
      </TabPanelStack>
    );
  }

  if (id === "functions") {
    return (
      <TabPanelStack tabId={id}>
        <FunctionsView
          wasmReady={wasmReady}
          functionCount={manifest?.metrics.function_count ?? 0}
          listNodes={listNodes}
        />
      </TabPanelStack>
    );
  }

  if (id === "cfg") {
    return (
      <TabPanelStack tabId={id}>
        <CfgView />
      </TabPanelStack>
    );
  }

  if (id === "dataflow") {
    return (
      <TabPanelStack tabId={id}>
        <DataflowView />
      </TabPanelStack>
    );
  }

  if (id === "slice") {
    return (
      <TabPanelStack tabId={id}>
        <SliceView computeSlice={computeSlice} />
      </TabPanelStack>
    );
  }

  if (id === "blast") {
    return (
      <TabPanelStack tabId={id}>
        <BlastView
          wasmReady={wasmReady}
          functionCount={manifest?.metrics.function_count ?? 0}
          listNodes={listNodes}
          blastRadius={blastRadius}
        />
      </TabPanelStack>
    );
  }

  if (id === "taint") {
    return (
      <TabPanelStack tabId={id}>
        <TaintView />
      </TabPanelStack>
    );
  }

  const placeholders: Record<Exclude<TabId, "graph" | "functions" | "cfg" | "dataflow" | "slice" | "blast" | "taint">, string> = {
    guide: "CLI query reference",
  };

  return (
    <TabPanelStack tabId={id}>
      <div class="p-4">
        <h2 class="h5 mb-2">{DASHBOARD_TABS.find((t) => t.id === id)?.label}</h2>
        <p class="text-muted">{placeholders[id as Exclude<TabId, "graph" | "functions" | "cfg" | "dataflow" | "slice" | "blast" | "taint">]}</p>
        {id === "guide" && (
          <pre class="bg-light border rounded p-3 small mb-0">
            {`rbuilder discover .
rbuilder -f json blast-radius OrderService::process --depth 5
rbuilder gql "MATCH (n:Function) RETURN n LIMIT 10"`}
          </pre>
        )}
      </div>
    </TabPanelStack>
  );
}
