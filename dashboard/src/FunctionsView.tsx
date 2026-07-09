import { useCallback, useEffect, useMemo, useState } from "preact/hooks";
import { ColumnHelp } from "./ColumnHelp";
import { NodeTypeFilter } from "./NodeTypeFilter";
import {
  filterNodeEntries,
  formatBlast,
  formatCentrality,
  FUNCTION_COLUMN_TOOLTIPS,
  functionCellTooltip,
  FUNCTION_LIST_PAGE_SIZE,
  loadAllNodeEntries,
  sortNodeEntries,
  type FunctionSortKey,
  type SortDirection,
} from "./functionListUtils";
import type { NodeListEntry } from "./types";
import { NODE_TYPE_MASK } from "./types";

export interface FunctionsViewProps {
  wasmReady: boolean;
  functionCount: number;
  listNodes: (typeMask: number, offset: number, limit: number) => Promise<import("./types").NodeListPayload>;
}

interface ColumnDef {
  key: FunctionSortKey;
  label: string;
  className?: string;
}

const COLUMNS: ColumnDef[] = [
  { key: "name", label: "Name", className: "fn-name" },
  { key: "node_type_name", label: "Type" },
  { key: "pagerank", label: "PR" },
  { key: "betweenness", label: "BC" },
  { key: "harmonic", label: "Harm" },
  { key: "blast_score", label: "Blast" },
];

function nextSort(
  currentKey: FunctionSortKey,
  currentDir: SortDirection,
  clicked: FunctionSortKey,
): { key: FunctionSortKey; dir: SortDirection } {
  if (currentKey === clicked) {
    return { key: clicked, dir: currentDir === "desc" ? "asc" : "desc" };
  }
  const defaultDesc: FunctionSortKey[] = ["pagerank", "betweenness", "harmonic", "blast_score"];
  return { key: clicked, dir: defaultDesc.includes(clicked) ? "desc" : "asc" };
}

