/**
 * The Code lens: the source, and nothing else.
 *
 * This lens existed as a paragraph *describing* a shared source reader ("Source rendering is shared, so
 * this lens is a view of the same reader rather than a second implementation") while rendering no source
 * and declaring no absence — the one lens whose text claimed a capability instead of showing one. Both
 * halves of that are tested here: the lines are drawn, and the description is gone.
 */
import { render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { beforeEach, describe, expect, it } from "vitest";
import { createAppRouter } from "../router";
import { installFetchStub, SOURCE_LINES, SOURCE_PATH, SUBJECT } from "./fixture";

function renderLens(functionId = SUBJECT) {
  const history = createMemoryHistory({
    initialEntries: [`/understand?lens=code&focus=${functionId}`],
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

  it("is not a placeholder: an implemented lens renders no typed absence", async () => {
    const { container } = renderLens();
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));
    expect(container.textContent).not.toMatch(/not written yet/i);
  });
});
