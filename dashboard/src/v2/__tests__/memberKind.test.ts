/**
 * A level's members are every node in the package, and only some of them are functions.
 *
 * Reported from use: a district showed five rows of absolute paths, and every one of them refused to open with "the
 * CFG index does not reach it". Three facts were tangled there — the level draws files as well as functions, a
 * file's *name is its path*, and sorting by name put those paths first — and each one is pinned here.
 */
import { describe, expect, it } from "vitest";
import { compareMembers, isFunctionKind, nonFunctionReason } from "../rooms/memberKind";

describe("isFunctionKind", () => {
  it("recognises the engine's own spelling, whatever its case", () => {
    expect(isFunctionKind("Function")).toBe(true);
    expect(isFunctionKind(" function ")).toBe(true);
  });

  it("does not mistake anything else for one", () => {
    for (const kind of ["File", "Class", "Module", "Variable", "Import", "ConfigKey", "Struct", ""]) {
      expect(isFunctionKind(kind), kind).toBe(false);
    }
    expect(isFunctionKind(null)).toBe(false);
    expect(isFunctionKind(undefined)).toBe(false);
  });
});

describe("nonFunctionReason", () => {
  it("says what the member is, instead of blaming the index", () => {
    const reason = nonFunctionReason("File", "/repo/tests/analysis_perf.rs");
    expect(reason).toContain("is a File node, not a function");
    expect(reason).toContain("no function body to open");
    // The old wording sent the reader looking for a missing artifact.
    expect(reason).not.toContain("CFG index does not reach it");
  });

  it("has nothing to say about a function — the index is worth asking", () => {
    expect(nonFunctionReason("Function", "handleAuth")).toBeNull();
  });

  it("names an unknown kind rather than calling it empty", () => {
    expect(nonFunctionReason("", "x")).toContain("is a node node");
  });
});

describe("compareMembers", () => {
  it("puts functions before files, which is the whole point", () => {
    const rows = [
      { name: "/repo/tests/analysis_perf.rs", kind: "File" },
      { name: "run_full_analysis", kind: "Function" },
      { name: "/repo/tests/analysis_e2e.rs", kind: "File" },
      { name: "bucket_to_metanode", kind: "Function" },
    ];
    expect([...rows].sort(compareMembers).map((r) => r.name)).toEqual([
      "bucket_to_metanode",
      "run_full_analysis",
      "/repo/tests/analysis_e2e.rs",
      "/repo/tests/analysis_perf.rs",
    ]);
  });

  it("still orders within each kind by name", () => {
    const fns = [
      { name: "beta", kind: "Function" },
      { name: "alpha", kind: "Function" },
    ];
    expect([...fns].sort(compareMembers).map((r) => r.name)).toEqual(["alpha", "beta"]);
  });
});
