/**
 * Tests for the review artifact adapter.
 *
 * Each of these pins something that actually went wrong or that the spec requires, rather than
 * restating the implementation:
 *
 *   - the `/private` path form, which made every symbol report "not in this graph" on a real export
 *   - freshness, which must say *unknown* rather than imply a match
 *   - identity, which must report ambiguity rather than choose a symbol
 *   - absence, which must carry the artifact's own reason instead of a zero
 */
import { describe, expect, it } from "vitest";
import type { FunctionEntry, Manifest } from "../data";
import {
  absenceNotes,
  formatGeneratedAt,
  freshnessOf,
  identityOf,
  samePath,
  truncationOf,
  type ReviewArtifact,
} from "../workflow";

const fn = (id: string, name: string, file: string): FunctionEntry => ({
  function_id: id,
  name,
  file_path: file,
});

describe("samePath — one file recorded two ways", () => {
  it("treats a /private-prefixed path as the same file", () => {
    // Regression: on macOS an export recorded `/tmp/rev3/...` while the graph indexed by the same
    // command recorded `/private/tmp/rev3/...`. `/tmp` is a symlink, so these are one file — comparing
    // them literally made every row fall back to "not in this graph" and hid the Focus handoff.
    expect(samePath("/tmp/rev3/src/a.java", "/private/tmp/rev3/src/a.java")).toBe(true);
    expect(samePath("/private/var/tmp/x/b.ts", "/var/tmp/x/b.ts")).toBe(true);
  });

  it("does not equate different files of the same name", () => {
    expect(samePath("/repo/one/A.java", "/repo/two/A.java")).toBe(false);
  });

  it("matches when one side is relative to the other", () => {
    expect(samePath("src/main/A.java", "/private/tmp/rev3/src/main/A.java")).toBe(true);
  });

  it("is false when either side is missing", () => {
    expect(samePath(null, "/repo/A.java")).toBe(false);
    expect(samePath("/repo/A.java", undefined)).toBe(false);
  });
});

describe("freshnessOf — the artifact against the browsed graph", () => {
  const manifest = (digest?: string): Manifest => ({ graph: digest ? { digest } : {} });
  const artifact = (head?: string): ReviewArtifact => ({
    provenance: { graph: head ? { head_digest: head } : {} },
  });

  it("is current when the digests agree", () => {
    expect(freshnessOf(artifact("abc"), manifest("abc"))).toBe("current");
  });

  it("reports a different revision rather than showing foreign numbers as current", () => {
    expect(freshnessOf(artifact("abc"), manifest("def"))).toBe("different-revision");
  });

  it("is unknown — never current — when a side is missing", () => {
    expect(freshnessOf(artifact(undefined), manifest("abc"))).toBe("unknown");
    expect(freshnessOf(artifact("abc"), manifest(undefined))).toBe("unknown");
    expect(freshnessOf(artifact("abc"), null)).toBe("unknown");
  });
});

describe("identityOf — resolve, never guess", () => {
  const functions = [
    fn("id-1", "getProduct", "/private/tmp/rev3/src/A.java"),
    fn("id-2", "getProduct", "/private/tmp/rev3/src/A.java"),
    fn("id-3", "unique", "/private/tmp/rev3/src/A.java"),
  ];

  it("resolves a unique match, including across the /private form", () => {
    const identity = identityOf({ name: "unique", head: { file: "/tmp/rev3/src/A.java" } }, functions);
    expect(identity).toEqual({ kind: "unique", functionId: "id-3" });
  });

  it("reports ambiguity instead of choosing one of several overloads", () => {
    // Java overloads share a name and a file. Picking one would silently carry the wrong symbol.
    const identity = identityOf({ name: "getProduct", head: { file: "/tmp/rev3/src/A.java" } }, functions);
    expect(identity).toEqual({ kind: "ambiguous", count: 2 });
  });

  it("reports absence when the symbol is not in this graph", () => {
    const identity = identityOf({ name: "gone", head: { file: "/tmp/rev3/src/A.java" } }, functions);
    expect(identity).toEqual({ kind: "absent" });
  });

  it("reports absence rather than matching on name alone", () => {
    // Same name, different file: matching it would be a guess about which symbol the review meant.
    const identity = identityOf({ name: "unique", head: { file: "/tmp/rev3/src/B.java" } }, functions);
    expect(identity).toEqual({ kind: "absent" });
  });
});

describe("absenceNotes — an absence carries its reason", () => {
  it("uses the artifact's recorded reason for an unevaluated policy", () => {
    const notes = absenceNotes({
      policy: null,
      timeline: [],
      provenance: {
        unavailable: {
          policy: "no --policy-file provided; policy was not evaluated",
          timeline: "no violation_ledger.jsonl under .rgctl/",
        },
      },
    });
    expect(notes).toEqual([
      { section: "policy", reason: "no --policy-file provided; policy was not evaluated" },
      { section: "timeline", reason: "no violation_ledger.jsonl under .rgctl/" },
    ]);
  });

  it("says nothing about a section that was produced", () => {
    const notes = absenceNotes({ policy: { passed: true }, timeline: [{ at: 1 }], provenance: {} });
    expect(notes).toEqual([]);
  });

  it("still reports absence when the artifact records no reason", () => {
    // Never silently drop the section: an unstated reason is still not a pass.
    const notes = absenceNotes({ policy: null, timeline: [], provenance: {} });
    expect(notes.map((n) => n.section)).toEqual(["policy", "timeline"]);
    expect(notes[0].reason).toMatch(/no policy was evaluated/);
  });
});

describe("truncationOf — a cut report must be visible", () => {
  it("collects the cut dimensions across every level", () => {
    expect(truncationOf([{ fanout: true }, undefined, { symbols: true, depth: true }])).toEqual([
      "symbols",
      "fanout",
      "depth",
    ]);
  });

  it("is empty when nothing was cut", () => {
    expect(truncationOf([{}, undefined, { symbols: false }])).toEqual([]);
  });
});

describe("formatGeneratedAt", () => {
  it("renders the artifact's unix seconds", () => {
    expect(formatGeneratedAt("1792086432")).toBe("2026-10-15 17:47");
  });

  it("is null rather than a fabricated date", () => {
    expect(formatGeneratedAt(undefined)).toBeNull();
    expect(formatGeneratedAt("not-a-number")).toBeNull();
  });
});
