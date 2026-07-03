export interface DashboardManifest {
  schema_version: number;
  dashboard_version: string;
  phases: Record<string, string>;
  graph: {
    payload_path: string;
    payload_format: string;
    node_count: number;
    edge_count: number;
    digest: string;
  };
  metrics: {
    function_count: number;
    class_count: number;
    calls_count: number;
    avg_complexity: number;
    high_blast_radius_count: number;
  };
  generated_at: string;
}

export interface EngineReady {
  nodeCount: number;
  edgeCount: number;
  schemaVersion: number;
  digest: string;
  wasm: boolean;
}

export type WorkerOut =
  | { type: "ready"; nodeCount: number; edgeCount: number; schemaVersion: number; digest: string; wasm: boolean }
  | { type: "error"; message: string };

export async function loadManifest(): Promise<DashboardManifest> {
  const embedded = document.getElementById("rbuilder-manifest");
  if (embedded?.textContent) {
    return JSON.parse(embedded.textContent) as DashboardManifest;
  }
  const res = await fetch("./manifest.json");
  if (!res.ok) {
    throw new Error(`manifest.json: HTTP ${res.status}`);
  }
  return (await res.json()) as DashboardManifest;
}

export function startEngineWorker(): Worker {
  return new Worker(new URL("./worker.ts", import.meta.url), { type: "module" });
}
