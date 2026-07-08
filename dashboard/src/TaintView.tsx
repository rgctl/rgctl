import { useEffect, useMemo, useState } from "preact/hooks";
import { bundleDataUrl } from "./bundleUrl";
import { FunctionListLayout, FunctionListSidebar } from "./FunctionListSidebar";
import { taintEntryToListItem } from "./functionListUtils";
import { ViewLegend } from "./ViewLegend";
import { TAINT_SEVERITY_LEGEND, TAINT_STATUS_LEGEND } from "./viewLegendData";
import type {
  TaintBundlePayload,
  TaintFlowView,
  TaintFunctionEntry,
  TaintIndexPayload,
} from "./types";

function severityBadge(severity: number): string {
  if (severity >= 9) return "danger";
  if (severity >= 7) return "warning";
  if (severity >= 5) return "info";
  return "secondary";
}

export function TaintView() {
  const [index, setIndex] = useState<TaintIndexPayload | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [bundle, setBundle] = useState<TaintBundlePayload | null>(null);
  const [vulnerableOnly, setVulnerableOnly] = useState(false);
  const [selectedFlowId, setSelectedFlowId] = useState<number | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    let cancelled = false;
    fetch(bundleDataUrl("taint_index.json"))
      .then((r) => {
        if (!r.ok) throw new Error(`taint_index.json HTTP ${r.status}`);
        return r.json();
      })
      .then((data: TaintIndexPayload) => {
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
      setBundle(null);
      setSelectedFlowId(null);
      return;
    }
    let cancelled = false;
    setLoading(true);
    fetch(bundleDataUrl(`${index.detail_dir}/${selectedId}.json`))
      .then((r) => {
        if (!r.ok) throw new Error(`taint bundle HTTP ${r.status}`);
        return r.json();
      })
      .then((data: TaintBundlePayload) => {
        if (!cancelled) {
          setBundle(data);
          setSelectedFlowId(data.flows.length > 0 ? data.flows[0].id : null);
        }
      })
      .catch((e) => {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [selectedId, index?.available, index?.detail_dir]);

  const functions: TaintFunctionEntry[] = index?.functions ?? [];

  const visibleFlows = useMemo(() => {
    if (!bundle) return [];
    return vulnerableOnly ? bundle.flows.filter((f) => f.vulnerable) : bundle.flows;
  }, [bundle, vulnerableOnly]);

  const selectedFlow: TaintFlowView | null =
    bundle?.flows.find((f) => f.id === selectedFlowId) ?? null;

  if (error) {
    return <div class="alert alert-danger small mb-0">{error}</div>;
  }

  if (!index) {
    return <p class="text-muted small mb-0">Loading taint index…</p>;
  }

  if (!index.available) {
    return (
      <div>
        <p class="text-muted mb-2">
          Taint analysis is not in this bundle. Run discover with CFG/PDG enabled:
        </p>
        <pre class="bg-light border rounded p-3 small mb-0">rbuilder discover . --languages java --cfg</pre>
      </div>
    );
  }

  return (
    <FunctionListLayout
      sidebar={
        <FunctionListSidebar
          count={index.function_count}
          items={functions.map(taintEntryToListItem)}
          selectedId={selectedId}
          onSelect={setSelectedId}
        />
      }
    >
      <div class="taint-view d-flex flex-column h-100 min-h-0 gap-3 p-3">
        <div class="d-flex flex-wrap align-items-center gap-3 flex-shrink-0">
          <div class="form-check mb-0">
            <input
              class="form-check-input"
              type="checkbox"
              id="taint-vuln-only"
              checked={vulnerableOnly}
              onChange={(e) => setVulnerableOnly((e.target as HTMLInputElement).checked)}
            />
            <label class="form-check-label small" for="taint-vuln-only">
              Vulnerable only
            </label>
          </div>
          <div class="small text-muted ms-auto">
            {index.total_flows} flows · {index.vulnerable_flows} vulnerable · {index.function_count}{" "}
            functions
          </div>
        </div>

        {loading && <p class="text-muted small mb-0">Loading flows…</p>}

        {!selectedId && !loading && (
          <p class="text-muted small mb-0">Select a function to inspect taint flows.</p>
        )}

        {bundle && !loading && (
        <div class="row g-3">
          <div class="col-lg-7">
            <div class="table-responsive border rounded">
              <table class="table table-sm table-hover mb-0 small">
                <thead class="table-light">
                  <tr>
                    <th>Severity</th>
                    <th>Status</th>
                    <th>Variable</th>
                    <th>Source → Sink</th>
                  </tr>
                </thead>
                <tbody>
                  {visibleFlows.length === 0 ? (
                    <tr>
                      <td colSpan={4} class="text-muted">
                        No flows match the filter.
                      </td>
                    </tr>
                  ) : (
                    visibleFlows.map((flow) => (
                      <tr
                        key={flow.id}
                        class={selectedFlowId === flow.id ? "table-primary" : ""}
                        style={{ cursor: "pointer" }}
                        onClick={() => setSelectedFlowId(flow.id)}
                      >
                        <td>
                          <span class={`badge bg-${severityBadge(flow.severity)}`}>{flow.severity}</span>
                        </td>
                        <td>
                          {flow.vulnerable ? (
                            <span class="badge bg-danger">Vulnerable</span>
                          ) : (
                            <span class="badge bg-success">Sanitized</span>
                          )}
                        </td>
                        <td>
                          <code>{flow.variable}</code>
                        </td>
                        <td>
                          {flow.source_type} → {flow.sink_type}
                        </td>
                      </tr>
                    ))
                  )}
                </tbody>
              </table>
            </div>
            <ViewLegend
              hint="Badges"
              items={[...TAINT_SEVERITY_LEGEND, ...TAINT_STATUS_LEGEND]}
              class="mt-2 border rounded"
            />
          </div>

          <div class="col-lg-5">
            {selectedFlow ? (
              <div class="border rounded p-3 small">
                <h3 class="h6 mb-2">Flow detail</h3>
                <dl class="row mb-2 g-1">
                  <dt class="col-4">Variable</dt>
                  <dd class="col-8">
                    <code>{selectedFlow.variable}</code>
                  </dd>
                  <dt class="col-4">Source</dt>
                  <dd class="col-8">
                    {selectedFlow.source_type}
                    {selectedFlow.source_line > 0 && (
                      <>
                        {" "}
                        · L{selectedFlow.source_line}
                      </>
                    )}
                  </dd>
                  <dt class="col-4">Sink</dt>
                  <dd class="col-8">
                    {selectedFlow.sink_type}
                    {selectedFlow.sink_line > 0 && (
                      <>
                        {" "}
                        · L{selectedFlow.sink_line}
                      </>
                    )}
                  </dd>
                  {selectedFlow.sanitizers.length > 0 && (
                    <>
                      <dt class="col-4">Sanitizers</dt>
                      <dd class="col-8">{selectedFlow.sanitizers.join(", ")}</dd>
                    </>
                  )}
                </dl>
                {selectedFlow.source_text && (
                  <pre class="bg-light border rounded p-2 mb-2 small mb-2">{selectedFlow.source_text}</pre>
                )}
                {selectedFlow.path_lines.length > 0 && (
                  <>
                    <h4 class="h6 mb-1">Path ({selectedFlow.path_lines.length} steps)</h4>
                    <ol class="mb-0 ps-3">
                      {selectedFlow.path_statements.map((stmt, i) => (
                        <li key={i} class="mb-1">
                          <span class="text-muted">L{selectedFlow.path_lines[i]}</span> {stmt}
                        </li>
                      ))}
                    </ol>
                  </>
                )}
              </div>
            ) : (
              <p class="text-muted small mb-0">Select a flow to inspect its path.</p>
            )}
          </div>
        </div>
      )}
      </div>
    </FunctionListLayout>
  );
}
