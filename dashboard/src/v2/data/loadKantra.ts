/**
 * Kantra / Static rules — index-first loaders for the universe bundle.
 *
 * Never fetch `.rgctl/kantra_findings.json` from the browser; only `kantra_index.json`
 * and on-demand `kantra/files/{id}.json` shards (plus source catalog).
 */

import { bundleUrl, loadSourceForPath, loadSourceIndex } from "../data";

export const KANTRA_FILTER_CATEGORIES = [
  "mandatory",
  "potential",
  "optional",
  "uncategorized",
] as const;

export type KantraFilterCategory = (typeof KANTRA_FILTER_CATEGORIES)[number];

export const ALL_KANTRA_CATEGORIES: ReadonlySet<KantraFilterCategory> = new Set(
  KANTRA_FILTER_CATEGORIES,
);

export interface KantraSection {
  available: boolean;
  violation_count?: number;
  evaluated_rules?: number;
  index_path?: string | null;
  detail_dir?: string | null;
  catalog_id?: string | null;
  ruleset?: string | null;
  target_filter?: string | null;
  file_count?: number;
  rule_count?: number;
}

export interface KantraTargetEntry {
  target: string;
  violation_count: number;
}

export interface KantraFileEntry {
  id: string;
  path: string;
  count: number;
  categories: Record<string, number>;
  max_blast: number;
  target_categories?: Record<string, Record<string, number>>;
  source_id?: string | null;
}

export interface KantraLink {
  title?: string | null;
  url: string;
}

export interface KantraSkippedRule {
  rule_id: string;
  reason: string;
}

export interface KantraRuleEntry {
  rule_id: string;
  count: number;
  category?: string | null;
  message?: string | null;
  description?: string | null;
  links?: KantraLink[];
  targets?: string[];
  sources?: string[];
  effort?: number | null;
}

export interface KantraViolation {
  rule_id: string;
  category?: string | null;
  file: string;
  line: number;
  message?: string | null;
  matched_by: string;
  symbol?: string | null;
  targets?: string[];
  sources?: string[];
  effort?: number | null;
  confidence?: string | null;
  matched_location?: string | null;
  enrichment?: {
    blast_radius_score?: number | null;
    impact_zone_size?: number | null;
    community_id?: number | null;
    pagerank?: number | null;
  } | null;
}

export interface KantraFileBundle {
  schema_version: number;
  path: string;
  source_id?: string | null;
  violations: KantraViolation[];
}

export interface KantraIndexPayload {
  schema_version: number;
  available: boolean;
  detail_dir: string;
  catalog_id?: string | null;
  ruleset: string;
  target_filter?: string | null;
  evaluated_rules: number;
  violation_count: number;
  by_category: Record<string, number>;
  file_count: number;
  rule_count: number;
  available_targets?: KantraTargetEntry[];
  available_sources?: string[];
  files: KantraFileEntry[];
  rules: KantraRuleEntry[];
  skipped_rules?: KantraSkippedRule[];
}

export type KantraIndexLoad =
  | { kind: "missing" }
  | { kind: "unavailable"; section: KantraSection | null }
  | { kind: "empty"; index: KantraIndexPayload }
  | { kind: "ready"; index: KantraIndexPayload };

async function getJson<T>(name: string): Promise<T | null> {
  const res = await fetch(bundleUrl(name));
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(`${name}: HTTP ${res.status}`);
  return (await res.json()) as T;
}

export function normalizeCategory(
  category: string | null | undefined,
): KantraFilterCategory {
  if (category === "mandatory" || category === "potential" || category === "optional") {
    return category;
  }
  return "uncategorized";
}

export function fileCategoryCount(
  entry: KantraFileEntry,
  categories: ReadonlySet<KantraFilterCategory>,
  targetFilter: string | null,
): number {
  if (targetFilter) {
    const perTarget = entry.target_categories?.[targetFilter];
    if (!perTarget) return 0;
    let count = 0;
    for (const cat of categories) count += perTarget[cat] ?? 0;
    return count;
  }
  let count = 0;
  for (const cat of categories) count += entry.categories[cat] ?? 0;
  return count;
}

