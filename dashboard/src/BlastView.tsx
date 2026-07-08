import { useCallback, useEffect, useMemo, useRef, useState } from "preact/hooks";
import type { BlastFunctionScore, BlastRadiusPayload, NodeListEntry } from "./types";
import { NODE_TYPE_MASK } from "./types";
import { bundleDataUrl } from "./bundleUrl";
import { FunctionListLayout, FunctionListSidebar } from "./FunctionListSidebar";
import { blastEntryToListItem } from "./functionListUtils";
import { ViewLegend } from "./ViewLegend";

const DEPTH_DEBOUNCE_MS = 300;

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

function scoreMapFromIndex(functions: BlastFunctionScore[] | undefined): Map<number, BlastFunctionScore> {
  const map = new Map<number, BlastFunctionScore>();
  for (const row of functions ?? []) {
    map.set(row.index, row);
  }
  return map;
}

function sortByImpactScore(
  entries: NodeListEntry[],
  scores: Map<number, BlastFunctionScore>,
): NodeListEntry[] {
  const scoreFor = (entry: NodeListEntry): number =>
    scores.get(entry.index)?.score ?? entry.blast_score;

  return [...entries].sort((a, b) => {
    const diff = scoreFor(b) - scoreFor(a);
    return diff !== 0 ? diff : a.name.localeCompare(b.name);
  });
}

export function BlastView({ wasmReady, functionCount, listNodes, blastRadius }: BlastViewProps) {
  const [depth, setDepth] = useState(5);
  const [functions, setFunctions] = useState<NodeListEntry[]>([]);
  const [scoreByIndex, setScoreByIndex] = useState<Map<number, BlastFunctionScore>>(() => new Map());
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [result, setResult] = useState<BlastRadiusPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);
  const [computing, setComputing] = useState(false);
  const requestSeq = useRef(0);
  const prevSelectedId = useRef<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("blast_index.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`blast_index.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: { functions?: BlastFunctionScore[] }) => {
        if (!cancelled) setScoreByIndex(scoreMapFromIndex(data.functions));
      })
      .catch(() => {
        if (!cancelled) setScoreByIndex(new Map());
      });
    return () => {
      cancelled = true;
    };
  }, []);

  const loadFunctions = useCallback(async () => {
    if (!wasmReady) return;
    setLoading(true);
    setError(null);
    try {
      const pageSize = 500;
      const target = Math.min(Math.max(functionCount, 1), 5000);
      const all: NodeListEntry[] = [];
      for (let offset = 0; offset < target; offset += pageSize) {
        const page = await listNodes(NODE_TYPE_MASK.Function, offset, pageSize);
        all.push(...page.items);
        if (page.items.length === 0 || all.length >= page.total) break;
      }
      setFunctions(sortByImpactScore(all, scoreByIndex));
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [wasmReady, listNodes, scoreByIndex, functionCount]);

  useEffect(() => {
    void loadFunctions();
  }, [loadFunctions]);

  const listItems = useMemo(
    () =>
      functions.map((entry) => blastEntryToListItem(entry, scoreByIndex.get(entry.index) ?? null)),
    [functions, scoreByIndex],
  );

  const selected = useMemo(
    () => functions.find((f) => String(f.index) === selectedId) ?? null,
    [functions, selectedId],
  );

  const runBlast = useCallback(
    async (nodeIndex: number, maxDepth: number) => {
      const seq = ++requestSeq.current;
      setComputing(true);
      setError(null);
      try {
        const payload = await blastRadius(nodeIndex, maxDepth);
        if (seq !== requestSeq.current) return;
        setResult(payload);
      } catch (e) {
        if (seq !== requestSeq.current) return;
        setError(e instanceof Error ? e.message : String(e));
        setResult(null);
      } finally {
        if (seq === requestSeq.current) {
          setComputing(false);
        }
      }
    },
    [blastRadius],
  );

  useEffect(() => {
    if (!selected) {
      requestSeq.current += 1;
      setComputing(false);
      setResult(null);
      prevSelectedId.current = null;
      return;
    }

    const selectionChanged = prevSelectedId.current !== selectedId;
    prevSelectedId.current = selectedId;

    if (selectionChanged) {
      void runBlast(selected.index, depth);
      return;
    }

    const timer = window.setTimeout(() => {
      void runBlast(selected.index, depth);
    }, DEPTH_DEBOUNCE_MS);

    return () => window.clearTimeout(timer);
  }, [selectedId, selected, depth, runBlast]);

  if (!wasmReady) {
    return (
      <p class="text-muted mb-0">
        WASM engine required for blast-radius analysis. Serve the dashboard directory over HTTP.
      </p>
    );
  }

  return (
    <FunctionListLayout
      sidebar={
        <FunctionListSidebar
          count={functionCount}
          items={listItems}
          selectedId={selectedId}
          onSelect={setSelectedId}
          loading={loading}
        />
      }
    >
      <div class="blast-view d-flex flex-column h-100 min-h-0 gap-3 p-3">
        <div class="d-flex flex-wrap align-items-center gap-3 flex-shrink-0">
          <ViewLegend
            hint="Sidebar meta: impact score · direct callers · impact zone"
            class="flex-grow-1 border rounded mb-0"
          />
          <div style={{ minWidth: "12rem" }}>
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
              disabled={!selected || computing}
              onInput={(e) => setDepth(Number((e.target as HTMLInputElement).value))}
            />
          </div>
          {computing && (
            <span class="text-muted small" aria-live="polite">
              Analyzing blast radius…
            </span>
          )}
        </div>

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
            Select a function from the list to see its blast radius. Functions are sorted by impact
            score (highest first). Adjust caller depth to change how far upstream to search.
          </p>
        )}
      </div>
    </FunctionListLayout>
  );
}
