/**
 * The Review workflow artifact — **one module owns this contract.**
 *
 * `rgctl review export` composes one document describing what changed between two revisions. This
 * module is the only place that reads its fields; components receive a view model. That is deliberate:
 * the artifact's shape is under upstream review (sshaaf/rgctl#117), so a shape change should be an edit
 * here rather than a hunt through components.
 *
 * Written against a real export, not against the spec — the measured shape is noted where it surprised
 * us:
 *
 *   - `review_paths.symbols[].stable_key` is the analysis **cache** key (`file \x1f name \x1f code_hash`),
 *     so it changes whenever a function body changes — i.e. for exactly the symbols a review lists. It is
 *     therefore *not* usable to set the Focus, whose identity is `function_id` (the graph's deterministic
 *     node UUID). There is no `function_id` anywhere in the artifact today, so identity is resolved by
 *     joining file + name against the function index, and ambiguity is reported rather than guessed.
 *   - `provenance.graph.head_digest` is the same digest function as the bundle manifest's `graph.digest`
 *     (verified on a real fixture: both `0710662194bcf548…`). That is what makes the freshness check
 *     below meaningful instead of a comparison that never matches.
 */
import { bundleUrl } from "./data";
import type { FunctionEntry, Manifest } from "./data";

/** Bundle-relative path written by the dashboard exporter. */
export const REVIEW_WORKFLOW_PATH = "workflows/review.json";

/** The manifest's slice describing the carried artifact (`manifest.workflows`). */
export interface WorkflowsSection {
  available?: boolean;
  review_path?: string;
  review_digest?: string;
  review_generated_at?: string;
  review_head_digest?: string;
}

export interface TruncationFlags {
  symbols?: boolean;
  fanout?: boolean;
  depth?: boolean;
}

export interface CallEdgeDelta {
  kind: string;
  from: string;
  to: string;
}

export interface ReviewSymbol {
  stable_key?: string;
  name?: string;
  kind?: string;
  head?: { file?: string; start_line?: number | null; end_line?: number | null };
  path_before?: string[];
  path_after?: string[];
  path_delta?: CallEdgeDelta[];
  truncation?: TruncationFlags;
}

export interface ChangeSummary {
  changed_symbols?: number;
  call_edges?: { added?: number; removed?: number; retargeted?: number; unchanged?: number };
  files_in_scope?: number;
  unscored_files?: number;
}

export interface ReviewPaths {
  schema_version?: string;
  command?: string;
  change_summary?: ChangeSummary;
  truncation?: TruncationFlags;
  symbols?: ReviewSymbol[];
  unscored_files?: string[];
  ambiguous?: unknown[];
}

export interface ReviewProvenance {
  commands?: string[];
  graph?: {
    base_snapshot?: string;
    head_snapshot?: string;
    base_digest?: string;
    head_digest?: string;
  };
  /** Section name → why that section was not produced. The reason a section is absent, never a zero. */
  unavailable?: Record<string, string>;
}

export interface ReviewArtifact {
  schema_version?: string;
  command?: string;
  generated_at?: string;
  review_paths?: ReviewPaths;
  policy?: unknown;
  timeline?: unknown[];
  truncation?: TruncationFlags;
  provenance?: ReviewProvenance;
}

/** Whether the artifact describes the graph being browsed. */
export type Freshness = "current" | "different-revision" | "unknown";

export interface LoadedWorkflow {
  artifact: ReviewArtifact;
  freshness: Freshness;
}

/**
 * The room's state, as a union rather than nullable fields: "there is no artifact", "there is one but it
 * cannot be read", and "here it is" are mutually exclusive and each needs a different sentence.
 */
export type WorkflowState =
  | { kind: "empty"; section: WorkflowsSection | null }
  | { kind: "unreadable"; reason: string }
  | { kind: "ready"; workflow: LoadedWorkflow };

/**
 * Compare the artifact's recorded head digest with the digest of the graph in this bundle.
 *
 * `unknown` when either side is missing — never `current` by default, because implying the artifact
 * matches when that has not been established is the misreporting this whole section exists to prevent.
 */
export function freshnessOf(artifact: ReviewArtifact, manifest: Manifest | null): Freshness {
  const head = artifact.provenance?.graph?.head_digest;
  const graph = manifest?.graph?.digest;
  if (!head || !graph) return "unknown";
  return head === graph ? "current" : "different-revision";
}

