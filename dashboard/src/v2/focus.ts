/**
 * The Focus — the one model that ties the surfaces together.
 *
 * A single portable identifier for "the thing you are currently looking at", carried across rooms and
 * lenses and serialized into the URL. Deliberately *not* a per-view selection: that is what makes rooms
 * feel disconnected, and it is the failure mode the maintainer parked issue #61 over.
 *
 * ## Why `function_id` is the identity
 *
 * It is the node's deterministic id: `UUIDv5(NAMESPACE_RGCTL, BLAKE3(path ‖ 0xff ‖ name ‖ 0xff ‖
 * node_type)[..8])`, assigned in `with_file_path` (`crates/rgctl-graph/src/schema.rs:444`) via
 * `deterministic_node_id`. So it *is* the graph's `StableNodeKey`, in UUID form, and it has exactly the
 * semantics a shareable Focus needs. Measured on a three-function repo:
 *
 *   | change                  | `function_id` | analysis `stable_key` |
 *   |-------------------------|---------------|-----------------------|
 *   | edit the function body  | unchanged     | changed               |
 *   | rename the function     | changed       | changed               |
 *   | re-index, no change     | unchanged     | unchanged             |
 *
 * The alternative was the analysis storage key, `file \x1f name \x1f code_hash`
 * (`crates/rgctl-analysis/src/storage.rs:24`), whose own doc comment calls it a *stable cache key*. It is
 * built for cache invalidation, so it **must** change whenever the content changes — and it did. A Focus
 * built on it would expire the moment anyone touched the function, which is exactly when a review link is
 * most wanted.
 *
 * An earlier version of this file namespaced the two (`sk:` / `fn:`) so their meanings could not be
 * confused while both were in play. That was unnecessary ceremony: they are the same identity expressed
 * twice, one of them worse, and the worse one is no longer exported.
 *
 * ## What it does not survive
 *
 * A **rename**. The name is part of the hash, so renaming a symbol produces a different id and an old link
 * stops resolving. Nothing in the engine survives a rename without a resolution step, and resolution needs
 * a baseline snapshot — which a pasted URL does not have. A Focus that no longer resolves therefore renders
 * an explicit stale state that says why, rather than a silently empty subject.
 */

export type Focus = string;

/**
 * A Focus is a node UUID.
 *
 * Validated rather than accepted blindly: a malformed URL should produce "no subject", not a subject that
 * looks real and resolves to nothing.
 */
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function keyOf(entry: { function_id: string }): Focus {
  return entry.function_id;
}

export function parseFocus(raw: unknown): Focus | null {
  return typeof raw === "string" && UUID.test(raw) ? raw : null;
}

/** Resolve a Focus against the loaded index. `null` means it no longer resolves — a real state. */
export function resolveFocus<T extends { function_id: string }>(key: Focus, entries: T[]): T | null {
  return entries.find((e) => e.function_id === key) ?? null;
}

export const ROOMS = [
  { id: "repo", label: "Repo", blurb: "where do I start?" },
  { id: "understand", label: "Understand", blurb: "what does this symbol do?" },
  { id: "review", label: "Review", blurb: "what changed, what does it touch?" },
  { id: "security", label: "Security", blurb: "what is reachable and dangerous?" },
  { id: "architecture", label: "Architecture", blurb: "how is it structured?" },
  { id: "migration", label: "Migration", blurb: "what order should we move it?" },
  { id: "proof", label: "Proof", blurb: "why should I believe any of this?" },
] as const;

export const LENSES = [
  { id: "spine", label: "Spine" },
  { id: "code", label: "Code" },
  { id: "cfgpddg", label: "CFG·PDG" },
  { id: "dataflow", label: "Dataflow" },
  { id: "slice", label: "Slice/Trace" },
  { id: "blast", label: "Blast" },
  { id: "metrics", label: "Metrics" },
  { id: "time", label: "Time" },
] as const;

export type RoomId = (typeof ROOMS)[number]["id"];
export type LensId = (typeof LENSES)[number]["id"];

export const isRoom = (v: unknown): v is RoomId => ROOMS.some((r) => r.id === v);
export const isLens = (v: unknown): v is LensId => LENSES.some((l) => l.id === v);
