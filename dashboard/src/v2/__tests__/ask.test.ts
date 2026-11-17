import { describe, expect, it } from "vitest";
import { answerFor, shortPath, type SemanticQueryResponse } from "../ask";

const base: SemanticQueryResponse = {
  schema_version: 3,
  query: "checkout pricing",
  model_id: "test-vocab",
  dimensions: 256,
  hits: [
    {
      node_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
      name: "priceShoppingCart",
      qualified_name: "cart.priceShoppingCart",
      file_path: "/repo/src/CartService.java",
      distance: 12,
      score: 0.91,
      fused_score: 0.91,
      ranking: "fusion",
    },
    {
      node_id: "11111111-2222-4333-8444-555555555555",
      name: "CartController",
      qualified_name: "cart.CartController",
      file_path: "/repo/src/CartController.java",
      distance: 20,
      score: 0.72,
    },
  ],
};

describe("answerFor", () => {
  it("states the top hit and its blast evidence without inventing anything", () => {
    const answer = answerFor({
      ...base,
      expansion: {
        blast: [
          {
            anchor_node_id: base.hits[0].node_id,
            anchor_name: "priceShoppingCart",
            direct_callers: 12,
            impact_zone: 14,
            score: 14.2,
          },
        ],
      },
    });
    expect(answer.headline).toBe("priceShoppingCart is the strongest match");
    expect(answer.summary).toContain("12 direct callers");
    expect(answer.summary).toContain("impact zone of 14");
    expect(answer.summary).toContain("CartController");
  });

  it("says blast evidence is absent rather than printing zero", () => {
    const answer = answerFor(base);
    expect(answer.summary).toContain("was not included");
    expect(answer.summary).not.toContain("0 direct callers");
  });

  it("treats no hits as no match, not as a clean result", () => {
    const answer = answerFor({ ...base, hits: [] });
    expect(answer.headline).toBe("No function matched");
    expect(answer.summary).toContain("no hits");
    expect(answer.top).toBeNull();
  });

  it("shortens long paths to the trailing segments", () => {
    expect(shortPath("/repo/backend/src/main/java/CartService.java")).toBe(
      "main/java/CartService.java",
    );
    expect(shortPath(undefined)).toBe("—");
  });
});
