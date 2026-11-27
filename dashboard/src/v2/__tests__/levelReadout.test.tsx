/**
 * The open level's read-out.
 *
 * This text was deleted by a refactor and **nothing failed**: the level still drew, the typecheck still passed, and
 * only a screenshot showed it had gone. It is a component now precisely so that a fabricated level can be rendered
 * and its claims read — including the two caveats, which are the difference between "the numbers are missing" and
 * "the numbers are absent for a stated reason".
 */
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { LevelReadout, type LevelStats } from "../rooms/Architecture";

const level: LevelStats = {
  title: "rgctl-analysis",
  drawn: 1500,
  members: 1940,
  edges: 1311,
  truncated: true,
  ms: 120,
  measuredHeight: false,
  boundary: 72,
};

describe("LevelReadout", () => {
  it("states drawn vs total, and the truncation rule when they differ", () => {
    render(<LevelReadout stats={level} />);
    expect(screen.getByText("rgctl-analysis")).toBeTruthy();
    const line = screen.getByTestId("level-readout").textContent ?? "";
    expect(line).toContain("1,500 of 1,940 members drawn");
    expect(line).toContain("largest by member count; the rest are not drawn");
    expect(line).toContain("1,311 member edges (calls only)");
    expect(line).toContain("0.1 s");
  });

  it("says nothing about truncation when nothing was truncated", () => {
    render(<LevelReadout stats={{ ...level, drawn: 1940, truncated: false }} />);
    expect(screen.getByTestId("level-readout").textContent).not.toContain("rest are not drawn");
  });

  it("names the level change, and the height caveat only when it applies", () => {
    const { unmount } = render(<LevelReadout stats={level} />);
    expect(screen.getByText(/the universe is hidden while this level is open/)).toBeTruthy();
    expect(screen.getByText(/heights are constant/)).toBeTruthy();
    unmount();

    // A bundle that *does* report complexity must not carry the caveat.
    render(<LevelReadout stats={{ ...level, measuredHeight: true }} />);
    expect(screen.queryByText(/heights are constant/)).toBeNull();
  });

  it("omits the outgoing-coupling clause when there is none", () => {
    render(<LevelReadout stats={{ ...level, boundary: 0 }} />);
    expect(screen.getByTestId("level-readout").textContent).not.toContain("outgoing coupling");
  });
});
