/**
 * Static rules room — fixture index + missing/stale/empty states.
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider, createMemoryHistory } from "@tanstack/react-router";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createAppRouter } from "../router";
import kantraIndex from "./fixtures/kantra/kantra_index.json";
import fileBundle from "./fixtures/kantra/kantra/files/fileaaa111111111.json";

function stubBundle(routes: Record<string, unknown>): void {
  vi.stubGlobal("fetch", async (input: RequestInfo | URL) => {
    const url = String(input);
    if (url.includes("/api/status")) {
      const body = routes["/api/status"] ?? { kantra_findings: false };
      return new Response(JSON.stringify(body), {
        status: 200,
        headers: { "content-type": "application/json" },
      });
    }
    const key = Object.keys(routes).find((name) => name !== "/api/status" && url.includes(name));
    if (!key) return new Response("", { status: 404 });
    const body = routes[key];
    if (typeof body === "string") {
      return new Response(body, { status: 200, headers: { "content-type": "text/plain" } });
    }
    return new Response(JSON.stringify(body), {
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
  "manifest.json": {
    schema_version: 1,
    kantra: {
      available: true,
      index_path: "kantra_index.json",
      detail_dir: "kantra",
      violation_count: 3,
      evaluated_rules: 10,
    },
  },
  "kantra_index.json": kantraIndex,
  "kantra/files/fileaaa111111111.json": fileBundle,
  "sources/srcdemo0001.txt":
    "package demo;\n\nimport java.sql.Connection;\n\npublic class Demo {\n  void a() {}\n  void b() {}\n  void c() {}\n  void d() {}\n  void e() {}\n  void f() {}\n  Connection c;\n}\n",
  "cfg_index.json": { schema_version: 1, available: false, functions: [] },
  "slice_index.json": { schema_version: 1, available: false, functions: [] },
  "/api/status": { kantra_findings: true },
};

describe("the Static rules room", () => {
  it("names --with-kantra when the export is missing and no session findings", async () => {
    stubBundle({
      "manifest.json": {},
      "cfg_index.json": { schema_version: 1, available: false, functions: [] },
      "slice_index.json": { schema_version: 1, available: false, functions: [] },
      "/api/status": { kantra_findings: false },
    });
    renderAt("/rules");
    await waitFor(() => expect(screen.getByTestId("rules-room-missing")).toBeTruthy());
    expect(screen.getByText(/--with-kantra/)).toBeTruthy();
    expect(screen.queryByTestId("rules-file-list")).toBeNull();
  });

  it("shows stale when session findings exist but the bundle has no Kantra index", async () => {
    stubBundle({
      "manifest.json": {},
      "cfg_index.json": { schema_version: 1, available: false, functions: [] },
      "slice_index.json": { schema_version: 1, available: false, functions: [] },
      "/api/status": { kantra_findings: true },
    });
    renderAt("/rules");
    await waitFor(() => expect(screen.getByTestId("rules-room-stale")).toBeTruthy());
    expect(screen.getByText(/stale export/i)).toBeTruthy();
    expect(screen.queryByTestId("rules-file-list")).toBeNull();
  });

  it("shows empty when the index is available with zero violations", async () => {
    stubBundle({
      "manifest.json": {
        kantra: { available: true, index_path: "kantra_index.json" },
      },
      "kantra_index.json": {
        ...kantraIndex,
        violation_count: 0,
        file_count: 0,
        files: [],
        rules: [],
        by_category: {},
      },
      "cfg_index.json": { schema_version: 1, available: false, functions: [] },
      "slice_index.json": { schema_version: 1, available: false, functions: [] },
      "/api/status": { kantra_findings: true },
    });
    renderAt("/rules");
    await waitFor(() => expect(screen.getByTestId("rules-room-empty")).toBeTruthy());
    expect(screen.getByText(/zero hits/i)).toBeTruthy();
  });

  it("loads the index, lists files, and opens a file shard on select", async () => {
    stubBundle(readyRoutes);
    renderAt("/rules");
    await waitFor(() => expect(screen.getByTestId("rules-room")).toBeTruthy());
    expect(screen.getByTestId("rules-file-list")).toBeTruthy();
    expect(screen.getByText(/Demo\.java/)).toBeTruthy();
    fireEvent.click(screen.getByText(/Demo\.java/));
    await waitFor(() => expect(screen.getByTestId("rules-violations")).toBeTruthy());
    const ruleCells = screen.getAllByText("jdbc-to-quarkus-00001");
    expect(ruleCells.length).toBeGreaterThan(0);
    fireEvent.click(ruleCells[0]!);
    await waitFor(() => expect(screen.getByTestId("rules-violation-detail")).toBeTruthy());
    expect(screen.getByText(/Direct JDBC/)).toBeTruthy();
  });

  it("narrows the file list when only mandatory is selected", async () => {
    stubBundle(readyRoutes);
    renderAt("/rules");
    await waitFor(() => expect(screen.getByTestId("rules-category-filters")).toBeTruthy());
    const optional = screen.getByLabelText(/optional:/i);
    fireEvent.click(optional);
    // After unchecking optional, Demo.java (optional-only) should leave; Applet stays if mandatory still on.
    await waitFor(() => {
      expect(screen.queryByText(/Demo\.java/)).toBeNull();
    });
    expect(screen.getByText(/Applet\.java/)).toBeTruthy();
  });
});
