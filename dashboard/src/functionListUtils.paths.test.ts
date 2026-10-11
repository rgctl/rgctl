import { describe, expect, it } from "vitest";
import { shortPath } from "./functionListUtils";

describe("shortPath", () => {
  it("returns paths that already fit verbatim, without a leading ellipsis", () => {
    expect(shortPath("src/App.tsx")).toBe("src/App.tsx");
    expect(shortPath("App.tsx")).toBe("App.tsx");
    expect(shortPath("src/routes/orders.ts")).toBe("src/routes/orders.ts");
  });

  it("keeps three trailing segments when eliding", () => {
    expect(shortPath("src/pages/ffm/AgentFilter.tsx")).toBe("…/pages/ffm/AgentFilter.tsx");
    expect(shortPath("a/b/c/d/e.ts")).toBe("…/c/d/e.ts");
  });

  it("does not collapse sibling files that differ above the last two segments", () => {
    // Regression: with only 2 segments kept, both of these rendered as
    // "…/routes/orders.ts" and looked like duplicate sidebar entries.
    const a = shortPath("src/routes/orders.ts");
    const b = shortPath("src/coolstore/routes/orders.ts");
    expect(a).not.toBe(b);
  });

  it("handles Windows separators", () => {
    expect(shortPath("src\\pages\\ffm\\AgentFilter.tsx")).toBe("…/pages/ffm/AgentFilter.tsx");
  });
});
