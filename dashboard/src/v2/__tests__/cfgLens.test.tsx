/**
 * The CFG·PDG lens: the graph as a surface, not a guest of the source pane.
 *
 * The claims here are the ones the lens exists to make, and each one was previously only true of the
 * retired prototype: the stat row reports the bundle's own numbers, the dominance-frontier overlay can be
 * switched off, clicking a block reads it out, and a subject with no CFG in the bundle says so instead of
 * rendering a canvas forever.
 */
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { beforeEach, describe, expect, it } from "vitest";
import { createAppRouter } from "../router";
import { installFetchStub, OTHER, SUBJECT } from "./fixture";

function renderLens(functionId = SUBJECT) {
  const history = createMemoryHistory({
    initialEntries: [`/understand?lens=cfgpddg&focus=${functionId}`],
  });
  const router = createAppRouter(history);
  const { container } = render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } })}
    >
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return { router, container };
}

/** A `Stat` renders its label and its value as sibling divs, so the value is read from the card. */
function statValue(label: string): string {
  const card = screen.getByText(label).parentElement!;
  return card.children[1]!.textContent ?? "";
}

beforeEach(() => installFetchStub());

describe("the lens draws the bundle's CFG", () => {
  it("reports blocks, edges, frontiers and exits from the sidecar", async () => {
    renderLens();
    await screen.findAllByText("handleAuth");

    // The fixture's CFG: 4 blocks, 4 edges, one block in a dominance frontier (B2), one exit (B3).
    await waitFor(() => expect(statValue("Blocks")).toBe("4"));
    expect(statValue("Control-flow edges")).toBe("4");
    expect(statValue("Dominance frontiers")).toBe("1");
    expect(statValue("Exit blocks")).toBe("1");

    // The nodes and the edge-type legend come from the same payload.
    expect(screen.getByText("B1")).toBeTruthy();
    expect(screen.getByText("if_true")).toBeTruthy();
  });

  it("states the depth the layout is built on", async () => {
    renderLens();
    await screen.findAllByText("handleAuth");
    // idom [0,0,1,1]: B0 at depth 0, B1 at 1, B2 and B3 at 2.
    await waitFor(() => expect(screen.getByText("depth 2")).toBeTruthy());
  });
});

describe("the dominance-frontier overlay is a control, not decoration", () => {
  it("dots the merge a φ belongs in, not the branch that reaches it", async () => {
    const { container } = renderLens();
    await screen.findAllByText("handleAuth");

    // The fixture writes `dominance_frontiers: [[], [2], [], []]`, i.e. DF(B1) = {B2}: B2 is the join
    // (predecessors B1 and B3) and B1 is the branch. The dot belongs on B2 — reading the rows the other
    // way round put it on B1, which is the block whose definitions travel, not the φ site.
    await waitFor(() => expect(container.querySelectorAll("circle")).toHaveLength(1));
    const dot = container.querySelector("circle")!;
    expect(dot.closest("g")!.textContent).toContain("B2");
  });

  it("removes the dots when switched off", async () => {
    const { container } = renderLens();
    await screen.findAllByText("handleAuth");

    const toggle = await screen.findByRole("button", { name: /dominance frontiers on/ });
    expect(toggle.getAttribute("aria-pressed")).toBe("true");

    fireEvent.click(toggle);
    await waitFor(() => expect(container.querySelectorAll("circle")).toHaveLength(0));
    expect(screen.getByRole("button", { name: /dominance frontiers off/ })).toBeTruthy();
  });
});

describe("clicking a block reads it out", () => {
  it("shows the block's statements, its line span and its frontier membership", async () => {
    renderLens();
    await screen.findAllByText("handleAuth");

    fireEvent.click(await screen.findByText("B1"));

    await waitFor(() => expect(screen.getByText("if (!token)")).toBeTruthy());
    expect(screen.getByText("1 statement(s) shown")).toBeTruthy();
  });

  it("marks a selected frontier block as such", async () => {
    renderLens();
    await screen.findAllByText("handleAuth");

    fireEvent.click(await screen.findByText("B2"));

    await waitFor(() => expect(screen.getByText("in a dominance frontier")).toBeTruthy());
  });
});

describe("a subject with no CFG in the bundle is stated", () => {
  it("says the graph is absent and gives the command that would produce it", async () => {
    renderLens(OTHER);
    await screen.findAllByText("verify");

    expect(await screen.findByText(/No control-flow graph bundled for verify/)).toBeTruthy();
    // Not a spinner: the previous behaviour was "loading…" forever, which reads as a slow disk.
    expect(screen.queryByText("loading…")).toBeNull();
    expect(screen.getByText(/inspect verify cfg/)).toBeTruthy();
  });
});
