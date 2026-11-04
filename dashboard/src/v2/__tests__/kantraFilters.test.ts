import { describe, expect, it } from "vitest";
import {
  ALL_KANTRA_CATEGORIES,
  fileMatchesFilters,
  normalizeCategory,
  violationMatchesFilters,
  type KantraFileEntry,
  type KantraRuleEntry,
  type KantraViolation,
} from "../data/loadKantra";

const file: KantraFileEntry = {
  id: "a",
  path: "Demo.java",
  count: 2,
  categories: { optional: 2, mandatory: 0 },
  max_blast: 0,
  target_categories: { quarkus: { optional: 2 } },
};

const rule: KantraRuleEntry = {
  rule_id: "r1",
  count: 1,
  category: "optional",
  targets: ["quarkus"],
};

const violation: KantraViolation = {
  rule_id: "r1",
  category: "optional",
  file: "Demo.java",
  line: 1,
  matched_by: "java.referenced",
  targets: ["quarkus"],
};

describe("kantra filter helpers", () => {
  it("normalizes unknown categories to uncategorized", () => {
    expect(normalizeCategory("weird")).toBe("uncategorized");
    expect(normalizeCategory("mandatory")).toBe("mandatory");
  });

  it("matches files by category and target", () => {
    expect(fileMatchesFilters(file, ALL_KANTRA_CATEGORIES, null)).toBe(true);
    expect(fileMatchesFilters(file, new Set(["mandatory"]), null)).toBe(false);
    expect(fileMatchesFilters(file, new Set(["optional"]), "quarkus")).toBe(true);
    expect(fileMatchesFilters(file, new Set(["optional"]), "eap8")).toBe(false);
  });

  it("matches violations by category and target", () => {
    expect(
      violationMatchesFilters(violation, rule, new Set(["optional"]), "quarkus"),
    ).toBe(true);
    expect(
      violationMatchesFilters(violation, rule, new Set(["mandatory"]), "quarkus"),
    ).toBe(false);
    expect(
      violationMatchesFilters(violation, rule, new Set(["optional"]), "eap8"),
    ).toBe(false);
  });
});
