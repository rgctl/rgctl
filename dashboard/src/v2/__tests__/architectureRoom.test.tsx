/**
 * The room, rendered — against real data, in a browser that has no WebGL.
 *
 * That is not a contrivance: `jsdom` gives `getContext` nothing, so every test here exercises the absence
 * state a user on a machine without WebGL would see, while the coverage line, the three object names and the
 * lists all still render. A canvas that fails must not take the room with it.
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ArchitectureRoom } from "../rooms/Architecture";
import metagraphJson from "./fixtures/architecture/metagraph.json";
import summaryJson from "./fixtures/architecture/communities.json";

function stubBundle(routes: Record<string, unknown>): void {
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url = String(input);
    const key = Object.keys(routes).find((name) => url.includes(name));
    if (!key) return new Response("", { status: 404 });
    return new Response(JSON.stringify(routes[key]), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  });
}

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("the Architecture room", () => {
  it("says so, with the command that would fill it, when the bundle has no metagraph", async () => {
    stubBundle({});
    render(<ArchitectureRoom />);
    await waitFor(() => expect(screen.getByText(/no architecture dataset/i)).toBeTruthy());
    // the command that produces one, not just a shrug
    expect(screen.getByText(/--with-universe --with-cfg/)).toBeTruthy();
    // and it must not render an empty city
    expect(screen.queryByTestId("universe-canvas")).toBeNull();
  });

  it("leads with coverage, names its evidence, and states the three objects", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom />);

    await waitFor(() => expect(screen.getByText(/43\.1% of the graph is in this map/i)).toBeTruthy());
    // the evidence form matters: member_indices is absent on this corpus
    expect(screen.getByText(/member_count/)).toBeTruthy();
    expect(screen.getByText(/elided above 50,000 nodes/)).toBeTruthy();

    // the three words, each doing one job
    expect(screen.getByText("namespace volume")).toBeTruthy();
    expect(screen.getByText("package")).toBeTruthy();
    expect(screen.getByText("community")).toBeTruthy();

    // the counts that disagree are all reported, none chosen
    expect(screen.getByText("metagraph.json")).toBeTruthy();
    expect(screen.getByText("communities.json")).toBeTruthy();
  });

  it("draws nothing in jsdom, and says why, instead of showing a blank canvas", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom />);
    await waitFor(() => expect(screen.getByText(/could not be started/i)).toBeTruthy());
    expect(screen.getByText(/a failure of the canvas is not a failure of the room/i)).toBeTruthy();
  });

  it("names the buckets as buckets and refuses to draw their coupling", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom />);
    await waitFor(() => expect(screen.getByText(/Not architecture/i)).toBeTruthy());
    // the remainder is 15.6% of the graph, in one object, and it is labelled as the remainder
    // the label appears in the bucket list and again in the caption, so count rather than pick
    expect(screen.getAllByText(/\(other\)/).length).toBeGreaterThan(0);
    expect(screen.getByText(/1,725/)).toBeTruthy();
    expect(screen.getByText(/never written/)).toBeTruthy();
  });

  it("says clustering was not applied rather than showing one community", async () => {
    stubBundle({ "metagraph.json": metagraphJson });
    render(<ArchitectureRoom />);
    await waitFor(() => expect(screen.getByText(/Clustering was not applied/i)).toBeTruthy());
    expect(screen.getByText(/not 'one big community'/i)).toBeTruthy();
    // coverage is still stated, from the metagraph's own form of the accounting
    expect(screen.getAllByText(/could not be established/i).length).toBeGreaterThan(0);
  });

  it("attributes the modularity to Fast-Leiden and keeps the producer's labels verbatim", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom />);
    await waitFor(() => expect(screen.getByText(/Fast-Leiden/)).toBeTruthy());
    // Room shows producer strings as-is. Stopword filtering (e.g. "Mutable") happens at
    // discover/label time in rgctl-analysis — not in the Architecture room.
    expect(screen.getByText("I_C_InvoiceLine")).toBeTruthy();
  });

  it("filters communities to shared-only and reports overflow with 'and N more'", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom />);
    await waitFor(() => expect(screen.getByText(/Fast-Leiden/)).toBeTruthy());

    // Overflow line ("and N more") renders when communities exceed the top-8 slice
    expect(screen.getAllByText(/^and \d+ more$/).length).toBeGreaterThan(0);

    // Toggle from "all (...)" to "shared only (...)"
    const toggleBtn = screen.getByRole("button", { name: /^all \(\d+\)$/ });
    expect(toggleBtn.getAttribute("aria-pressed")).toBe("false");
    fireEvent.click(toggleBtn);
    expect(screen.getByRole("button", { name: /^shared only \(\d+\)$/ }).getAttribute("aria-pressed")).toBe("true");
  });

  /**
   * The room's own controls, in the order a reader uses them.
   *
   * Naming a district comes before acting on one: the lookup sat *below* Expand/Collapse, so the control that
   * answers "which district?" was the one you found last, under a button that could not be used until you had.
   */
  it("puts the district lookup above the controls that act on a district", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom />);
    // Two fields share the label (the overlay's lookup and the panel's filter), so the overlay's is the one
    // inside the canvas wrapper — that is the ordering claim being tested.
    await waitFor(() => expect(screen.getAllByLabelText("Find a district by name").length).toBeGreaterThan(0));

    const lookup = screen.getAllByLabelText("Find a district by name")[0];
    const expand = screen.getByRole("button", { name: /Expand/ });
    const collapse = screen.getByRole("button", { name: /Collapse/ });

    // Node.DOCUMENT_POSITION_FOLLOWING means the second node comes after the first.
    expect(lookup.compareDocumentPosition(expand) & 4).toBeTruthy();
    expect(lookup.compareDocumentPosition(collapse) & 4).toBeTruthy();
  });

  /**
   * A level is a link, so a link to a level that is not in this bundle has to say so.
   *
   * Checked against the data and not against the canvas: this is what a stale `?explode=` from a previous index
   * looks like, and "nothing happened" would be indistinguishable from a broken room.
   */
  it("reports a district named in the URL that this bundle does not contain", async () => {
    stubBundle({ "metagraph.json": metagraphJson, "communities.json": summaryJson });
    render(<ArchitectureRoom explode={999999} onExplodeChange={() => {}} />);
    await waitFor(() => expect(screen.getByText(/district #999999/)).toBeTruthy());
    expect(screen.getByText(/The link is stale; the map is not empty/)).toBeTruthy();
  });
});
