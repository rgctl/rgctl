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
import { nameMatch } from "../rooms/SourcePane";
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

    // Three levels of mark, and the reader can tell them apart: the declaration line in the light-blue theme,
    // the rest of the span faint, everything outside it plain.
    const rows = [...container.querySelectorAll("tr")];
    const declaration = rows.find((r) => r.getAttribute("data-line") === "1");
    expect(declaration?.className).toContain("bg-sky-400/[0.13]");
    const bodyLines = rows.filter((r) => r.getAttribute("data-line") !== "1");
    expect(bodyLines).toHaveLength(SOURCE_LINES.length - 1);
    for (const row of bodyLines) expect(row.className).toContain("bg-white/[0.035]");
  });

  /**
   * The two marks the drawer exists for.
   *
   * Opening a member's code from a level answers one question — *where is this function?* — and answering it with
   * a faint band over seven lines is not an answer. The name is material yellow (the only filled, high-chroma
   * mark in the pane, because a syntax theme has already spent colour on every other token), and the line that
   * declares it is the app's light blue.
   */
  it("marks the function's name in yellow, on the line that declares it", async () => {
    const { container } = renderLens();
    await waitFor(() => expect(container.textContent).toContain(SOURCE_LINES[0]));

    const marks = screen.getAllByTestId("subject-name-mark");
    // Exactly one: a declaration line names the function once.
    expect(marks).toHaveLength(1);
    expect(marks[0].textContent).toBe("handleAuth");
    expect(marks[0].className).toContain("ffd54f");
    // On the declaration line, not merely somewhere in the file.
    expect(marks[0].closest("tr")?.getAttribute("data-line")).toBe("1");
  });

  it("does not mark a name that only appears inside a longer identifier", async () => {
    // `verify` is the other fixture function and appears on line 6 as a *call*; the declaration-line search
    // must not accept `verifyToken`-style partial matches, which is why the match is word-bounded.
    expect(nameMatch("  return verifyToken(token);", "verify")).toBeNull();
    expect(nameMatch("  return verify(token);", "verify")?.hit).toBe("verify");
    // And a name with regex metacharacters is matched literally rather than compiled.
    expect(nameMatch("fn a.b(c) {}", "a.b")?.hit).toBe("a.b");
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
