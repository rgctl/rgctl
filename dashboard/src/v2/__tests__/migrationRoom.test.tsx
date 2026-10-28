/**
 * Migration room — real exported fixture + empty/missing states (no Placeholder copy).
 */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { MigrationRoom } from "../rooms/Migration";
import { Placeholder } from "../rooms/Placeholder";
import migrationGraph from "./fixtures/migration_graph.json";

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

describe("the Migration room", () => {
  it("names the discover flags when migration_graph.json is missing", async () => {
    stubBundle({});
    render(<MigrationRoom />);
    await waitFor(() => expect(screen.getAllByText(/no migration graph/i).length).toBeGreaterThan(0));
    expect(screen.getByText(/--with-universe --with-harmonic/)).toBeTruthy();
    expect(screen.queryByTestId("migration-canvas")).toBeNull();
    expect(screen.queryByText(/not written yet/i)).toBeNull();
  });

  it("distinguishes empty communities from a missing file", async () => {
    stubBundle({
      "migration_graph.json": {
        schema_version: 3,
        mode: "package_macro",
        modularity: 0,
        communities: [],
        edges: [],
      },
    });
    render(<MigrationRoom />);
    await waitFor(() => expect(screen.getByText(/zero communities/i)).toBeTruthy());
    expect(screen.getByText(/--with-universe --with-harmonic/)).toBeTruthy();
    expect(screen.queryByText(/no migration graph/i)).toBeNull();
  });

  it("renders tuning + roadmap against a real exported migration_graph.json", async () => {
    stubBundle({ "migration_graph.json": migrationGraph });
    render(<MigrationRoom />);
    await waitFor(() => expect(screen.getByTestId("migration-room")).toBeTruthy());
    expect(screen.getByTestId("migration-roadmap")).toBeTruthy();
    expect(screen.getByText("src")).toBeTruthy();
    expect(screen.getByText("com.example")).toBeTruthy();
    expect(screen.getByText(/Foundational First/i)).toBeTruthy();
    // jsdom: canvas absence stated; table still live
    await waitFor(() => expect(screen.getByText(/could not be started/i)).toBeTruthy());
  });

  it("recomputes when a preset is applied", async () => {
    stubBundle({ "migration_graph.json": migrationGraph });
    render(<MigrationRoom />);
    await waitFor(() => expect(screen.getByTestId("migration-roadmap")).toBeTruthy());
    fireEvent.click(screen.getByRole("button", { name: /Risk Mitigation/i }));
    expect(screen.getByRole("button", { name: /Risk Mitigation/i }).getAttribute("aria-pressed")).toBe(
      "true",
    );
  });
});

describe("Placeholder no longer owns Migration", () => {
  it("does not claim Migration as pending", () => {
    const { container } = render(<Placeholder room="migration" />);
    expect(container.textContent).not.toMatch(/Migration room/i);
    expect(container.textContent).not.toMatch(/add-migration/i);
  });
});
