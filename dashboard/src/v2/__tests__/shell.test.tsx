/**
 * The shell's four claims, asserted against the real components.
 *
 * Each of these was previously verified by hand against a live bundle. That is how two bugs in this branch
 * were found, and it is not a regression test: a hand check only happens when someone remembers to do it.
 */
import { render, screen, fireEvent, waitFor, within } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { beforeEach, describe, expect, it } from "vitest";
import { createAppRouter } from "../router";
import { installFetchStub, SUBJECT, OTHER, SOURCE_PATH } from "./fixture";

function makeClient() {
  return new QueryClient({
    defaultOptions: { queries: { staleTime: Infinity, refetchOnWindowFocus: false, retry: false } },
  });
}

function renderApp(initialPath: string) {
  const history = createMemoryHistory({ initialEntries: [initialPath] });
  const router = createAppRouter(history);
  render(
    <QueryClientProvider client={makeClient()}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

/** Search params as the browser would serialize them — the thing a shared URL actually carries. */
function searchOf(router: ReturnType<typeof createAppRouter>): string {
  return new URLSearchParams(
    Object.entries(router.state.location.search as Record<string, string>).filter(
      ([, v]) => v !== undefined,
    ) as [string, string][],
  ).toString();
}

beforeEach(() => installFetchStub());

describe("the Focus round-trips through the URL", () => {
  it("resolves a subject from the URL alone, and a reload keeps room, lens and subject", async () => {
    const first = renderApp(`/understand?lens=spine&focus=${SUBJECT}`);

    // Resolved from the URL: the Focus bar names the subject.
    expect(await screen.findAllByText("handleAuth")).not.toHaveLength(0);

    // "Reload": a *fresh* router built from what the URL carries must land on the same state. This is the
    // claim the whole design rests on — a view state is a link.
    const href = `${first.state.location.pathname}?${searchOf(first)}`;
    expect(href).toContain(`focus=${SUBJECT}`);
    expect(href).toContain("lens=spine");

    document.body.innerHTML = "";
    renderApp(href);
    expect(await screen.findAllByText("handleAuth")).not.toHaveLength(0);
    expect(await screen.findAllByText(SOURCE_PATH.split("/").pop()!)).not.toHaveLength(0);
  });

  it("normalises a malformed Focus instead of rendering a subject that cannot resolve", async () => {
    renderApp("/understand?focus=not-a-uuid");
    expect(await screen.findByText(/no subject selected/i)).toBeTruthy();
    // `queryByText`, not `findByText`: the latter *throws* when nothing matches, so an assertion built on
    // it can never express "this must be absent" — it fails before the assertion runs.
    expect(screen.queryByText(/does not resolve/i)).toBeNull();
  });
});

describe("a lens is a control, not a destination", () => {
  it("switching lens preserves the subject", async () => {
    const router = renderApp(`/understand?lens=spine&focus=${SUBJECT}`);
    await screen.findAllByText("handleAuth");

    fireEvent.click(screen.getByRole("button", { name: "Blast" }));

    await waitFor(() => expect(router.state.location.search).toMatchObject({ lens: "blast", focus: SUBJECT }));
    // The subject is still the one that was selected — a lens change is not a selection change.
    expect(await screen.findAllByText("handleAuth")).not.toHaveLength(0);
  });

  it("carrying the Focus to another room keeps it", async () => {
    const router = renderApp(`/understand?lens=spine&focus=${SUBJECT}`);
    await screen.findAllByText("handleAuth");

    fireEvent.click(screen.getByRole("button", { name: "security →" }));

    await waitFor(() => expect(router.state.location.pathname).toBe("/security"));
    expect(router.state.location.search).toMatchObject({ focus: SUBJECT });
  });
});

describe("a room renders a default with no Focus", () => {
  it("Understand explains how to pick a subject rather than showing an empty pane", async () => {
    renderApp("/understand");
    expect(await screen.findByText(/no subject selected/i)).toBeTruthy();
    expect(await screen.findByText(/⌘K/)).toBeTruthy();
    // Exact name: the top omnibox's accessible name is "Search symbols, communities, or jump to a room…",
    // which also matches a loose /search symbols/i and made this query ambiguous.
    expect(await screen.findByRole("button", { name: "Search symbols" })).toBeTruthy();
  });

  it("Repo ranks something to look at from the index", async () => {
    renderApp("/repo");
    expect(await screen.findByText(/this screen is not a dashboard/i)).toBeTruthy();
    expect(await screen.findByText(/symbols carry data-flow/i)).toBeTruthy();
  });
});

describe("the shell has no tab strip", () => {
  it("rooms are a labelled vertical nav and there is no tablist", async () => {
    renderApp("/repo");
    await screen.findByText(/this screen is not a dashboard/i);

    // Structural assertion, not a pixel one: a tab strip is a `tablist` of `tab`s, and the regression this
    // guards against is someone reaching for that pattern when adding change 2's room.
    expect(document.querySelectorAll('[role="tablist"]')).toHaveLength(0);
    expect(document.querySelectorAll('[role="tab"]')).toHaveLength(0);

    const nav = document.querySelector('nav[aria-label="Rooms"]');
    expect(nav).toBeTruthy();
    expect(nav!.querySelectorAll("button")).toHaveLength(7);
  });
});

describe("an unbuilt lens explains itself instead of rendering an empty pane", () => {
  it("dataflow names the lens and says the data is present", async () => {
    renderApp(`/understand?lens=dataflow&focus=${SUBJECT}`);
    // Previously this rendered `null`: `<Placeholder room="understand" />` is keyed by room, so five lens
    // tabs produced an empty pane with nothing saying why.
    // The lens names itself in the panel title and again in the explanation, so match all occurrences.
    expect((await screen.findAllByText(/Dataflow lens/)).length).toBeGreaterThan(0);
    expect(await screen.findByText(/not written yet — the data is here/)).toBeTruthy();
    expect(await screen.findByText(/dataflow_detail_dir/)).toBeTruthy();
  });

  it("metrics states the keying mismatch rather than printing a zero", async () => {
    renderApp(`/understand?lens=metrics&focus=${SUBJECT}`);
    expect(await screen.findByText(/blocked, not unbuilt/i)).toBeTruthy();
    expect(await screen.findByText(/function_metrics\.json/)).toBeTruthy();
    // The badge used to be hard-coded to "not in this bundle", which contradicted the sentence under it:
    // the metrics data *is* in the bundle, keyed by `index` (0…2944) with no `function_id` to join on.
    expect(await screen.findByText("blocked: no join key")).toBeTruthy();
    expect(screen.queryByText("not in this bundle")).toBeNull();
  });

  it("time says why one snapshot is not enough", async () => {
    renderApp(`/understand?lens=time&focus=${SUBJECT}`);
    expect(await screen.findByText(/needs a second snapshot/i)).toBeTruthy();
  });
});

describe("the palette lands you where the graph is", () => {
  it("picking a symbol from Repo moves to Understand, keeping the subject", async () => {
    const router = renderApp("/repo");
    await screen.findByText(/this screen is not a dashboard/i);

    fireEvent.click(screen.getByRole("button", { name: /Search symbols, communities/ }));
    const dialog = await screen.findByRole("dialog");
    // Scoped to the palette: the Repo room also renders symbol chips, so an unscoped query is ambiguous.
    fireEvent.click(await within(dialog).findByText("handleAuth"));

    // Setting only the Focus left Repo on screen with a subject it does not render, so the graph stayed
    // two actions away.
    await waitFor(() => expect(router.state.location.pathname).toBe("/understand"));
    expect(router.state.location.search).toMatchObject({ focus: SUBJECT });
  });

  it("does not drag you out of a room that already reads the Focus", async () => {
    const router = renderApp(`/security?focus=${SUBJECT}`);
    await screen.findByRole("button", { name: /Search symbols, communities/ });

    fireEvent.click(screen.getByRole("button", { name: /Search symbols, communities/ }));
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(await within(dialog).findByText("verify"));

    await waitFor(() => expect(router.state.location.search).toMatchObject({ focus: OTHER }));
    // Choosing a symbol while in Security should not move you to Understand.
    expect(router.state.location.pathname).toBe("/security");
  });
});
