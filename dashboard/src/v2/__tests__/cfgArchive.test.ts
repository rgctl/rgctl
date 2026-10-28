/**
 * Archive-only CFG load: WASM decode of a record pack slice.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { encodeRecordIndex } from "./cfgRecordPackFixture";
import {
  isArchiveOnly,
  loadCfgDetailFromArchive,
  loadCfgIndexMeta,
  resetCfgArchiveCaches,
} from "../cfgArchive";

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
