/**
 * Package membership — which graph nodes belong to a district.
 *
 * The Architecture room can expand a solid only if something says what is inside it. The engine has always
 * been able to draw a member set (`expandIndices(indices, type_mask)` in `rgctl-wasm`), but above 50 000
 * nodes the bundle used to drop the membership that makes the set, so the ability went unused: the room had a
 * gesture it could not honour and a district that could not be opened.
 *
 * Membership arrives in one of two forms, and this module is the only place that knows it:
 *
 *   * **inline** — `member_indices` on the metanode, for graphs below `threshold_community_only`;
 *   * **out of line** — `metanode_members.bin`, a little-endian `u32` stream, addressed per package by
 *     `member_offset` / `member_count` (the producer's `metagraph.rs`).
 *
 * Both forms are the same accounting. A bundle with neither is a **typed absence**, not an empty package:
 * "loaded and empty" and "the export did not carry membership" are different claims, and the room says
 * which one it means.
 */
import { bundleUrl } from "./data";
import type { Metagraph, MetaNode } from "./architectureTypes";

export type MembersState =
  | { kind: "loading" }
  | { kind: "ready"; indices: Uint32Array }
  /** The bundle does not carry membership for this package, and why. */
  | { kind: "absent"; reason: string }
  /** The read itself failed — a different claim from "absent". */
  | { kind: "failed"; reason: string };

/** The sidecar is loaded once per session and sliced per package; it is ~1 MB for 250 k members. */
const sidecars = new Map<string, Promise<Uint8Array | null>>();

function loadSidecar(path: string): Promise<Uint8Array | null> {
  const cached = sidecars.get(path);
  if (cached) return cached;
  const pending = (async () => {
    try {
      const res = await fetch(bundleUrl(path));
      if (!res.ok) return null;
      return new Uint8Array(await res.arrayBuffer());
    } catch {
      return null;
    }
  })();
  sidecars.set(path, pending);
  return pending;
}

/** Where a package's run sits in the sidecar, or `null` when it has no out-of-line run. */
export function memberRange(
  node: MetaNode,
): { offset: number; count: number } | null {
  if (typeof node.member_offset !== "number" || typeof node.member_count !== "number") return null;
  if (node.member_count <= 0) return null;
  return { offset: node.member_offset, count: node.member_count };
}

/**
 * Disjoint-union membership: the indices this package owns, in either form.
 *
 * `null` means "no membership was exported", which the caller must render as an absence with a reason —
 * never as an empty district.
 */
export async function loadMemberIndices(
  metagraph: Metagraph,
  packageId: number,
  options: { signal?: AbortSignal } = {},
): Promise<MembersState> {
  const node = metagraph.nodes.find((n) => n.id === packageId);
  if (!node) {
    return {
      kind: "absent",
      reason: `This bundle has no metanode ${packageId}. The id is stable within one export, not across a re-index.`,
    };
  }

  if (Array.isArray(node.member_indices) && node.member_indices.length > 0) {
    return { kind: "ready", indices: Uint32Array.from(node.member_indices) };
  }

  const range = memberRange(node);
  if (range === null) {
    return {
      kind: "absent",
      reason:
        "This bundle carries no membership for this package — the export wrote neither `member_indices` nor a run in `metanode_members.bin`. Re-index with a build that exports membership to open the district.",
    };
  }

  const path = metagraph.members_path;
  if (!path) {
    return {
      kind: "absent",
      reason:
        "The metanode addresses its members by offset, but the metagraph declares no `members_path`, so the sidecar's location is unknown.",
    };
  }

  if (options.signal?.aborted) {
    return { kind: "failed", reason: "The read was cancelled." };
  }

  const bytes = await loadSidecar(path);
  if (bytes === null) {
    return {
      kind: "failed",
      reason: `\`${path}\` could not be read, so the members of this package are unknown. That is a broken bundle, not an empty package.`,
    };
  }

  const start = range.offset;
  const end = start + range.count * 4;
  if (start % 4 !== 0 || end > bytes.byteLength) {
    return {
      kind: "failed",
      reason: `The run for this package (offset ${start}, ${range.count} indices) falls outside \`${path}\` (${bytes.byteLength} bytes). The metagraph and the sidecar disagree.`,
    };
  }

  // `buffer.slice` rather than a `Uint32Array` view: the stream is little-endian by declaration, and a view
  // would silently reinterpret it on a big-endian host.
  const copy = bytes.slice(start, end);
  return { kind: "ready", indices: decodeU32Le(copy) };
}

/** Decode a little-endian `u32` stream. Exported for the test that proves the byte order. */
export function decodeU32Le(bytes: Uint8Array): Uint32Array {
  const count = Math.floor(bytes.byteLength / 4);
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const out = new Uint32Array(count);
  for (let i = 0; i < count; i += 1) out[i] = view.getUint32(i * 4, true);
  return out;
}

/** Test seam: forget the cached sidecars. */
export function resetMemberCache(): void {
  sidecars.clear();
}
