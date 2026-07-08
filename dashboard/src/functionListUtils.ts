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
  return {
    id: entry.function_id,
    name: entry.name,
    filePath: entry.file_path,
    meta: `${entry.data_edges} data flow${entry.data_edges === 1 ? "" : "s"} · ${blocks} block${blocks === 1 ? "" : "s"}`,
  };
}

export function sliceEntryToListItem(entry: SliceFunctionEntry): FunctionListItem {
  return {
    id: entry.function_id,
    name: entry.name,
    filePath: entry.file_path,
    meta: `${entry.pdg_nodes} node${entry.pdg_nodes === 1 ? "" : "s"}`,
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
  metaParts.push(`cx ${entry.complexity.toFixed(1)}`);
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
  if (entry.complexity > 0) metaParts.push(`cx ${entry.complexity.toFixed(1)}`);
  return {
    id: String(entry.index),
    name: entry.name,
    filePath: entry.file_path,
    meta: metaParts.length > 0 ? metaParts.join(" · ") : undefined,
  };
}
