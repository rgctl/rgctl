/**
 * Focus identity claims that do not need a mounted shell.
 */
import { describe, expect, it } from "vitest";
import { keyOf, resolveFocus } from "../focus";
import { languageCoverageFromPaths } from "../data";
import { LANGUAGE_TIER } from "../languageTiers";

describe("3.5a Focus survives a re-index", () => {
  it("the same function_id resolves against a second index with different payloads", () => {
    const id = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    const before = [{ function_id: id, name: "handleAuth", file_path: "/repo/src/auth.ts", pdg_nodes: 4 }];
    // Re-export: body / metrics changed, identity did not — which is what `function_id` guarantees.
    const after = [{ function_id: id, name: "handleAuth", file_path: "/repo/src/auth.ts", pdg_nodes: 9 }];

    const key = keyOf(before[0]!);
    expect(key).toBe(id);
    expect(resolveFocus(key, after)?.pdg_nodes).toBe(9);
    expect(resolveFocus(key, after)?.name).toBe("handleAuth");
  });

  it("a rename produces a different key and the old Focus does not resolve", () => {
    const oldId = "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee";
    const newId = "11111111-2222-4333-8444-555555555555";
    const afterRename = [{ function_id: newId, name: "handleAuthentication" }];
    expect(resolveFocus(oldId, afterRename)).toBeNull();
  });
});

describe("2.3 language coverage from recorded tiers", () => {
  it("only languages with a stated tier are reported as covered", () => {
    const { languages, unstatedFiles } = languageCoverageFromPaths([
      "/repo/src/auth.ts",
      "/repo/src/main.rs",
      "/repo/vendor/weird.zzz",
      "/repo/README",
    ]);
    expect(languages.map((l) => l.id).sort()).toEqual(["rust", "typescript"]);
    expect(languages.every((l) => LANGUAGE_TIER[l.id] !== undefined)).toBe(true);
    expect(unstatedFiles).toBe(2);
  });

  it("does not invent coverage when the source index is empty", () => {
    expect(languageCoverageFromPaths([])).toEqual({ languages: [], unstatedFiles: 0 });
  });
});
