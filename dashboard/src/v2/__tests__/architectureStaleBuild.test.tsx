/**
 * The failure a rebuild leaves behind in an already-open page.
 *
 * `rgctl serve` embeds the dashboard in the binary, so rebuilding the binary replaces every content-hashed
 * chunk. A page that was open keeps its old entry script, and the room's lazy `import()` then asks for a file
 * that no longer exists. The bytes are gone, so the import can never succeed — the room's only useful move is to
 * name the cause and offer the reload.
 *
 * The mock reproduces the **message**, not the mechanism: a `vi.mock` factory that throws is repackaged by
 * vitest into its own warning, which would test vitest rather than the room. So the stub scene refuses to
 * construct with the same `TypeError` a browser raises, and it lands in the same catch block the import would.
 */
import { render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import metagraphJson from "./fixtures/architecture/metagraph.json";
import summaryJson from "./fixtures/architecture/communities.json";

const STALE = new TypeError(
  "Failed to fetch dynamically imported module: http://127.0.0.1:8080/assets/UniverseScene-rl4xgPwq.js",
);

vi.mock("../universe/UniverseScene", () => ({
  webglAvailable: () => true,
  UniverseScene: class {
    constructor() {
      throw STALE;
    }
  },
}));

const { ArchitectureRoom } = await import("../rooms/Architecture");

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("a page older than the bundle on the server", () => {
  it("says which build it is running and offers the one action that fixes it", async () => {
    vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
      const url = String(input);
      const body = url.includes("metagraph.json")
        ? metagraphJson
        : url.includes("communities.json")
          ? summaryJson
          : null;
      if (!body) return new Response("", { status: 404 });
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    });

    render(<ArchitectureRoom />);

    // Named as a stale page, not as a broken room: the coverage and the lists beside it still render.
    expect(await screen.findByText("This page is older than the bundle on the server")).toBeTruthy();
    expect(screen.getByText(/earlier build of the dashboard/)).toBeTruthy();
    expect(screen.getByText(/Nothing is wrong with the bundle/)).toBeTruthy();
    // Advice is not an affordance: the reload is a button.
    expect(screen.getByRole("button", { name: "Reload the dashboard" })).toBeTruthy();
    // And the room did not blame the data for it.
    expect(screen.queryByText(/no architecture dataset/i)).toBeNull();
  });
});
