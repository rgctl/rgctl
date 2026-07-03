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
  const [items, setItems] = useState<NodeListEntry[]>([]);
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
      const limit = PAGE_SIZE;
      setLoading(true);
      try {
        const payload = await listNodes(typeMask, offset, limit);
        setTotal(payload.total);
        const cache = cacheRef.current;
        payload.items.forEach((item, i) => {
          cache.set(offset + i, item);
        });
        setItems(Array.from(cache.entries()).sort((a, b) => a[0] - b[0]).map(([, v]) => v));
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
    setItems([]);
    if (wasmReady) void loadRange(0);
  }, [typeMask, wasmReady, loadRange]);

  useEffect(() => {
    if (!wasmReady) return;
    const firstVisible = Math.floor(scrollTop / ROW_HEIGHT);
    const needOffset = Math.floor(firstVisible / PAGE_SIZE) * PAGE_SIZE;
    const cache = cacheRef.current;
    let missing = false;
    for (let i = needOffset; i < needOffset + PAGE_SIZE * 2 && i < total; i++) {
      if (!cache.has(i)) {
        missing = true;
        break;
      }
    }
    if (missing) void loadRange(needOffset);
  }, [scrollTop, total, wasmReady, loadRange]);

  const onScroll = () => {
    const el = scrollerRef.current;
    if (el) setScrollTop(el.scrollTop);
  };

  const visibleCount = Math.ceil((scrollerRef.current?.clientHeight ?? 400) / ROW_HEIGHT) + 2;
  const startIndex = Math.floor(scrollTop / ROW_HEIGHT);
  const endIndex = Math.min(total, startIndex + visibleCount);

  if (!wasmReady) {
    return (
      <div class="functions-view">
        <p class="placeholder">WASM engine loading — function table requires columnar payload parse.</p>
      </div>
    );
  }

  return (
    <div class="functions-view">
      <div class="functions-toolbar">
        <NodeTypeFilter mask={typeMask} onChange={setTypeMask} />
        <span class="functions-meta">
          {loading ? "Loading…" : `${total.toLocaleString()} nodes matching filter`}
        </span>
      </div>
      {error && <div class="banner banner-error">{error}</div>}

      <div
        class="functions-scroller"
        ref={scrollerRef}
        onScroll={onScroll}
        style={{ maxHeight: "480px" }}
      >
        <div style={{ height: `${total * ROW_HEIGHT}px`, position: "relative" }}>
          {Array.from({ length: endIndex - startIndex }, (_, i) => {
            const row = startIndex + i;
            const entry = cacheRef.current.get(row);
            return (
              <div
                key={row}
                class="functions-row"
                style={{
                  position: "absolute",
                  top: `${row * ROW_HEIGHT}px`,
                  height: `${ROW_HEIGHT}px`,
                  left: 0,
                  right: 0,
                }}
              >
                {entry ? (
                  <>
                    <span class="fn-name">{entry.name}</span>
                    <span class="fn-type">{entry.node_type_name}</span>
                    <span class="fn-cx">{entry.complexity.toFixed(1)}</span>
                    <span class="fn-idx">{entry.index}</span>
                  </>
                ) : (
                  <span class="fn-skeleton">…</span>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
