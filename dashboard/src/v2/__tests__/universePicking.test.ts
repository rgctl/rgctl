/**
 * What a click through a translucent volume should select.
 *
 * The bug: a namespace volume is a box drawn around the packages inside it, so its near face is nearer than they
 * are and `intersectObjects(...)[0]` answered "the volume" for every click aimed at a package inside one. The
 * solids inside an x-rayed group were unclickable — visible, labelled on hover, and not selectable.
 */
import { describe, expect, it } from "vitest";
import { nearestSolid } from "../universe/UniverseScene";

const volume = { kind: "volume" as const, id: 1, title: "crates", lines: [] };
const pkg = { kind: "package" as const, id: 2, title: "rgctl-analysis", lines: [] };
const bucket = { kind: "bucket" as const, id: 3, title: "(other)", lines: [] };

describe("nearestSolid", () => {
  it("selects the package behind a volume, not the volume", () => {
    // Ray order: the shell's near face, then the solid inside it, then the shell's far face.
    expect(nearestSolid([volume, pkg, volume])).toBe(pkg);
  });

  it("selects a bucket the same way — it is a drawn object, not a label", () => {
    expect(nearestSolid([volume, bucket])).toBe(bucket);
  });

  it("still selects the volume when nothing solid is behind it", () => {
    // Clicking the empty part of a namespace shell is a click on the namespace, and has to keep working.
    expect(nearestSolid([volume])).toBe(volume);
  });

  it("reports nothing for a ray that hit nothing", () => {
    expect(nearestSolid([])).toBeNull();
  });
});