export function fileMatchesFilters(
  entry: KantraFileEntry,
  categories: ReadonlySet<KantraFilterCategory>,
  targetFilter: string | null,
): boolean {
  if (categories.size === 0) return false;
  return fileCategoryCount(entry, categories, targetFilter) > 0;
}

export function violationMatchesFilters(
  violation: KantraViolation,
  rule: KantraRuleEntry | undefined,
  categories: ReadonlySet<KantraFilterCategory>,
  targetFilter: string | null,
  sourceFilter: string | null = null,
  highConfidenceOnly = false,
): boolean {
  if (!categories.has(normalizeCategory(violation.category))) return false;
  if (highConfidenceOnly && violation.confidence === "low") return false;
  if (sourceFilter) {
    const sources = violation.sources?.length
      ? violation.sources
      : (rule?.sources ?? []);
    if (!sources.includes(sourceFilter)) return false;
  }
  if (!targetFilter) return true;
  const fromViolation = violation.targets ?? [];
  if (fromViolation.includes(targetFilter)) return true;
  return (rule?.targets ?? []).includes(targetFilter);
}

export function violationKey(v: KantraViolation, index: number): string {
  return `${v.rule_id}:${v.line}:${index}`;
}

export function shortPath(path: string): string {
  const parts = path.split(/[/\\]/);
  if (parts.length <= 2) return path;
  return parts.slice(-2).join("/");
}

/** Live session hint from `rgctl serve` `/api/status` (optional when serving static-only). */
export async function loadSessionKantraFindings(): Promise<boolean | null> {
  try {
    const res = await fetch("/api/status");
    if (!res.ok) return null;
    const body = (await res.json()) as { kantra_findings?: boolean };
    return typeof body.kantra_findings === "boolean" ? body.kantra_findings : null;
  } catch {
    return null;
  }
}

export async function loadKantraIndex(
  section: KantraSection | null | undefined,
): Promise<KantraIndexLoad> {
  if (!section?.available) {
    return { kind: "unavailable", section: section ?? null };
  }
  const path = section.index_path ?? "kantra_index.json";
  const index = await getJson<KantraIndexPayload>(path);
  if (!index || !index.available) {
    return { kind: "unavailable", section };
  }
  if (index.violation_count === 0) {
    return { kind: "empty", index };
  }
  return { kind: "ready", index };
}

export async function loadKantraFileBundle(
  detailDir: string,
  fileId: string,
): Promise<KantraFileBundle | null> {
  return getJson<KantraFileBundle>(`${detailDir}/files/${fileId}.json`);
}

/** Resolve source text for a Kantra file entry / bundle path. */
export async function loadKantraSourceText(
  sourceId: string | null | undefined,
  filePath: string,
): Promise<string | null> {
  if (sourceId) {
    const res = await fetch(bundleUrl(`sources/${sourceId}.txt`));
    if (res.ok) return res.text();
  }
  const byPath = await loadSourceForPath(filePath);
  if (byPath) return byPath;
  // Absolute paths in shards: try basename match against the source index.
  const index = await loadSourceIndex();
  const base = filePath.split(/[/\\]/).pop();
  if (!base) return null;
  const hit = index.find((e) => e.file_path.endsWith(`/${base}`) || e.file_path.endsWith(`\\${base}`));
  if (!hit) return null;
  return loadSourceForPath(hit.file_path);
}

export function snippetRange(
  source: string,
  line: number,
  context = 4,
): { text: string; firstLine: number } | null {
  const lines = source.split("\n");
  if (lines.length === 0) return null;
  const start = Math.max(0, line - 1 - context);
  const end = Math.min(lines.length, line + context);
  return {
    text: lines.slice(start, end).join("\n"),
    firstLine: start + 1,
  };
}
