/**
 * Package membership: both storage forms, and the absences that must not look like emptiness.
 *
 * The producer emits membership inline below `threshold_community_only` and out of line above it. A client
 * that only understands one form reports "no members" for the other — which is exactly the bug this module
 * exists to prevent, so each form is tested, and so is the bundle that carries neither.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { Metagraph } from "../architectureTypes";
import {
  decodeU32Le,
  loadMemberIndices,
  memberRange,
  resetMemberCache,
} from "../metagraphMembers";

function metagraph(overrides: Partial<Metagraph> = {}, nodes: Metagraph["nodes"] = []): Metagraph {
  return {
    schema_version: 3,
    mode: "community_only",
    community_only: true,
    threshold_community_only: 50000,
    source_node_count: 247791,
    nodes,
    edges: [],
    ...overrides,
  } as Metagraph;
}

const node = (id: number, extra: Record<string, unknown> = {}) =>
  ({
    id,
    label: `pkg${id}`,
    size: 10,
    functions: 8,
    classes: 2,
    avg_complexity: 0,
    x: 0,
    y: 0,
    community_id: 0,
    ...extra,
  }) as Metagraph["nodes"][number];

/** A sidecar of three packages: [0,1,2], [3,4], [] — the last has no run. */
function sidecar(): Uint8Array {
  const values = [0, 1, 2, 3, 4];
  const bytes = new Uint8Array(values.length * 4);
  const view = new DataView(bytes.buffer);
  values.forEach((v, i) => view.setUint32(i * 4, v, true));
  return bytes;
}

beforeEach(() => {
  resetMemberCache();
});

describe("inline membership", () => {
  it("is used directly, with no fetch", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);

    const state = await loadMemberIndices(
      metagraph({ members_path: undefined }, [node(0, { member_indices: [7, 9] })]),
      0,
    );

    expect(state).toEqual({ kind: "ready", indices: Uint32Array.from([7, 9]) });
    expect(fetchSpy).not.toHaveBeenCalled();
  });
});

describe("out-of-line membership", () => {
  it("slices the package's own run out of the sidecar", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, arrayBuffer: async () => sidecar().buffer })),
    );

    const meta = metagraph({ members_path: "metanode_members.bin", members_total: 5 }, [
      node(0, { member_offset: 0, member_count: 3 }),
      node(1, { member_offset: 12, member_count: 2 }),
      node(2, { member_offset: 20, member_count: 0 }),
    ]);

    await expect(loadMemberIndices(meta, 0)).resolves.toEqual({
      kind: "ready",
      indices: Uint32Array.from([0, 1, 2]),
    });
    await expect(loadMemberIndices(meta, 1)).resolves.toEqual({
      kind: "ready",
      indices: Uint32Array.from([3, 4]),
    });
  });

  it("reports a package with no run as an absence, not as empty", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, arrayBuffer: async () => sidecar().buffer })),
    );
    const meta = metagraph({ members_path: "metanode_members.bin" }, [node(2, { member_count: 0 })]);

    const state = await loadMemberIndices(meta, 2);
    expect(state.kind).toBe("absent");
    if (state.kind === "absent") {
      expect(state.reason).toMatch(/carries no membership/i);
    }
  });

  it("decodes little-endian regardless of host byte order", () => {
    const bytes = new Uint8Array([0x01, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00]);
    expect(Array.from(decodeU32Le(bytes))).toEqual([1, 256]);
  });
});

describe("a run that does not fit its sidecar", () => {
  it("is reported as a disagreement between artifacts, not as an empty package", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => ({ ok: true, arrayBuffer: async () => sidecar().buffer })),
    );
    const meta = metagraph({ members_path: "metanode_members.bin" }, [
      // offset 400 is far past a 20-byte sidecar.
      node(0, { member_offset: 400, member_count: 3 }),
    ]);

    const state = await loadMemberIndices(meta, 0);
    expect(state.kind).toBe("failed");
    if (state.kind === "failed") expect(state.reason).toMatch(/disagree/i);
  });

  it("is reported when the sidecar itself is missing", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => ({ ok: false, status: 404 })));
    const meta = metagraph({ members_path: "metanode_members.bin" }, [
      node(0, { member_offset: 0, member_count: 3 }),
    ]);

    const state = await loadMemberIndices(meta, 0);
    expect(state.kind).toBe("failed");
    if (state.kind === "failed") expect(state.reason).toMatch(/broken bundle/i);
  });
});

describe("a bundle with no membership at all", () => {
  it("names the limitation instead of drawing an empty district", async () => {
    const fetchSpy = vi.fn();
    vi.stubGlobal("fetch", fetchSpy);

    const state = await loadMemberIndices(metagraph({}, [node(0)]), 0);
    expect(state.kind).toBe("absent");
    if (state.kind === "absent") expect(state.reason).toMatch(/member_indices/);
    expect(fetchSpy).not.toHaveBeenCalled();
  });

  it("says when the id is not in this bundle at all", async () => {
    const state = await loadMemberIndices(metagraph({}, [node(0)]), 0);
    const missing = await loadMemberIndices(metagraph({}, [node(0)]), 99);
    expect(state.kind).toBe("absent");
    expect(missing.kind).toBe("absent");
    if (missing.kind === "absent") expect(missing.reason).toMatch(/no metanode 99/);
  });
});

describe("memberRange", () => {
  it("returns null for a package with no run", () => {
    expect(memberRange(node(0))).toBeNull();
    expect(memberRange(node(0, { member_offset: 0, member_count: 0 }))).toBeNull();
    expect(memberRange(node(0, { member_offset: 8, member_count: 2 }))).toEqual({
      offset: 8,
      count: 2,
    });
  });
});
