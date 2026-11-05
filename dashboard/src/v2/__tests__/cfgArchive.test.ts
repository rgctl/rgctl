/**
 * Archive-only CFG load: WASM decode of a record pack slice.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { encodeRecordIndex } from "./cfgRecordPackFixture";
import {
  isArchiveOnly,
  isSliceArchiveOnly,
  loadCfgDetailFromArchive,
  loadCfgIndexMeta,
  loadSliceIndexMeta,
  resetCfgArchiveCaches,
} from "../cfgArchive";
import { loadDfgDetail } from "../hooks/useDfgDetail";

const FN = "11111111-2222-4333-8444-555555555555";

const detailJson = JSON.stringify({
  schema_version: 2,
  function_id: FN,
  name: "handle",
  file_path: "app.py",
  entry: 0,
  exits: [1],
  blocks: [
    { id: 0, label: "B0", start_line: 1, end_line: 1, statements: ["a"] },
    { id: 1, label: "B1", start_line: 2, end_line: 2, statements: ["b"] },
  ],
  edges: [{ from: 0, to: 1, edge_type: "next" }],
  idom: [0, 0],
  dominance_frontiers: [[], []],
});

vi.mock("../../../wasm/rgctl_wasm.js", () => ({
  default: vi.fn(async () => undefined),
  parseCfgDetail: vi.fn((bytes: Uint8Array) => {
    // Fixture stores UTF-8 JSON as the "record" body.
    return new TextDecoder().decode(bytes);
  }),
}));

afterEach(() => {
  resetCfgArchiveCaches();
  vi.unstubAllGlobals();
});

describe("isArchiveOnly", () => {
  it("requires mode and both pack paths", () => {
    expect(isArchiveOnly(null)).toBe(false);
    expect(
      isArchiveOnly({
        detail_mode: "archive_only",
        record_index_path: "cfg_pdg.record_index.bin",
        record_data_path: "cfg_pdg.record_data.bin",
      }),
    ).toBe(true);
    expect(
      isArchiveOnly({
        detail_mode: "per_file",
        record_index_path: "cfg_pdg.record_index.bin",
        record_data_path: "cfg_pdg.record_data.bin",
      }),
    ).toBe(false);
  });
});

describe("loadCfgDetailFromArchive", () => {
  it("decodes a function record via WASM parseCfgDetail", async () => {
    const body = new TextEncoder().encode(detailJson);
    const indexBytes = encodeRecordIndex([{ id: FN, offset: 0, length: body.length }]);

    vi.stubGlobal("fetch", async (input: RequestInfo | URL, init?: RequestInit) => {
      const url = String(input);
      if (url.includes("cfg_index.json")) {
        return new Response(
          JSON.stringify({
            schema_version: 2,
            available: true,
            detail_mode: "archive_only",
            record_index_path: "cfg_pdg.record_index.bin",
            record_data_path: "cfg_pdg.record_data.bin",
            function_count: 1,
            functions: [],
          }),
          { status: 200 },
        );
      }
      if (url.includes("cfg_pdg.record_index.bin")) {
        return new Response(indexBytes, { status: 200 });
      }
      if (url.includes("cfg_pdg.record_data.bin")) {
        const range = init?.headers
          ? (init.headers as Record<string, string>).Range ??
            (init.headers as Headers).get?.("Range")
          : undefined;
        // Range or full-file both ok — return the whole body; fetchCfgRecordBytes slices if needed.
        if (range) {
          return new Response(body, { status: 206 });
        }
        return new Response(body, { status: 200 });
      }
      return new Response("", { status: 404 });
    });

    const meta = await loadCfgIndexMeta();
    expect(isArchiveOnly(meta)).toBe(true);
    const cfg = await loadCfgDetailFromArchive(FN, meta);
    expect(cfg).not.toBeNull();
    expect(cfg!.name).toBe("handle");
    expect(cfg!.blocks).toHaveLength(2);
    expect(cfg!.edges?.[0]?.edge_type).toBe("next");
  });

  it("returns null when the function is missing from the pack", async () => {
    const indexBytes = encodeRecordIndex([]);
    vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
      const url = String(input);
      if (url.includes("cfg_pdg.record_index.bin")) {
        return new Response(indexBytes, { status: 200 });
      }
      return new Response("", { status: 404 });
    });
    const cfg = await loadCfgDetailFromArchive(FN, {
      detail_mode: "archive_only",
      record_index_path: "cfg_pdg.record_index.bin",
      record_data_path: "cfg_pdg.record_data.bin",
    });
    expect(cfg).toBeNull();
  });
});

const sliceDetailJson = JSON.stringify({
  schema_version: 2,
  function_id: FN,
  name: "handle",
  pdg: {
    nodes: [{ id: "n1", line: 10, label: "x = req.param", defined: ["x"], used: ["req"] }],
    edges: [],
  },
});

describe("slice archive & loadDfgDetail", () => {
  it("loads DFG from slice.record_*.bin without fetching slice/<id>.json when manifest.capabilities.slice_detail_archive is true", async () => {
    const body = new TextEncoder().encode(sliceDetailJson);
    const indexBytes = encodeRecordIndex([{ id: FN, offset: 0, length: body.length }]);
    const requestedUrls: string[] = [];

    const customFetch: typeof fetch = async (input: RequestInfo | URL) => {
      const url = String(input);
      requestedUrls.push(url);
      if (url.includes("slice.record_index.bin")) {
        return new Response(indexBytes, { status: 200 });
      }
      if (url.includes("slice.record_data.bin")) {
        return new Response(body, { status: 206 });
      }
      return new Response("", { status: 404 });
    };

    const dfg = await loadDfgDetail(FN, "slice", customFetch, {
      capabilities: { slice_detail_archive: true },
      slice_record_index_path: "slice.record_index.bin",
      slice_record_data_path: "slice.record_data.bin",
    });
    expect(dfg).not.toBeNull();
    expect(dfg!.name).toBe("handle");
    expect(dfg!.nodes).toHaveLength(1);
    expect(requestedUrls.some((u) => u.includes(`slice/${FN}.json`))).toBe(false);
  });

  it("checks cached slice_index.json metadata before per-file fetch when manifest is omitted in archive_only mode", async () => {
    const body = new TextEncoder().encode(sliceDetailJson);
    const indexBytes = encodeRecordIndex([{ id: FN, offset: 0, length: body.length }]);
    const requestedUrls: string[] = [];

    const customFetch: typeof fetch = async (input: RequestInfo | URL) => {
      const url = String(input);
      requestedUrls.push(url);
      if (url.includes("slice_index.json")) {
        return new Response(
          JSON.stringify({
            schema_version: 2,
            available: true,
            detail_mode: "archive_only",
            record_index_path: "slice.record_index.bin",
            record_data_path: "slice.record_data.bin",
            function_count: 6000,
          }),
          { status: 200 },
        );
      }
      if (url.includes("slice.record_index.bin")) {
        return new Response(indexBytes, { status: 200 });
      }
      if (url.includes("slice.record_data.bin")) {
        return new Response(body, { status: 206 });
      }
      return new Response("", { status: 404 });
    };

    // First load reads slice_index.json once and goes straight to archive (no 404 on slice/<id>.json).
    const dfg1 = await loadDfgDetail(FN, "slice", customFetch, null);
    expect(dfg1).not.toBeNull();
    expect(requestedUrls.some((u) => u.includes(`slice/${FN}.json`))).toBe(false);
    expect(requestedUrls.filter((u) => u.includes("slice_index.json"))).toHaveLength(1);

    // Second load reuses cached sliceIndexMeta and sliceRecordIndex (only fetches record_data.bin Range).
    const dfg2 = await loadDfgDetail(FN, "slice", customFetch, null);
    expect(dfg2).not.toBeNull();
    expect(requestedUrls.filter((u) => u.includes("slice_index.json"))).toHaveLength(1);
    expect(requestedUrls.filter((u) => u.includes("slice.record_index.bin"))).toHaveLength(1);

    const meta = await loadSliceIndexMeta(customFetch);
    expect(isSliceArchiveOnly(meta)).toBe(true);
  });
});
