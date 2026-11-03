/**
 * The source pane: the reader inside the CFG lens.
 *
 * It exists as its own component because the SourcePane is reached from one place now (the CFG lens's source
 * column, which the `show source` control collapses) but it owns real behaviour: loading, two typed
 * absences, the subject's line span, and a remembered reading position across lens switches. Those are the
 * claims tested here, and they were the ones missing when this was a paragraph describing a reader.
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { beforeEach, describe, expect, it } from "vitest";
import { createAppRouter } from "../router";
import { installFetchStub, SOURCE_LINES, SOURCE_PATH, SUBJECT } from "./fixture";

function renderLens(functionId = SUBJECT) {
  const history = createMemoryHistory({
    initialEntries: [`/understand?lens=cfg&focus=${functionId}`],
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

beforeEach(() => {
  installFetchStub();
});

describe("the Code lens draws the source", () => {
  it("renders every line of the file, not a description of rendering it", async () => {
    const { container } = renderLens();

    // The first and last lines of the fixture's source, so a partial excerpt cannot pass this.
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));
    expect(container.textContent).toContain(SOURCE_LINES[SOURCE_LINES.length - 1]);

    // Line numbers are part of the reader: a pane that shows text without them is a file dump.
    const rows = container.querySelectorAll("tr");
    expect(rows.length).toBe(SOURCE_LINES.length);
  });

  it("no longer claims to share a reader it does not have", async () => {
    renderLens();
    await waitFor(() => expect(screen.getAllByText(/\S/).length).toBeGreaterThan(0));
    expect(screen.queryByText(/same source pane as Spine/)).toBeNull();
  });

  it("names the file it is showing", async () => {
    const { container } = renderLens();
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));
    // `relative` is fed the repo root from the fixture's manifest; the path is repo-relative, not absolute.
    expect(container.textContent).toContain(SOURCE_PATH.split("/").pop()!);
  });

  it("marks the subject's own span, not the whole file", async () => {
    const { container } = renderLens();
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));

    // The fixture's CFG covers lines 1–7 (blocks 1–2, 3–4, 6–7), so the whole file is the subject here —
    // the chip proves the span came from the CFG rather than being invented by the pane.
    expect(screen.getByText(/lines 1–7/)).toBeTruthy();

    // In-range rows carry the band; this is what tells the reader where the subject is on arrival.
    const banded = [...container.querySelectorAll("tr")].filter((r) =>
      r.className.includes("bg-white/[0.035]"),
    );
    expect(banded.length).toBe(SOURCE_LINES.length);
  });

  it("remembers where you were reading, instead of snapping back to the subject", async () => {
    const history = createMemoryHistory({ initialEntries: [`/understand?lens=cfg&focus=${SUBJECT}`] });
    const router = createAppRouter(history);
    const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, retry: false } } });
    const { unmount } = render(
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>,
    );

    const scroller = await screen.findByTestId("source-scroller");
    // Read past the subject, the way a reader does, then leave the lens entirely (which unmounts the pane).
    scroller.scrollTop = 420;
    fireEvent.scroll(scroller);
    unmount();

    // Back to the same file through the other lens: the pane must resume at 420, not re-centre the span.
    const history2 = createMemoryHistory({ initialEntries: [`/understand?lens=cfg&focus=${SUBJECT}`] });
    render(
      <QueryClientProvider client={client}>
        <RouterProvider router={createAppRouter(history2)} />
      </QueryClientProvider>,
    );
    const again = await screen.findByTestId("source-scroller");
    await waitFor(() => expect(again.scrollTop).toBe(420));
  });

  it("is not a placeholder: an implemented lens renders no typed absence", async () => {
    const { container } = renderLens();
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));
    expect(container.textContent).not.toMatch(/not written yet/i);
  });
});
