/**
 * One reader, three framings — and the framing is the caller's choice.
 *
 * `SymbolReader` is the composition the CFG lens used to own privately. It has a second caller now: the
 * Architecture room opens a member's code beside the level it is inside, and maximized that is exactly this
 * reader. The claims worth pinning are about the **variants**, not the drawing (the CFG lens's own tests own
 * that):
 *
 *   * `source` draws the code and no graph, and — with no graph beside it — makes no row clickable and claims no
 *     block, because a control that selects nothing is worse than a plain line of text;
 *   * `source-and-graph` draws both.
 *
 * The first is the one that rots silently if the variant is wired wrongly.
 */
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it } from "vitest";
import { SymbolReader } from "../rooms/SymbolReader";
import { installFetchStub, SOURCE_LINES, SUBJECT_ENTRY } from "./fixture";

function renderReader(variant: "source" | "source-and-graph") {
  return render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } })}
    >
      <SymbolReader entry={SUBJECT_ENTRY} variant={variant} />
    </QueryClientProvider>,
  );
}

beforeEach(() => {
  installFetchStub();
});

describe("the source-only framing", () => {
  it("draws the code and no control-flow graph", async () => {
    const { container } = renderReader("source");

    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));
    expect(container.textContent).toContain(SOURCE_LINES[SOURCE_LINES.length - 1]);

    // What the drawer is before it is maximized: the function, and nothing beside it.
    expect(screen.queryByText("Control flow")).toBeNull();
    expect(screen.queryByText(/dominance frontier/i)).toBeNull();
    // Where the subject sits in its file is still announced. The span comes from the CFG, which is a second
    // query, so it can land after the source does.
    await waitFor(() => expect(container.textContent).toMatch(/lines \d+–\d+/));
  });

  it("claims no block it cannot show", async () => {
    const { container } = renderReader("source");
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));

    // No block chip, and no row offering a block selection: with no graph there is nothing to select.
    expect(screen.queryByText(/^B\d+/)).toBeNull();
    const rows = [...container.querySelectorAll("tr")];
    expect(rows.length).toBe(SOURCE_LINES.length);
    for (const row of rows) {
      expect(row.getAttribute("title") ?? "").not.toMatch(/block/i);
      expect(row.className).not.toContain("cursor-pointer");
    }
  });
});

describe("the graph-inclusive framing", () => {
  it("draws both halves", async () => {
    const { container } = renderReader("source-and-graph");
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));

    expect(screen.getByText("Control flow")).toBeTruthy();
    // The control and the footnote both name it; the control is the one that must exist.
    expect(screen.getByRole("button", { name: "dominance frontier" })).toBeTruthy();
  });
});