export async function loadWorkflow(manifest: Manifest | null): Promise<WorkflowState> {
  const section = manifest?.workflows ?? null;

  let res: Response;
  try {
    res = await fetch(bundleUrl(REVIEW_WORKFLOW_PATH));
  } catch {
    // No bundle reachable at all. Report as unreadable rather than as "no artifact": the file may exist
    // and we failed to ask for it, and those are different facts.
    return { kind: "unreadable", reason: "the bundle could not be reached" };
  }
  if (res.status === 404) return { kind: "empty", section };
  if (!res.ok) return { kind: "unreadable", reason: `HTTP ${res.status} reading ${REVIEW_WORKFLOW_PATH}` };

  let artifact: ReviewArtifact;
  try {
    artifact = (await res.json()) as ReviewArtifact;
  } catch {
    return { kind: "unreadable", reason: `${REVIEW_WORKFLOW_PATH} is present but is not valid JSON` };
  }
  return { kind: "ready", workflow: { artifact, freshness: freshnessOf(artifact, manifest) } };
}

/** How a review symbol maps onto a Focus. `ambiguous` is a result, not a failure to hide. */
export type Identity =
  | { kind: "unique"; functionId: string }
  | { kind: "ambiguous"; count: number }
  | { kind: "absent" };

/**
 * Normalise a path for comparison.
 *
 * Two forms that name the same file must compare equal, or a resolvable symbol is reported as absent —
 * which is the failure this room must never produce silently:
 *
 *   - **`/private` prefixes.** On macOS `/tmp`, `/var` and `/etc` are symlinks under `/private`, so a
 *     command invoked with `/tmp/repo` and a graph indexed at `/private/tmp/repo` record the same file
 *     under different strings. Observed on a real fixture: the artifact said `/tmp/rev-head/…` and the
 *     index said `/private/tmp/rev-head/…`, and every affected row fell back to "not in this graph".
 *   - **Trailing slashes and separators.**
 */
function normalizePath(p: string): string {
  return p
    .replace(/\\/g, "/")
    .replace(/\/+$/, "")
    .replace(/^\/private\/(tmp|var|etc)\//, "/$1/");
}

/**
 * Whether two recorded paths name the same file.
 *
 * The suffix branch exists because one side may be relative while the other is absolute. It is a
 * *weaker* test than equality, so it is only reached after normalisation has had its chance.
 */
export function samePath(a: string | null | undefined, b: string | null | undefined): boolean {
  if (!a || !b) return false;
  const x = normalizePath(a);
  const y = normalizePath(b);
  return x === y || x.endsWith(`/${y}`) || y.endsWith(`/${x}`);
}

export function identityOf(symbol: ReviewSymbol, functions: FunctionEntry[]): Identity {
  const file = symbol.head?.file;
  if (!symbol.name || !file) return { kind: "absent" };
  const matches = functions.filter((f) => f.name === symbol.name && samePath(f.file_path, file));
  if (matches.length === 1) return { kind: "unique", functionId: matches[0].function_id };
  if (matches.length > 1) return { kind: "ambiguous", count: matches.length };
  return { kind: "absent" };
}

/** Sections the artifact says it did not produce, with the reason it recorded. */
export function absenceNotes(artifact: ReviewArtifact): { section: string; reason: string }[] {
  const unavailable = artifact.provenance?.unavailable ?? {};
  const notes: { section: string; reason: string }[] = [];
  const push = (section: string, produced: boolean, fallback: string) => {
    if (produced) return;
    notes.push({ section, reason: unavailable[section] ?? fallback });
  };
  push("policy", artifact.policy != null, "the artifact records that no policy was evaluated");
  push(
    "timeline",
    Array.isArray(artifact.timeline) && artifact.timeline.length > 0,
    "the artifact records an empty timeline",
  );
  return notes;
}

/** Which dimensions were cut, top-level or per symbol. Empty means nothing was truncated. */
export function truncationOf(flags: (TruncationFlags | undefined)[]): ("symbols" | "fanout" | "depth")[] {
  const cut = new Set<"symbols" | "fanout" | "depth">();
  for (const f of flags) {
    if (!f) continue;
    if (f.symbols) cut.add("symbols");
    if (f.fanout) cut.add("fanout");
    if (f.depth) cut.add("depth");
  }
  // Returned in a fixed order rather than discovery order: this feeds a sentence in the room, and a
  // notice that reshuffles itself depending on which level reported what reads as noise.
  const order = ["symbols", "fanout", "depth"] as const;
  return order.filter((dimension) => cut.has(dimension));
}

export function formatGeneratedAt(value: string | undefined): string | null {
  if (!value) return null;
  const secs = Number(value);
  if (!Number.isFinite(secs)) return null;
  return new Date(secs * 1000).toISOString().replace("T", " ").slice(0, 16);
}

/** The command that produces an artifact, shown when there is none — including the base-artifact need. */
export const PRODUCE_COMMAND =
  "rgctl review export --base-artifact <base> --base-ref <base-ref> --head-ref <head-ref>";
