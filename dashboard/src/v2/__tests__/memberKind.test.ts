/**
 * A level's members are every node in the package, and only some of them are functions.
 *
 * Reported from use: a district showed five rows of absolute paths, and every one of them refused to open with "the
 * CFG index does not reach it". Three facts were tangled there — the level draws files as well as functions, a
 * file's *name is its path*, and sorting by name put those paths first — and each one is pinned here.
 */
import { describe, expect, it } from "vitest";
import { compareMembers, isFunctionKind, nonFunctionReason, shortKind } from "../rooms/memberKind";
import { KIND_LEGEND, OTHER_COLOUR, colourForKind, colourHexForKind } from "../universe/memberColours";

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

describe("shortKind", () => {
  it("uses the short word where there is one", () => {
    expect(shortKind("Function")).toBe("func");
    expect(shortKind("Variable")).toBe("var");
    expect(shortKind("TypeAlias")).toBe("type");
  });

  it("keeps the words that fit whole", () => {
    for (const kind of ["module", "struct", "import", "config"]) {
      expect(shortKind(kind), kind).toBe(kind);
    }
  });

  it("never returns a fragment of a longer word", () => {
    // The reported defect: `FUNCTION` rendered as `FUNCTI…`, three quarters of a word and no `func`.
    expect(shortKind("Dependency")).toBe("dep");
    expect(shortKind("PuppetFact")).toBe("fact");
    // Anything unmapped is cut at six with an ellipsis, which is at least honest about being cut.
    expect(shortKind("SomethingLong")).toBe("someth…");
    expect(shortKind("")).toBe("node");
  });
});

describe("colourForKind", () => {
  it("is stable and shared: the list and the level agree", () => {
    expect(colourForKind("Function")).toBe(colourForKind("function"));
    // The same call the scene makes for the instance colour.
    expect(colourHexForKind("Function")).toBe(0x8fd0ff);
    expect(colourForKind("File")).toBe("#94a3b8");
  });

  it("puts the type-like kinds in one family, so a level is not confetti", () => {
    const types = ["Class", "Struct", "Interface", "Enum", "TypeAlias", "Annotation"];
    const colours = new Set(types.map((k) => colourForKind(k)));
    expect(colours.size).toBe(1);
  });

  it("gives an unnamed kind a dull colour rather than inventing a bright one", () => {
    expect(colourForKind("SomethingNew")).toBe(OTHER_COLOUR);
    expect(colourForKind(null)).toBe(OTHER_COLOUR);
  });

  it("keeps functions distinct from every family in the legend", () => {
    const legend = KIND_LEGEND.map((e) => e.colour);
    expect(new Set(legend).size).toBe(legend.length);
    expect(legend).toContain(colourForKind("function"));
  });
});
