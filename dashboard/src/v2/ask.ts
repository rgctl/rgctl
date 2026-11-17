/**
 * Semantic retrieval for the Ask surface.
 *
 * The semantic index is server-owned (`.rgctl/semantic_index.bin`), so this is the one part of the
 * dashboard that is not bundle data. `rgctl serve` exposes the index and graph expansion; a static
 * host can serve the shell but cannot answer these requests, which the UI states explicitly.
 */

export interface SemanticStatus {
  available: boolean;
  model_id?: string;
  dimensions?: number;
  functions_indexed?: number;
  graph_digest?: string;
  message?: string;
}

export interface SemanticHit {
  node_id: string;
  name: string;
  qualified_name?: string | null;
  file_path?: string | null;
  distance: number;
  score: number;
  fused_score?: number | null;
  ranking?: string | null;
}

export interface SemanticBlastSummary {
  anchor_node_id: string;
  anchor_name: string;
  direct_callers: number;
  impact_zone: number;
  score: number;
}

export interface SemanticNeighbor {
  node_id: string;
  name: string;
  qualified_name?: string | null;
  file_path?: string | null;
  relation: string;
  anchor_node_id?: string | null;
}

export interface SemanticExpansion {
  neighbors?: SemanticNeighbor[];
  blast?: SemanticBlastSummary[];
  gql?: SemanticNeighbor[];
}

export interface SemanticQueryResponse {
  schema_version: number;
  query: string;
  model_id: string;
  dimensions: number;
  hits: SemanticHit[];
  expansion?: SemanticExpansion;
}

export async function fetchSemanticStatus(): Promise<SemanticStatus> {
  const res = await fetch("/api/semantic/status");
  if (!res.ok) {
    return {
      available: false,
      message:
        res.status === 404
          ? "Semantic API not reachable. Run `rgctl serve` rather than serving the bundle statically."
          : `Semantic status failed (HTTP ${res.status}).`,
    };
  }
  return (await res.json()) as SemanticStatus;
}

export async function askSemantic(
  query: string,
  opts: { limit?: number; expand?: "all" | "blast" | "neighbors" | null } = {},
): Promise<SemanticQueryResponse> {
  const res = await fetch("/api/semantic/query", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify({
      query,
      limit: opts.limit ?? 20,
      fusion: true,
      keyword_and: false,
      candidate_pool: 256,
      expand: opts.expand === null ? undefined : (opts.expand ?? "blast"),
      expand_depth: 1,
      scope: "function",
    }),
  });
  const body = (await res.json()) as SemanticQueryResponse & { message?: string };
  if (!res.ok) {
    const detail =
      typeof body === "object" && body && "message" in body && body.message
        ? body.message
        : `HTTP ${res.status}`;
    throw new Error(detail);
  }
  return body;
}

/** Short, repo-relative-ish path for a result row. */
export function shortPath(filePath?: string | null, segments = 3): string {
  if (!filePath) return "—";
  const parts = filePath.split(/[/\\]/);
  return parts.length <= segments ? filePath : parts.slice(-segments).join("/");
}

export function hitLabel(hit: SemanticHit): string {
  return hit.qualified_name?.trim() || hit.name;
}

export function formatScore(value: number): string {
  return value.toFixed(3);
}

/**
 * The deterministic answer: retrieval + graph evidence, not a generated claim.
 *
 * `answerFor` deliberately describes what the index and expansion actually contain. When blast data
 * is absent it says so rather than inventing a caller count.
 */
export function answerFor(response: SemanticQueryResponse): {
  top: SemanticHit | null;
  blast: SemanticBlastSummary | null;
  headline: string;
  summary: string;
} {
  const top = response.hits[0] ?? null;
  if (!top) {
    return {
      top: null,
      blast: null,
      headline: "No function matched",
      summary:
        "The semantic index returned no hits. Try a broader phrase, or rebuild the index if the graph changed since it was written.",
    };
  }

  const blast =
    response.expansion?.blast?.find((b) => b.anchor_node_id === top.node_id) ?? null;
  const alternatives = response.hits
    .slice(1, 3)
    .map((h) => h.name)
    .filter((name) => name !== top.name);
  const location = shortPath(top.file_path);

  const evidence = blast
    ? `${blast.direct_callers} direct caller${blast.direct_callers === 1 ? "" : "s"} and an upstream impact zone of ${blast.impact_zone}.`
    : "Caller and impact evidence was not included for this hit.";
  const next =
    alternatives.length > 0 ? ` Closest alternatives: ${alternatives.join(", ")}.` : "";

  return {
    top,
    blast,
    headline: `${top.name} is the strongest match`,
    summary: `${hitLabel(top)} in ${location}. ${evidence}${next}`,
  };
}
