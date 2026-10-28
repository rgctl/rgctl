/**
 * Taint room — real fixture + empty/missing states (router-wrapped; room uses useFocus).
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createAppRouter } from "../router";
import { Placeholder } from "../rooms/Placeholder";
import { TaintPathStage } from "../rooms/TaintPathStage";
import taintIndex from "./fixtures/taint/taint_index.json";
import taintFlow from "./fixtures/taint/taint/8fcef6c3-dfc0-5338-ba7b-e14cc4816997.json";
import dataflowIndex from "./fixtures/taint/dataflow_index.json";
import blastIndex from "./fixtures/taint/blast_index.json";
import cfgDetail from "./fixtures/taint/cfg/8fcef6c3-dfc0-5338-ba7b-e14cc4816997.json";
import sliceDetail from "./fixtures/taint/slice/8fcef6c3-dfc0-5338-ba7b-e14cc4816997.json";

const FN = "8fcef6c3-dfc0-5338-ba7b-e14cc4816997";

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

function renderAt(path: string) {
  const client = new QueryClient({
    defaultOptions: { queries: { retry: false, staleTime: Infinity } },
  });
  const history = createMemoryHistory({ initialEntries: [path] });
  const router = createAppRouter(history);
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return router;
}

afterEach(() => {
  vi.unstubAllGlobals();
});

const readyRoutes: Record<string, unknown> = {
  "taint_index.json": taintIndex,
  [`taint/${FN}.json`]: taintFlow,
  "dataflow_index.json": dataflowIndex,
  "blast_index.json": blastIndex,
  [`cfg/${FN}.json`]: cfgDetail,
  [`slice/${FN}.json`]: sliceDetail,
  "cfg_index.json": {
    schema_version: 1,
    available: true,
    functions: [{ function_id: FN, name: "handle", pdg_nodes: 2 }],
  },
  "slice_index.json": { schema_version: 1, available: false, functions: [] },
  "manifest.json": { schema_version: 1 },
};

describe("the Taint room", () => {
  it("names --with-taint when the index is missing", async () => {
    stubBundle({
      "cfg_index.json": { schema_version: 1, available: false, functions: [] },
      "slice_index.json": { schema_version: 1, available: false, functions: [] },
      "manifest.json": {},
    });
    renderAt("/taint");
    await waitFor(() => expect(screen.getByText(/no usable taint analysis/i)).toBeTruthy());
    expect(screen.getByText(/--with-taint/)).toBeTruthy();
  });

  it("renders path stage and flows against a real export", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-room")).toBeTruthy());
    expect(screen.getByTestId("taint-flows")).toBeTruthy();
    await waitFor(() => expect(screen.getByTestId("taint-path-stage")).toBeTruthy());
    expect(screen.getAllByText(/HttpParameter/).length).toBeGreaterThan(0);
    expect(screen.getByTestId("taint-intra-proc").textContent).toMatch(/intra-procedural/i);
  });

  it("offers blast expand instead of fabricating a zero score", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-room")).toBeTruthy());
    expect(screen.getByText(/open to expand \(WASM\)/i)).toBeTruthy();
  });

  it("shows CFG and DFG in the context metrics block", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-context-metrics")).toBeTruthy());
    const metrics = screen.getByTestId("taint-context-metrics");
    expect(metrics.textContent).toMatch(/blocks/i);
    expect(metrics.textContent).toMatch(/PDG/i);
  });

  it("lists a flow name column with severity for each row", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-flows")).toBeTruthy());
    const table = screen.getByTestId("taint-flows");
    expect(table.textContent).toMatch(/flow/i);
    expect(table.textContent).toMatch(/severity/i);
    expect(table.textContent).toMatch(/HttpParameter → SqlQuery/);
  });

  it("uses a searchable select for many functions, not a chip wall", async () => {
    const multi = {
      ...taintIndex,
      function_count: 2,
      total_flows: 2,
      vulnerable_flows: 2,
      functions: [
        taintIndex.functions[0],
        {
          function_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
          name: "otherFn",
          file_path: "other.py",
          flow_count: 1,
          vulnerable_count: 1,
        },
      ],
    };
    stubBundle({
      ...readyRoutes,
      "taint_index.json": multi,
      "taint/aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee.json": {
        schema_version: 1,
        function_id: "aaaaaaaa-bbbb-4ccc-8ddd-eeeeeeeeeeee",
        name: "otherFn",
        flows: [taintFlow.flows[0]],
      },
    });
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-function-picker")).toBeTruthy());
    expect(screen.getByTestId("taint-function-select")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "otherFn" })).toBeNull();
  });

  it("opens CFG on the canvas when Context CFG is clicked", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-open-cfg")).toBeTruthy());
    fireEvent.click(screen.getByTestId("taint-open-cfg"));
    await waitFor(() => expect(screen.getByTestId("taint-cfg-stage")).toBeTruthy());
  });

  it("opens DFG on the canvas when Context DFG is clicked", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-open-dfg")).toBeTruthy());
    fireEvent.click(screen.getByTestId("taint-open-dfg"));
    await waitFor(() => expect(screen.getByTestId("taint-dfg-stage")).toBeTruthy());
  });

  it("opens blast shells with a depth control when Context Blast is clicked", async () => {
    stubBundle(readyRoutes);
    renderAt("/taint");
    await waitFor(() => expect(screen.getByTestId("taint-open-blast")).toBeTruthy());
    fireEvent.click(screen.getByTestId("taint-open-blast"));
    await waitFor(() => expect(screen.getByTestId("taint-blast-stage")).toBeTruthy());
    expect(screen.getByTestId("taint-blast-depth")).toBeTruthy();
  });

  it("Focus miss states that Focus has no taint", async () => {
    stubBundle(readyRoutes);
    const missing = "00000000-0000-4000-8000-000000000099";
    renderAt(`/taint?focus=${missing}`);
    await waitFor(() => expect(screen.getByText(/Focus has no taint flows/i)).toBeTruthy());
  });
});

describe("TaintPathStage empty state", () => {
  it("prompts selection when no flow is selected", () => {
    render(<TaintPathStage flow={null} />);
    expect(screen.getByTestId("taint-path-empty")).toBeTruthy();
    expect(screen.getByText(/Pick a row/i)).toBeTruthy();
  });
});

describe("Placeholder no longer owns Security", () => {
  it("does not claim Security or Taint as pending", () => {
    const { container } = render(<Placeholder room={"taint" as never} />);
    expect(container.textContent).not.toMatch(/Security room/i);
    expect(container.textContent).not.toMatch(/add-security/i);
    expect(container.textContent).not.toMatch(/Taint room/i);
  });
});
