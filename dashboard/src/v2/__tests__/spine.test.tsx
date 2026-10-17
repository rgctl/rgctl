/**
 * The Spine's two bindings and its honesty about missing line data.
 *
 * These are the claims that make this a code browser rather than a source viewer with a diagram next to
 * it: the graph and the code point at each other, and a block that covers no source says so.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { beforeEach, describe, expect, it } from "vitest";
import { createAppRouter } from "../router";
import { installFetchStub, SUBJECT } from "./fixture";

function renderSpine() {
  const history = createMemoryHistory({ initialEntries: [`/understand?lens=spine&focus=${SUBJECT}`] });
  const router = createAppRouter(history);
  render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } })}
    >
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

beforeEach(() => installFetchStub());

describe("source and graph are bound in both directions", () => {
  it("selecting a block highlights the lines it covers", async () => {
    renderSpine();
    await screen.findAllByText("handleAuth");

    // B1 covers lines 3-4 in the fixture. Clicking the block in the graph should mark both lines.
    const block = await screen.findByText("B1");
    fireEvent.click(block);

    await waitFor(() => expect(screen.getByText(/B1 · lines 3–4/)).toBeTruthy());
    // The source row for line 3 is marked as part of the selection.
    expect(screen.getByText("3").closest("tr")!.className).toContain("bg-sky-500/15");
  });

  it("clicking a source line selects the block that covers it", async () => {
    renderSpine();
    await screen.findAllByText("handleAuth");

    // Line 6 is inside B3 (6-7). Clicking the line number is the reverse direction.
    fireEvent.click(screen.getByText("6"));
    await waitFor(() => expect(screen.getByText(/B3 · lines 6–7/)).toBeTruthy());
  });
});

describe("blocks with no source are stated, not numbered", () => {
  it("a synthetic block renders an em dash for its line field rather than L0", async () => {
    renderSpine();
    await screen.findAllByText("handleAuth");

    // B2 has start_line 0, which is the absence of a line. "L0" would invite the reader to believe
    // otherwise — the same `—`-versus-`0` confusion fixed as D4 in the v1 dashboard.
    // The block renders its line field and statement count in one text node, so match the pair rather
    // than the dash alone — asserting on "—" alone fails for a reason that has nothing to do with the
    // behaviour under test.
    expect(await screen.findAllByText(/— · \d+ stmt/)).not.toHaveLength(0);
    expect(screen.queryByText(/^L0/)).toBeNull();
  });

  it("reports how many blocks carry source lines at all", async () => {
    renderSpine();
    await screen.findAllByText("handleAuth");
    // The fixture is 3 of 4: one synthetic block has no line.
    expect(await screen.findByText(/3\/4 blocks carry source lines/)).toBeTruthy();
  });
});
