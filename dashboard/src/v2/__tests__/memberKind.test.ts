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

/** HSL saturation and lightness of a `#rrggbb`, for the "no greys, nothing dark" assertions. */
function hsl(hex: string): { s: number; l: number } {
  const n = Number.parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const l = (max + min) / 2;
  const d = max - min;
  const s = d === 0 ? 0 : d / (1 - Math.abs(2 * l - 1));
  return { s, l };
}
const saturation = (hex: string) => hsl(hex).s;
const lightness = (hex: string) => hsl(hex).l;

/** Shortest distance around the colour wheel, in degrees. */
function hueDistance(a: string, b: string): number {
  const ha = hueOf(a);
  const hb = hueOf(b);
  const raw = Math.abs(ha - hb);
  return Math.min(raw, 360 - raw);
}

function hueOf(hex: string): number {
  const n = Number.parseInt(hex.slice(1), 16);
  const r = ((n >> 16) & 255) / 255;
  const g = ((n >> 8) & 255) / 255;
  const b = (n & 255) / 255;
  const max = Math.max(r, g, b);
  const min = Math.min(r, g, b);
  const d = max - min;
  if (d === 0) return 0;
  const h = max === r ? ((g - b) / d) % 6 : max === g ? (b - r) / d + 2 : (r - g) / d + 4;
  return ((h * 60) % 360 + 360) % 360;
}

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
    expect(colourHexForKind("Function")).toBe(0x60a5fa);
    expect(colourForKind("File")).toBe("#c99a3d");
  });

  it("puts the type-like kinds in one family, so a level is not confetti", () => {
    const types = ["Class", "Struct", "Interface", "Enum", "TypeAlias", "Annotation"];
    const colours = new Set(types.map((k) => colourForKind(k)));
    expect(colours.size).toBe(1);
  });

  it("gives an unnamed kind a colour of its own rather than a grey", () => {
    // Not grey and not black: on a near-black canvas those are not "quiet", they are invisible — which is what the
    // file/import greys were, and they were the same grey as each other as well.
    expect(colourForKind("SomethingNew")).toBe(OTHER_COLOUR);
    expect(colourForKind(null)).toBe(OTHER_COLOUR);
    expect(saturation(OTHER_COLOUR)).toBeGreaterThan(0.3);
  });

  it("uses no grey for any kind, and never a dark one", () => {
    for (const kind of ["Function", "Class", "Struct", "Interface", "Enum", "TypeAlias", "Annotation", "Module",
      "File", "Import", "Dependency", "Variable", "ConfigKey", "Table", "Macro", "Job", "BuildStep", "Other"]) {
      const colour = colourForKind(kind);
      expect(saturation(colour), `${kind} ${colour} is grey`).toBeGreaterThan(0.3);
      expect(lightness(colour), `${kind} ${colour} is too dark`).toBeGreaterThan(0.45);
    }
  });

  it("tells the most confusable pairs apart", () => {
    // The first reported defect in one assertion: these were the same grey.
    expect(colourForKind("File")).not.toBe(colourForKind("Import"));
    // And the pair a reader meets most often after functions must not be two blues.
    expect(colourForKind("Function")).not.toBe(colourForKind("Module"));
    // The second report: `file` was a leaf green beside a teal `module`, and at nine pixels the two read as one
    // colour in two shades. Hue separation is the rule, not "they are different hexes".
    expect(hueDistance(colourForKind("File"), colourForKind("Module"))).toBeGreaterThan(30);
    expect(hueDistance(colourForKind("Module"), colourForKind("Import"))).toBeGreaterThan(60);
  });

  it("keeps every pair of named families at least 30° of hue apart", () => {
    const named = [
      "Function", "Class", "Module", "File", "Import", "Variable", "Macro",
    ].map((kind) => ({ kind, colour: colourForKind(kind) }));
    for (let i = 0; i < named.length; i += 1) {
      for (let j = i + 1; j < named.length; j += 1) {
        const apart = hueDistance(named[i].colour, named[j].colour);
        expect(apart, `${named[i].kind} vs ${named[j].kind}`).toBeGreaterThan(30);
      }
    }
  });

  it("keeps functions distinct from every family in the legend", () => {
    const legend = KIND_LEGEND.map((e) => e.colour);
    expect(new Set(legend).size).toBe(legend.length);
    expect(legend).toContain(colourForKind("function"));
  });
});
