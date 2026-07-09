import type {
  CfgFunctionEntry,
  DataflowFunctionEntry,
  NodeListEntry,
  SliceFunctionEntry,
  TaintFunctionEntry,
} from "./types";

export interface FunctionListItem {
  id: string;
  name: string;
  filePath?: string | null;
  meta?: string;
  badge?: string;
  badgeClass?: string;
}

export const FUNCTION_LIST_PAGE_SIZE = 30;
export const FUNCTION_METRICS_BATCH_SIZE = 500;

export type FunctionSortKey =
  | "name"
  | "node_type_name"
  | "pagerank"
  | "betweenness"
  | "harmonic"
  | "blast_score";

export type SortDirection = "asc" | "desc";

export const FUNCTION_COLUMN_TOOLTIPS: Record<FunctionSortKey, string> = {
  name: "Symbol name from the indexed graph.",
  node_type_name: "Node kind (Function, Class, Module, etc.).",
  pagerank:
    "PageRank (PR): global influence on the behavioral call graph. Scores are normalized fractions that sum to ~1 across all nodes, so values look small (e.g. 2.46e-4 = 0.000246). Rank order matters more than the absolute number.",
  betweenness:
    "Betweenness (BC): how often this symbol sits on shortest paths between others — bridges and bottlenecks score high. Sampled approximation on graphs above 500 nodes. — means zero (not on meaningful paths).",
  harmonic:
    "Harmonic centrality (Harm): average closeness to reachable nodes (inverse distance). High values mean the symbol can reach much of the graph quickly. HyperBall approximation on large graphs. — means zero.",
  blast_score:
    "Blast radius score (0–100): upstream caller impact from discover — how many callers are affected if this symbol changes. Higher = wider blast radius.",
};

/** Display centrality: scientific notation below 0.01, else 3 decimal places. */
export function formatCentrality(value: number | undefined): string {
  if (value === undefined || value <= 0) return "—";
  if (value >= 0.01) return value.toFixed(3);
  return value.toExponential(2);
}

/** Full precision for cell hover tooltips. */
export function formatCentralityTooltip(value: number | undefined): string | undefined {
  if (value === undefined || value <= 0) return undefined;
  return value.toPrecision(6);
}

export function formatBlast(value: number | undefined): string {
  if (value === undefined || value <= 0) return "—";
  return value.toFixed(0);
}

export function formatBlastTooltip(value: number | undefined): string | undefined {
  if (value === undefined || value <= 0) return undefined;
  return value.toFixed(2);
}

/** Combined column explanation + cell value for row tooltips. */
export function functionCellTooltip(column: FunctionSortKey, entry: NodeListEntry): string {
  const base = FUNCTION_COLUMN_TOOLTIPS[column];
  switch (column) {
    case "name": {
      const path = entry.file_path ? `\nFile: ${entry.file_path}` : "";
      return `${base}${path}`;
    }
    case "node_type_name":
      return `${base}\nThis row: ${entry.node_type_name}`;
    case "pagerank": {
      const v = formatCentralityTooltip(entry.pagerank);
      return v ? `${base}\nValue: ${v}` : `${base}\n— no score`;
    }
    case "betweenness": {
      const v = formatCentralityTooltip(entry.betweenness);
      return v ? `${base}\nValue: ${v}` : `${base}\n— no score`;
    }
    case "harmonic": {
      const v = formatCentralityTooltip(entry.harmonic);
      return v ? `${base}\nValue: ${v}` : `${base}\n— no score`;
    }
    case "blast_score": {
      const v = formatBlastTooltip(entry.blast_score);
      return v ? `${base}\nValue: ${v}` : `${base}\n— no score`;
    }
  }
}

function metricSortValue(value: number | undefined): number {
  return value !== undefined && value > 0 ? value : -1;
}

export function filterNodeEntries(entries: NodeListEntry[], search: string): NodeListEntry[] {
  const q = search.trim().toLowerCase();
  if (!q) return entries;
  return entries.filter((entry) => {
    const path = entry.file_path?.toLowerCase() ?? "";
    return entry.name.toLowerCase().includes(q) || path.includes(q);
  });
}

export function sortNodeEntries(
  entries: NodeListEntry[],
  sortKey: FunctionSortKey,
  direction: SortDirection,
): NodeListEntry[] {
  const factor = direction === "asc" ? 1 : -1;
  return [...entries].sort((a, b) => {
    let cmp = 0;
    switch (sortKey) {
      case "name":
      case "node_type_name":
        cmp = a[sortKey].localeCompare(b[sortKey]);
        break;
      case "pagerank":
      case "betweenness":
      case "harmonic":
      case "blast_score": {
        const av =
          sortKey === "blast_score"
            ? metricSortValue(a.blast_score)
            : metricSortValue(a[sortKey]);
        const bv =
          sortKey === "blast_score"
            ? metricSortValue(b.blast_score)
            : metricSortValue(b[sortKey]);
        cmp = av - bv;
        if (cmp === 0) {
          cmp = a.name.localeCompare(b.name);
        }
        break;
      }
    }
    return cmp * factor;
  });
}