function SortableHeader({
  column,
  sortKey,
  sortDir,
  onSort,
}: {
  column: ColumnDef;
  sortKey: FunctionSortKey;
  sortDir: SortDirection;
  onSort: (key: FunctionSortKey) => void;
}) {
  const active = sortKey === column.key;
  const indicator = active ? (sortDir === "asc" ? " ▲" : " ▼") : "";
  return (
    <th scope="col" class="functions-sort-header">
      <div class="functions-sort-header-inner">
        <button
          type="button"
          class={`btn btn-link btn-sm p-0 text-decoration-none functions-sort-btn ${active ? "fw-semibold text-body" : "text-muted"}`}
          aria-label={`Sort by ${column.label}`}
          aria-sort={active ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
          onClick={() => onSort(column.key)}
        >
          {column.label}
          {indicator}
        </button>
        <ColumnHelp text={FUNCTION_COLUMN_TOOLTIPS[column.key]} />
      </div>
    </th>
  );
}

export function FunctionsView({ wasmReady, functionCount, listNodes }: FunctionsViewProps) {
  const [typeMask, setTypeMask] = useState(NODE_TYPE_MASK.Function);
  const [allRows, setAllRows] = useState<NodeListEntry[]>([]);
  const [search, setSearch] = useState("");
  const [sortKey, setSortKey] = useState<FunctionSortKey>("pagerank");
  const [sortDir, setSortDir] = useState<SortDirection>("desc");
  const [page, setPage] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const loadAll = useCallback(async () => {
    if (!wasmReady) return;
    setLoading(true);
    try {
      const entries = await loadAllNodeEntries(listNodes, typeMask, functionCount);
      setAllRows(entries);
      setError(null);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setAllRows([]);
    } finally {
      setLoading(false);
    }
  }, [wasmReady, listNodes, typeMask, functionCount]);

  useEffect(() => {
    void loadAll();
  }, [loadAll]);

  useEffect(() => {
    setPage(0);
  }, [typeMask, search, sortKey, sortDir]);

  const filtered = useMemo(() => filterNodeEntries(allRows, search), [allRows, search]);
  const sorted = useMemo(
    () => sortNodeEntries(filtered, sortKey, sortDir),
    [filtered, sortKey, sortDir],
  );

  const total = sorted.length;
  const pageCount = Math.max(1, Math.ceil(total / FUNCTION_LIST_PAGE_SIZE));
  const safePage = Math.min(page, pageCount - 1);
  const rangeStart = total === 0 ? 0 : safePage * FUNCTION_LIST_PAGE_SIZE + 1;
  const rangeEnd = Math.min((safePage + 1) * FUNCTION_LIST_PAGE_SIZE, total);
  const rows = sorted.slice(
    safePage * FUNCTION_LIST_PAGE_SIZE,
    (safePage + 1) * FUNCTION_LIST_PAGE_SIZE,
  );

  useEffect(() => {
    setPage((current) => Math.min(current, Math.max(0, pageCount - 1)));
  }, [pageCount]);

  const handleSort = (key: FunctionSortKey) => {
    const next = nextSort(sortKey, sortDir, key);
    setSortKey(next.key);
    setSortDir(next.dir);
  };

  if (!wasmReady) {
    return <p class="text-muted small">WASM engine loading…</p>;
  }

  return (
    <div class="functions-view h-100 d-flex flex-column">
      <div class="d-flex flex-wrap align-items-center gap-3 mb-3 flex-shrink-0">
        <NodeTypeFilter mask={typeMask} onChange={setTypeMask} />
        <input
          type="search"
          class="form-control form-control-sm functions-search"
          placeholder="Search by name or file…"
          value={search}
          onInput={(e) => setSearch((e.target as HTMLInputElement).value)}
          aria-label="Search functions"
        />
        <span class="small text-muted">
          {loading
            ? "Loading…"
            : search.trim()
              ? `${total.toLocaleString()} of ${allRows.length.toLocaleString()} matching`
              : `${total.toLocaleString()} nodes matching filter`}
        </span>
      </div>

      {error && <div class="alert alert-danger py-2 small">{error}</div>}

      <div class="functions-scroller border rounded flex-grow-1 min-h-0 overflow-auto">
        <table class="table table-sm table-hover mb-0">
          <thead class="table-light sticky-top">
            <tr>
              {COLUMNS.map((col) => (
                <SortableHeader
                  key={col.key}
                  column={col}
                  sortKey={sortKey}
                  sortDir={sortDir}
                  onSort={handleSort}
                />
              ))}
            </tr>
          </thead>
          <tbody>
            {loading && rows.length === 0 ? (
              <tr>
                <td colSpan={6} class="text-muted small">
                  Loading…
                </td>
              </tr>
            ) : rows.length === 0 ? (
              <tr>
                <td colSpan={6} class="text-muted small">
                  {search.trim() ? "No nodes match your search." : "No nodes match this filter."}
                </td>
              </tr>
            ) : (
              rows.map((entry) => (
                <tr key={`${entry.index}-${entry.name}`}>
                  <td class="fn-name small" title={functionCellTooltip("name", entry)}>
                    {entry.name}
                  </td>
                  <td class="small text-muted" title={functionCellTooltip("node_type_name", entry)}>
                    {entry.node_type_name}
                  </td>
                  <td class="small text-muted" title={functionCellTooltip("pagerank", entry)}>
                    {formatCentrality(entry.pagerank)}
                  </td>
                  <td class="small text-muted" title={functionCellTooltip("betweenness", entry)}>
                    {formatCentrality(entry.betweenness)}
                  </td>
                  <td class="small text-muted" title={functionCellTooltip("harmonic", entry)}>
                    {formatCentrality(entry.harmonic)}
                  </td>
                  <td class="small text-muted" title={functionCellTooltip("blast_score", entry)}>
                    {formatBlast(entry.blast_score)}
                  </td>
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
