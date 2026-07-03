import { useCallback, useEffect, useRef, useState } from "preact/hooks";
import { NodeTypeFilter } from "./NodeTypeFilter";
import type { NodeListEntry, NodeListPayload } from "./types";
import { NODE_TYPE_MASK } from "./types";

const ROW_HEIGHT = 36;
const PAGE_SIZE = 80;

export interface FunctionsViewProps {
  wasmReady: boolean;
  functionCount: number;
  listNodes: (typeMask: number, offset: number, limit: number) => Promise<NodeListPayload>;
}

export function FunctionsView({ wasmReady, functionCount, listNodes }: FunctionsViewProps) {
  const [typeMask, setTypeMask] = useState(NODE_TYPE_MASK.Function);
  const [total, setTotal] = useState(functionCount);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const scrollerRef = useRef<HTMLDivElement>(null);
  const cacheRef = useRef<Map<number, NodeListEntry>>(new Map());

  const loadRange = useCallback(
    async (startRow: number) => {
      if (!wasmReady) return;
      const offset = Math.max(0, startRow);
      setLoading(true);
      try {
        const payload = await listNodes(typeMask, offset, PAGE_SIZE);
        setTotal(payload.total);
        payload.items.forEach((item, i) => cacheRef.current.set(offset + i, item));
        setError(null);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [listNodes, typeMask, wasmReady],
  );

  useEffect(() => {
    cacheRef.current = new Map();
    if (wasmReady) void loadRange(0);
  }, [typeMask, wasmReady, loadRange]);

  useEffect(() => {
    if (!wasmReady) return;
    const firstVisible = Math.floor(scrollTop / ROW_HEIGHT);
    const needOffset = Math.floor(firstVisible / PAGE_SIZE) * PAGE_SIZE;
    let missing = false;
    for (let i = needOffset; i < needOffset + PAGE_SIZE * 2 && i < total; i++) {
      if (!cacheRef.current.has(i)) {
        missing = true;
        break;
      }
    }
    if (missing) void loadRange(needOffset);
  }, [scrollTop, total, wasmReady, loadRange]);

  if (!wasmReady) {
    return <p class="text-muted small">WASM engine loading…</p>;
  }

  const visibleCount = Math.ceil((scrollerRef.current?.clientHeight ?? 400) / ROW_HEIGHT) + 2;
  const startIndex = Math.floor(scrollTop / ROW_HEIGHT);
  const endIndex = Math.min(total, startIndex + visibleCount);

  return (
    <div class="functions-view h-100">
      <div class="d-flex flex-wrap align-items-center gap-3 mb-3 flex-shrink-0">
        <NodeTypeFilter mask={typeMask} onChange={setTypeMask} />
        <span class="small text-muted">
          {loading ? "Loading…" : `${total.toLocaleString()} nodes matching filter`}
        </span>
      </div>

      {error && <div class="alert alert-danger py-2 small">{error}</div>}

      <div
        class="functions-scroller border rounded"
        ref={scrollerRef}
        onScroll={() => {
          const el = scrollerRef.current;
          if (el) setScrollTop(el.scrollTop);
        }}
      >
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
            {Array.from({ length: endIndex - startIndex }, (_, i) => {
              const row = startIndex + i;
              const entry = cacheRef.current.get(row);
              return (
                <tr key={row} style={{ height: `${ROW_HEIGHT}px` }}>
                  {entry ? (
                    <>
                      <td class="fn-name small">{entry.name}</td>
                      <td class="small text-muted">{entry.node_type_name}</td>
                      <td class="small text-muted">{entry.complexity.toFixed(1)}</td>
                      <td class="small text-muted">
                        {entry.blast_score > 0 ? entry.blast_score.toFixed(0) : "—"}
                      </td>
                      <td class="small text-muted">{entry.index}</td>
                    </>
                  ) : (
                    <td colSpan={5} class="text-muted small">
                      …
                    </td>
                  )}
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>
    </div>
  );
}
