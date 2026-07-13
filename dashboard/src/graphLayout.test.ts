import { describe, expect, it } from "vitest";
import { hasPrecomputedLayout } from "./graphLayout";

describe("hasPrecomputedLayout", () => {
  it("detects export-time coordinates", () => {
    expect(hasPrecomputedLayout([{ x: 10, y: 0 }, { x: -10, y: 0 }])).toBe(true);
  });

  it("rejects placeholder zeros", () => {
    expect(hasPrecomputedLayout([{ x: 0, y: 0 }, { x: 0, y: 0 }])).toBe(false);
  });
});
