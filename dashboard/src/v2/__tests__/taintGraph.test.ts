import { describe, expect, it } from "vitest";
import {
  blastForFunction,
  classifyTaintIndex,
  parseTaintIndex,
} from "../data/loadTaint";
import { buildTaintPathModel, pathPositions } from "../taintPathLayout";
import taintIndex from "./fixtures/taint/taint_index.json";
import taintFlow from "./fixtures/taint/taint/8fcef6c3-dfc0-5338-ba7b-e14cc4816997.json";

describe("classifyTaintIndex", () => {
  it("treats invalid as missing", () => {
    expect(classifyTaintIndex(null).kind).toBe("missing");
    expect(classifyTaintIndex({}).kind).toBe("missing");
  });

  it("distinguishes unavailable from empty from ready", () => {
    expect(
      classifyTaintIndex({
        schema_version: 1,
        available: false,
        detail_dir: "taint",
        function_count: 0,
        total_flows: 0,
        vulnerable_flows: 0,
        functions: [],
      }).kind,
    ).toBe("unavailable");

    expect(
      classifyTaintIndex({
        schema_version: 1,
        available: true,
        detail_dir: "taint",
        function_count: 0,
        total_flows: 0,
        vulnerable_flows: 0,
        functions: [],
      }).kind,
    ).toBe("empty");

    expect(classifyTaintIndex(taintIndex).kind).toBe("ready");
  });

  it("accepts a real exported taint_index.json", () => {
    const parsed = parseTaintIndex(taintIndex);
    expect(parsed?.available).toBe(true);
    expect(parsed!.functions.length).toBeGreaterThan(0);
  });
});

describe("buildTaintPathModel", () => {
  it("orders path steps from a real flow export", () => {
    const flow = taintFlow.flows[0]!;
    const model = buildTaintPathModel(flow);
    expect(model.steps.length).toBeGreaterThanOrEqual(2);
    expect(model.steps[0]!.kind).toBe("source");
    expect(model.steps[model.steps.length - 1]!.kind).toBe("sink");
    expect(model.vulnerable).toBe(true);
    const pos = pathPositions(model, 720, 220);
    expect(pos).toHaveLength(model.steps.length);
    expect(pos[0]!.x).toBeLessThan(pos[pos.length - 1]!.x);
  });
});

describe("blastForFunction", () => {
  it("states absence when no columnar join exists", () => {
    const r = blastForFunction({ schema_version: 2, available: true, functions: [] }, "abc", null);
    expect(r.kind).toBe("absent");
    if (r.kind === "absent") expect(r.reason).toMatch(/columnar|join|fabricated/i);
  });

  it("returns score when index matches", () => {
    const r = blastForFunction(
      {
        schema_version: 2,
        available: true,
        functions: [{ index: 3, score: 0.42, direct: 2, zone: 5 }],
      },
      "abc",
      3,
    );
    expect(r.kind).toBe("ready");
    if (r.kind === "ready") expect(r.score.score).toBe(0.42);
  });
});
