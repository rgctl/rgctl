/**
 * The level framing solve.
 *
 * These are the invariants behind a blank screen, not properties anyone would think to check by eye. The
 * district level solved a camera against a `Box3` of the whole level — members *plus* the coupling lines that
 * deliberately run out past them — landing ~12 000 units out, and left the universe's fog range (2200…7200) in
 * force. Past the fog far plane every member renders at the background colour: nothing threw, nothing logged,
 * and the room's own read-out said "1 500 members drawn".
 *
 * The radii below bracket what a district actually spans on the measured corpus: a six-member package sits at
 * the floor of `spread` (60 units), a capped 1 500-member district at ~1 000, and the widest member set in any
 * bundle at ~3 000.
 */
import { describe, expect, it } from "vitest";
import { frameLevel, UNIVERSE_FOG } from "../universe/framing";

const RADII = [60, 260, 1007, 1500, 3000];
const FOV = 44; // the scene camera's vertical field of view

describe("frameLevel", () => {
  it("puts the camera far enough out for the whole model to be inside the field of view", () => {
    for (const radius of RADII) {
      const { distance } = frameLevel(radius, FOV);
      // The sphere's silhouette subtends 2*asin(r/d) of the vertical field of view, and must leave the margin.
      const subtended = (2 * Math.asin(Math.min(1, radius / distance)) * 180) / Math.PI;
      expect(subtended).toBeLessThan(FOV);
    }
  });

  it("fogs nothing that is inside the level", () => {
    for (const radius of RADII) {
      const { distance, fogNear } = frameLevel(radius, FOV);
      // The far side of the framed sphere is the furthest point of the level from the camera.
      expect(fogNear).toBeGreaterThan(distance + radius);
    }
  });

  it("fogs what lies beyond the level, rather than cutting it off", () => {
    for (const radius of RADII) {
      const { distance, fogNear, fogFar } = frameLevel(radius, FOV);
      expect(fogFar).toBeGreaterThan(fogNear);
      // The coupling lines leaving a district run to ~2.4x its radius past the member field.
      const furthest = distance + radius * 2.4;
      expect(fogFar).toBeGreaterThan(furthest);
    }
  });

  it("clips neither side of the model", () => {
    for (const radius of RADII) {
      const { distance, near, far } = frameLevel(radius, FOV);
      expect(near).toBeLessThan(distance - radius);
      expect(far).toBeGreaterThan(distance + radius);
      expect(near).toBeGreaterThan(0);
    }
  });

  it("would have failed when the radius came from the level's outermost line", () => {
    // The regression, pinned: framed against the coupling lines (which run ~2.4x the member radius out) the
    // camera stood past the universe's fog far plane, where the whole level is background colour.
    const asTheOldSolveMeasuredIt = frameLevel(1007 * 2.4, FOV);
    expect(asTheOldSolveMeasuredIt.distance).toBeGreaterThan(UNIVERSE_FOG[1]);
    // And even that solve keeps its own fog honest, which is what makes the range a property of the level.
    expect(asTheOldSolveMeasuredIt.fogNear).toBeGreaterThan(asTheOldSolveMeasuredIt.distance + 1007 * 2.4);
  });

  it("keeps a degenerate model frameable", () => {
    const framing = frameLevel(0, FOV);
    expect(framing.distance).toBeGreaterThan(0);
    expect(framing.fogNear).toBeGreaterThan(framing.distance);
  });
});
