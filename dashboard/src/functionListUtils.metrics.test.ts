import { describe, expect, it } from "vitest";
import {
  formatBlast,
  formatBlastTooltip,
  formatCentrality,
  formatCentralityTooltip,
} from "./functionListUtils";

describe("formatCentrality", () => {
  it("distinguishes an unavailable metric from a computed zero", () => {
    // `undefined` means no metrics row was exported for this node.
    expect(formatCentrality(undefined)).toBe("—");
    // A computed zero is real information and must not read as "missing".
    expect(formatCentrality(0)).toBe("0");
  });

  it("treats non-finite and negative values as unavailable", () => {
    expect(formatCentrality(Number.NaN)).toBe("—");
    expect(formatCentrality(Number.POSITIVE_INFINITY)).toBe("—");
    expect(formatCentrality(-1)).toBe("—");
  });

  it("uses scientific notation below 0.01 and 3 decimals above", () => {
    expect(formatCentrality(0.0044)).toBe("4.40e-3");
    expect(formatCentrality(0.5)).toBe("0.500");
    expect(formatCentrality(0.01)).toBe("0.010");
  });
});

describe("formatCentralityTooltip", () => {
  it("supplies a value for zero but not for unavailable", () => {
    expect(formatCentralityTooltip(0)).toBe("0.00000");
    expect(formatCentralityTooltip(undefined)).toBeUndefined();
  });
});

describe("formatBlast", () => {
  it("distinguishes an unavailable metric from a computed zero", () => {
    expect(formatBlast(undefined)).toBe("—");
    expect(formatBlast(0)).toBe("0");
    expect(formatBlast(42.4)).toBe("42");
    expect(formatBlast(-1)).toBe("—");
  });

  it("supplies a tooltip for zero but not for unavailable", () => {
    expect(formatBlastTooltip(0)).toBe("0.00");
    expect(formatBlastTooltip(undefined)).toBeUndefined();
  });
});