export async function loadAllNodeEntries(
  listNodes: (typeMask: number, offset: number, limit: number) => Promise<import("./types").NodeListPayload>,
  typeMask: number,
  hintCount: number,
): Promise<NodeListEntry[]> {
  const target = Math.min(Math.max(hintCount, 1), 50_000);
  const all: NodeListEntry[] = [];
  for (let offset = 0; offset < target; offset += FUNCTION_METRICS_BATCH_SIZE) {
    const page = await listNodes(typeMask, offset, FUNCTION_METRICS_BATCH_SIZE);
    all.push(...page.items);
    if (page.items.length === 0 || all.length >= page.total) break;
  }
  return all;
}

export function shortPath(p: string): string {
  const parts = p.split(/[/\\]/);
  if (parts.length <= 2) return p;
  return `…/${parts.slice(-2).join("/")}`;
}

export function fileLabel(filePath?: string | null): string | undefined {
  if (!filePath) return undefined;
  const parts = filePath.split(/[/\\]/);
  return parts[parts.length - 1] ?? filePath;
}

export function filterFunctionItems(items: FunctionListItem[], search: string): FunctionListItem[] {
  const q = search.trim().toLowerCase();
  if (!q) return items;
  return items.filter((item) => {
    const path = item.filePath?.toLowerCase() ?? "";
    return item.name.toLowerCase().includes(q) || path.includes(q) || (item.meta?.toLowerCase().includes(q) ?? false);
  });
}

export function cfgEntryToListItem(entry: CfgFunctionEntry): FunctionListItem {
  return {
    id: entry.function_id,
    name: entry.name,
    filePath: entry.file_path,
    meta: `${entry.block_count} block${entry.block_count === 1 ? "" : "s"}`,
  };
}

export function dataflowEntryToListItem(entry: DataflowFunctionEntry): FunctionListItem {
  const blocks = entry.block_count ?? 0;
  const metaParts: string[] = [];
  if (entry.file_path) metaParts.push(fileLabel(entry.file_path) ?? entry.file_path);
  metaParts.push(`${entry.data_edges} data flow${entry.data_edges === 1 ? "" : "s"}`);
  metaParts.push(`${blocks} block${blocks === 1 ? "" : "s"}`);
  return {
    id: entry.function_id,
    name: entry.name,
    filePath: entry.file_path,
    meta: metaParts.join(" · "),
  };
}

export function sliceEntryToListItem(entry: SliceFunctionEntry): FunctionListItem {
  const metaParts: string[] = [];
  if (entry.file_path) metaParts.push(fileLabel(entry.file_path) ?? entry.file_path);
  metaParts.push(`${entry.pdg_nodes} node${entry.pdg_nodes === 1 ? "" : "s"}`);
  return {
    id: entry.function_id,
    name: entry.name,
    filePath: entry.file_path,
    meta: metaParts.join(" · "),
  };
}

export function taintEntryToListItem(entry: TaintFunctionEntry): FunctionListItem {
  return {
    id: entry.function_id,
    name: entry.name,
    filePath: entry.file_path,
    meta: `${entry.flow_count} flow${entry.flow_count === 1 ? "" : "s"}`,
    badge: entry.vulnerable_count > 0 ? `${entry.vulnerable_count} vuln` : undefined,
    badgeClass: entry.vulnerable_count > 0 ? "bg-danger" : undefined,
  };
}

export function nodeEntryToListItem(entry: NodeListEntry): FunctionListItem {
  const metaParts: string[] = [];
  if (entry.file_path) metaParts.push(fileLabel(entry.file_path) ?? entry.file_path);
  if (entry.pagerank !== undefined && entry.pagerank > 0) {
    metaParts.push(`pr ${entry.pagerank.toFixed(4)}`);
  }
  if (entry.blast_score > 0) metaParts.push(`blast ${entry.blast_score.toFixed(0)}`);
  return {
    id: String(entry.index),
    name: entry.name,
    filePath: entry.file_path,
    meta: metaParts.join(" · "),
  };
}

export function blastEntryToListItem(
  entry: NodeListEntry,
  score?: { score: number; direct: number; zone: number } | null,
): FunctionListItem {
  const metaParts: string[] = [];
  if (score && score.score > 0) {
    metaParts.push(`score ${score.score.toFixed(1)}`);
    metaParts.push(`${score.direct} direct`);
    metaParts.push(`${score.zone} zone`);
  } else if (entry.blast_score > 0) {
    metaParts.push(`score ${entry.blast_score.toFixed(0)}`);
  }
  return {
    id: String(entry.index),
    name: entry.name,
    filePath: entry.file_path,
    meta: metaParts.length > 0 ? metaParts.join(" · ") : undefined,
  };
}
