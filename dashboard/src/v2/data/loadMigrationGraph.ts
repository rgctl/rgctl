/**
 * Fetch + validate `migration_graph.json` (schema_version 3, package_macro).
 *
 * Missing / unreadable → `{ kind: "missing" }`.
 * Present with zero communities → `{ kind: "empty", graph }` (distinct from missing).
 * Present with communities → `{ kind: "ready", graph }`.
 */
import type { MigrationGraphPayload } from "../../migration/types";
import { bundleUrl } from "../data";

export type MigrationGraphLoad =
  | { kind: "missing" }
  | { kind: "empty"; graph: MigrationGraphPayload }
  | { kind: "ready"; graph: MigrationGraphPayload };

export async function loadMigrationGraph(
  fetchImpl: typeof fetch = fetch,
): Promise<MigrationGraphLoad> {
  try {
    const res = await fetchImpl(bundleUrl("migration_graph.json"), { cache: "no-store" });
    if (!res.ok) return { kind: "missing" };
    const raw: unknown = await res.json();
    return classifyMigrationGraph(raw);
  } catch {
    return { kind: "missing" };
  }
}

/** Pure classify for unit tests (no fetch). */
export function classifyMigrationGraph(raw: unknown): MigrationGraphLoad {
  const graph = parseMigrationGraph(raw);
  if (!graph) return { kind: "missing" };
  if (graph.communities.length === 0) return { kind: "empty", graph };
  return { kind: "ready", graph };
}

export function parseMigrationGraph(raw: unknown): MigrationGraphPayload | null {
  if (!raw || typeof raw !== "object") return null;
  const o = raw as Record<string, unknown>;
  if (o.schema_version !== 3) return null;
  if (o.mode !== "package_macro") return null;
  if (!Array.isArray(o.communities) || !Array.isArray(o.edges)) return null;
  if (typeof o.modularity !== "number") return null;
  return raw as MigrationGraphPayload;
}
