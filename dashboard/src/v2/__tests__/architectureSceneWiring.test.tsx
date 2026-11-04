/**
 * The scene's callbacks reach the room — the wiring the review found unwired.
 *
 * A double-click in the 3D scene calls through a ref (`expandRef`), because the scene is created once and holds
 * the callback it was given. When that ref is not synced, the gesture calls the initial no-op and **nothing
 * happens**: no exception, no state change, no failing test. That is exactly what happened — `expandRef`'s
 * effect was overwritten by a duplicated `pickMemberRef` line — and it survived every gate, so it is pinned here
 * at the only level jsdom can honestly test: a fake scene that records the callbacks the room installs, fired by
 * the test.
 *
 * The mock deliberately implements nothing: the claim is not that a member expands in a browser without WebGL,
 * it is that the room *answers* the scene. `reading … membership…` appears only after `expand()` runs, so its
 * presence is proof the ref was wired.
 */
import { act, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import metagraphJson from "./fixtures/architecture/metagraph.json";
import summaryJson from "./fixtures/architecture/communities.json";
import { installFetchStub } from "./fixture";

/** The callbacks the room installed on the scene, as the scene would have received them. */
const installed: {
  press?: () => void;
  explode?: (entry: { kind: string; id?: number; title: string } | null) => void;
  memberOpen?: (member: unknown) => void;
  memberPick?: (member: unknown) => void;
  pick?: (entry: unknown) => void;
} = {};

vi.mock("../universe/UniverseScene", () => ({
  webglAvailable: () => true,
  UniverseScene: class {
    constructor() {
      // Every method the room calls on construction and on update; the assertions are about the callbacks.
    }
    setDrift() {}
    onHoverChange() {}
    onPickChange(fn: (entry: unknown) => void) {
      installed.pick = fn;
    }
    onMemberPickChange(fn: (member: unknown) => void) {
      installed.memberPick = fn;
    }
    onMemberOpen(fn: (member: unknown) => void) {
      installed.memberOpen = fn;
    }
    onExplode(fn: (entry: { kind: string; id?: number; title: string } | null) => void) {
      installed.explode = fn;
    }
    onPressChange(fn: () => void) {
      installed.press = fn;
    }
    attachPointer() {}
    setEdges() {}
    update() {}
    resize() {}
    dispose() {}
    collapse() {}
    highlightSolid() {}
    focusSolid() {}
    highlightMember() {}
    get isExpanded() {
      return false;
    }
  },
}));

const { ArchitectureRoom } = await import("../rooms/Architecture");

/**
 * The architecture fixtures and the shared function/source fixtures in one bundle.
 *
 * Both are needed: the room will not draw without `metagraph.json`, and the member path will not resolve without
 * the function index. `ResizeObserver` is stubbed for the same reason the scene mock exists — jsdom has no layout,
 * and its absence would otherwise be reported by the room as "drawing unavailable", which is a different claim
 * from the wiring being tested here.
 */
function stubBundle(): void {
  vi.stubGlobal("ResizeObserver", class {
    observe() {}
    unobserve() {}
    disconnect() {}
  });
  // The shared fixture bundle first (functions, source, cfg, manifest), then the architecture fixtures on top.
  const shared = installFetchStub();
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes("metagraph.json")) return json(metagraphJson);
    if (url.includes("communities.json")) return json(summaryJson);
    return shared(input);
  });
}

function json(body: unknown): Response {
  return new Response(JSON.stringify(body), { status: 200, headers: { "content-type": "application/json" } });
}

afterEach(() => {
  vi.unstubAllGlobals();
  // Every field, not just the one a test used: a callback left over from the previous test's (now unmounted) room
  // satisfies the next test's `waitFor` and quietly tests nothing — the same shape of defect as the `skip:` that
  // reported success in `tests/http_serve.rs`.
  installed.press = undefined;
  installed.explode = undefined;
  installed.memberOpen = undefined;
  installed.memberPick = undefined;
  installed.pick = undefined;
});

describe("the scene's callbacks reach the room", () => {
  it("answers a double-clicked district instead of calling into nothing", async () => {
    stubBundle();
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } })}
      >
        <ArchitectureRoom />
      </QueryClientProvider>,
    );

    // The room installs its explode handler when the scene is created.
    await waitFor(() => expect(installed.explode).toBeTypeOf("function"), { timeout: 5000 });

    // A district the fixture's metagraph actually contains: the room reads its label from the view.
    const first = (metagraphJson as { nodes: Array<{ id: number; label?: string }> }).nodes[0];
    await act(async () => {
      installed.explode?.({ kind: "package", id: first.id, title: first.label ?? "district" });
    });

    // The room answers with a typed absence (this fixture's metagraph carries no membership), which is only
    // reachable if `expand()` ran — the initial no-op produces no state at all.
    await waitFor(
      () => expect(screen.getByText(/carries no membership for this package/i)).toBeTruthy(),
      { timeout: 5000 },
    );
  });

  it("stops the drift on a press, and says so in its own control", async () => {
    stubBundle();
    render(
      <QueryClientProvider
        client={new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } })}
      >
        <ArchitectureRoom />
      </QueryClientProvider>,
    );
    await waitFor(() => expect(installed.press).toBeTypeOf("function"), { timeout: 5000 });
    // The room opens with drift on; a press anywhere on the map ends it, and the toggle is the room's own claim
    // about that state rather than a second opinion.
    expect(screen.getByRole("button", { name: /drift on/i })).toBeTruthy();
    await act(async () => {
      installed.press?.();
    });
    await waitFor(() => expect(screen.getByRole("button", { name: /drift off/i })).toBeTruthy());
  });

  /**
   * **Not tested here: the member double-click.** It is the same class of wiring, but its observable end is the
   * drawer, and reaching it needs a member from an open level — which needs the WASM engine and a WebGL scene
   * jsdom cannot provide. A test that mocks all three would assert its own mocks. The path was verified in the
   * browser instead, and `symbolReader.test.tsx` covers the drawer's own behaviour once it is open.
   */
});
