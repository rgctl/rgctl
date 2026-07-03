import { useCallback, useEffect, useState } from "preact/hooks";
import type { BlastRadiusPayload, NodeListEntry } from "./types";
import { NODE_TYPE_MASK } from "./types";

export interface BlastViewProps {
  wasmReady: boolean;
  functionCount: number;
  listNodes: (
    typeMask: number,
    offset: number,
    limit: number,
  ) => Promise<import("./types").NodeListPayload>;
  blastRadius: (nodeIndex: number, maxDepth: number) => Promise<BlastRadiusPayload>;
}

export function BlastView({ wasmReady, functionCount, listNodes, blastRadius }: BlastViewProps) {
  const [search, setSearch] = useState("");
  const [depth, setDepth] = useState(5);
  const [functions, setFunctions] = useState<NodeListEntry[]>([]);
  const [selected, setSelected] = useState<NodeListEntry | null>(null);
  const [result, setResult] = useState<BlastRadiusPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [computing, setComputing] = useState(false);

  const loadFunctions = useCallback(async () => {
    if (!wasmReady) return;
    setLoading(true);
    setError(null);
    try {
      const page = await listNodes(NODE_TYPE_MASK.Function, 0, 500);
      setFunctions(page.items);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [wasmReady, listNodes]);

  useEffect(() => {
    void loadFunctions();
  }, [loadFunctions]);

  const filtered = filterFunctions(functions, search);

  const runBlast = async () => {
    if (!selected) return;
    setComputing(true);
    setError(null);
    try {
      const payload = await blastRadius(selected.index, depth);
      setResult(payload);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setResult(null);
    } finally {
      setComputing(false);
    }
  };

  if (!wasmReady) {
    return (
      <p class="text-muted mb-0">
        WASM engine required for blast-radius analysis. Serve the dashboard directory over HTTP.
      </p>
    );
  }

  return (
    <div class="blast-view d-flex flex-column h-100 min-h-0 gap-3">
      <div class="d-flex flex-wrap align-items-end gap-2 flex-shrink-0">
        <div class="flex-grow-1">
          <label class="form-label small mb-1" for="blast-search">
            Function ({functionCount.toLocaleString()} total)
          </label>
          <input
            id="blast-search"
            type="search"
            class="form-control form-control-sm"
            placeholder="Filter by name…"
            value={search}
            onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
          />
        </div>
        <div style={{ minWidth: "280px", maxWidth: "100%" }}>
          <label class="form-label small mb-1" for="blast-fn">
            Target
          </label>
          <select
            id="blast-fn"
            class="form-select form-select-sm"
            value={selected?.index ?? ""}
            onChange={(e) => {
              const v = Number((e.target as HTMLSelectElement).value);
              const fn = filtered.find((f) => f.index === v) ?? null;
              setSelected(fn);
              setResult(null);
            }}
          >
            <option value="">Select function…</option>
            {filtered.map((f) => (
              <option key={f.index} value={f.index}>
                {f.name}
                {f.blast_score > 0 ? ` (score ${f.blast_score.toFixed(0)})` : ""}
              </option>
            ))}
          </select>
        </div>
        <div>
          <label class="form-label small mb-1" for="blast-depth">
            Caller depth: {depth}
          </label>
          <input
            id="blast-depth"
            type="range"
            class="form-range"
            min={1}
            max={15}
            value={depth}
            onInput={(e) => setDepth(Number((e.target as HTMLInputElement).value))}
          />
        </div>
        <button
          type="button"
          class="btn btn-primary btn-sm"
          disabled={!selected || computing}
          onClick={() => void runBlast()}
        >
          {computing ? "Analyzing…" : "Compute blast radius"}
        </button>
      </div>

      {loading && <p class="text-muted small mb-0">Loading functions…</p>}
      {error && <div class="alert alert-warning py-2 small mb-0">{error}</div>}

      {result && (
        <div class="flex-grow-1 min-h-0 d-flex flex-column gap-2">
          <div class="row g-2 flex-shrink-0">
            <div class="col-md-3">
              <div class="card h-100">
                <div class="card-body py-2 small">
                  <div class="text-muted">Impact score</div>
                  <div class="fs-4 fw-semibold text-primary">{result.score.toFixed(1)}</div>
                </div>
              </div>
            </div>
            <div class="col-md-3">
              <div class="card h-100">
                <div class="card-body py-2 small">
                  <div class="text-muted">Direct callers</div>
                  <div class="fs-4 fw-semibold">{result.direct_caller_count}</div>
                </div>
              </div>
            </div>
            <div class="col-md-3">
              <div class="card h-100">
                <div class="card-body py-2 small">
                  <div class="text-muted">Impact zone</div>
                  <div class="fs-4 fw-semibold">{result.impact_zone_count}</div>
                </div>
              </div>
            </div>
            <div class="col-md-3">
              <div class="card h-100">
                <div class="card-body py-2 small">
                  <div class="text-muted">Depth limit</div>
                  <div class="fs-4 fw-semibold">{result.depth_limit}</div>
                </div>
              </div>
            </div>
          </div>

          <div class="card flex-grow-1 min-h-0">
            <div class="card-header py-2 small fw-semibold">
              Callers of <code>{result.seed_name}</code>
            </div>
            <div class="table-responsive flex-grow-1 min-h-0 overflow-auto">
              <table class="table table-sm table-striped mb-0 small">
                <thead>
                  <tr>
                    <th>Depth</th>
                    <th>Name</th>
                    <th>Type</th>
                  </tr>
                </thead>
                <tbody>
                  {result.callers.length === 0 ? (
                    <tr>
                      <td colSpan={3} class="text-muted">
                        No callers within depth {result.depth_limit}.
                      </td>
                    </tr>
                  ) : (
                    result.callers.map((c) => (
                      <tr key={`${c.index}-${c.depth}`}>
                        <td>{c.depth}</td>
                        <td>
                          <code>{c.name}</code>
                        </td>
                        <td>{c.node_type_name}</td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
          </div>
        </div>
      )}

      {!selected && !loading && (
        <p class="text-muted small mb-0">
          Pick a function and adjust caller depth to explore upstream impact (WASM reverse call-graph
          BFS).
        </p>
      )}
    </div>
  );
}

function filterFunctions(list: NodeListEntry[], search: string): NodeListEntry[] {
  const q = search.trim().toLowerCase();
  if (!q) return list;
  return list.filter((f) => f.name.toLowerCase().includes(q));
}
