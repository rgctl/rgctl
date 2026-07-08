import { useCallback, useEffect, useState } from "preact/hooks";
import { NodeTypeFilter } from "./NodeTypeFilter";
import { FUNCTION_LIST_PAGE_SIZE } from "./functionListUtils";
import type { NodeListEntry, NodeListPayload } from "./types";
import { NODE_TYPE_MASK } from "./types";

export interface FunctionsViewProps {
  wasmReady: boolean;
  functionCount: number;
  listNodes: (typeMask: number, offset: number, limit: number) => Promise<NodeListPayload>;
}

export function FunctionsView({ wasmReady, functionCount, listNodes }: FunctionsViewProps) {
  const [typeMask, setTypeMask] = useState(NODE_TYPE_MASK.Function);
  const [total, setTotal] = useState(functionCount);
  const [page, setPage] = useState(0);
  const [rows, setRows] = useState<NodeListEntry[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const pageCount = Math.max(1, Math.ceil(total / FUNCTION_LIST_PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const rangeStart = total === 0 ? 0 : safePage * FUNCTION_LIST_PAGE_SIZE + 1;
  const rangeEnd = Math.min((safePage + 1) * FUNCTION_LIST_PAGE_SIZE, total);

  const loadPage = useCallback(
    async (pageIndex: number) => {
      if (!wasmReady) return;
      setLoading(true);
      try {
        const offset = pageIndex * FUNCTION_LIST_PAGE_SIZE;
        const payload = await listNodes(typeMask, offset, FUNCTION_LIST_PAGE_SIZE);
        setTotal(payload.total);
        setRows(payload.items);
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
        setRows([]);
      } finally {
        setLoading(false);
      }
    },
    [listNodes, typeMask, wasmReady],
  );

  useEffect(() => {
    setPage(0);
  }, [typeMask]);

  useEffect(() => {
    if (wasmReady) void loadPage(safePage);
  }, [wasmReady, safePage, loadPage]);

  useEffect(() => {
    setPage((current) => Math.min(current, Math.max(0, pageCount - 1)));
  }, [pageCount]);

  if (!wasmReady) {
    return <p class="text-muted small">WASM engine loading…</p>;
  }

  return (
    <div class="functions-view h-100 d-flex flex-column">
      <div class="d-flex flex-wrap align-items-center gap-3 mb-3 flex-shrink-0">
        <NodeTypeFilter mask={typeMask} onChange={setTypeMask} />
        <span class="small text-muted">
          {loading ? "Loading…" : `${total.toLocaleString()} nodes matching filter`}
        </span>
      </div>

      {error && <div class="alert alert-danger py-2 small">{error}</div>}

      <div class="functions-scroller border rounded flex-grow-1 min-h-0 overflow-auto">
        <table class="table table-sm table-hover mb-0">
          <thead class="table-light sticky-top">
            <tr>
              <th>Name</th>
              <th>Type</th>
              <th>Cx</th>
              <th>Blast</th>
              <th>Idx</th>
            </tr>
          </thead>
          <tbody>
            {loading && rows.length === 0 ? (
              <tr>
                <td colSpan={5} class="text-muted small">
                  Loading…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={5} class="text-muted small">
                  No nodes match this filter.
                </td>
              </tr>
            ) : (
              rows.map((entry) => (
                <tr key={`${entry.index}-${entry.name}`}>
                  <td class="fn-name small">{entry.name}</td>
                  <td class="small text-muted">{entry.node_type_name}</td>
                  <td class="small text-muted">{entry.complexity.toFixed(1)}</td>
                  <td class="small text-muted">
                    {entry.blast_score > 0 ? entry.blast_score.toFixed(0) : "—"}
                  </td>
                  <td class="small text-muted">{entry.index}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {total > 0 && (
        <div class="d-flex align-items-center justify-content-between gap-2 mt-3 flex-shrink-0">
          <button
            type="button"
            class="btn btn-sm btn-outline-secondary"
            disabled={safePage <= 0 || loading}
            aria-label="Previous page"
            onClick={() => setPage((p) => Math.max(0, p - 1))}
          >
            Prev
          </button>
          <span class="small text-muted text-center flex-grow-1">
            {rangeStart}–{rangeEnd} of {total.toLocaleString()}
            {pageCount > 1 && ` · ${safePage + 1}/${pageCount}`}
          </span>
          <button
            type="button"
            class="btn btn-sm btn-outline-secondary"
            disabled={safePage >= pageCount - 1 || loading}
            aria-label="Next page"
            onClick={() => setPage((p) => Math.min(pageCount - 1, p + 1))}
          >
            Next
          </button>
        </div>
      )}
    </div>
  );
}
