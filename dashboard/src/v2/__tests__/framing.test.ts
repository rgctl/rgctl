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
import * as THREE from "three";
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

/**
 * Where a city is centred — measured, because "the map sits low" is a claim about pixels.
 *
 * The shape below is this corpus's own: a disc of radius ~131 with solids up to 150 tall, standing on a floor at
 * −4. Aiming at the middle of the bounding box put the disc's centre at NDC −0.23 while the box centre sat dead
 * middle — the framing was mathematically correct and visibly wrong, which is the kind of bug only a projection
 * can settle.
 */
function cityFrame(aimAt: "floor" | "box-centre") {
  const R = 131;
  const top = 150;
  const floor = -4;
  const box = new THREE.Box3(new THREE.Vector3(-R, floor, -R), new THREE.Vector3(R, top, R));
  const centre = box.getCenter(new THREE.Vector3());
  const aim = aimAt === "floor" ? new THREE.Vector3(centre.x, floor, centre.z) : centre;

  let radius = 0;
  for (const x of [box.min.x, box.max.x]) {
    for (const y of [box.min.y, box.max.y]) {
      for (const z of [box.min.z, box.max.z]) {
        radius = Math.max(radius, new THREE.Vector3(x, y, z).distanceTo(aim));
      }
    }
  }
  const fov = 44;
  const distance = (radius / Math.sin((fov * Math.PI) / 360)) * 1.25;
  const camera = new THREE.PerspectiveCamera(fov, 1.6, 1, 90000);
  camera.position.copy(aim).addScaledVector(new THREE.Vector3(0.55, 0.62, 0.75).normalize(), distance);
  camera.lookAt(aim);
  camera.updateMatrixWorld();

  // `Vector3.project(camera)` — the camera has `projectionMatrix`, the vector does the projecting.
  const onScreen = (v: THREE.Vector3) => v.clone().project(camera).y;

  // The disc's own vertical extent on screen: 96 points around its rim.
  const ys: number[] = [];
  for (let i = 0; i < 96; i += 1) {
    const a = (i / 96) * Math.PI * 2;
    ys.push(onScreen(new THREE.Vector3(Math.cos(a) * R, 0, Math.sin(a) * R)));
  }
  return { onScreen, discCentre: (Math.min(...ys) + Math.max(...ys)) / 2, aimY: onScreen(aim) };
}

describe("a city is framed on its floor", () => {
  it("puts the map in the middle of the panel", () => {
    const { discCentre, aimY } = cityFrame("floor");
    expect(Math.abs(aimY)).toBeLessThan(0.001);
    // The disc is a wide flat thing seen at a tilt, so its projected bounding box is not exactly symmetric
    // around its centre; a few percent is perspective, everything beyond that is a misplaced aim.
    expect(Math.abs(discCentre)).toBeLessThan(0.05);
  });

  it("keeps the tallest solid inside the frame while doing it", () => {
    const { onScreen } = cityFrame("floor");
    // Nothing may be clipped: the tower's top and both rims stay inside the panel.
    expect(onScreen(new THREE.Vector3(0, 150, 0))).toBeLessThan(1);
    expect(onScreen(new THREE.Vector3(0, 0, -131))).toBeLessThan(1);
    expect(onScreen(new THREE.Vector3(0, 0, 131))).toBeGreaterThan(-1);
  });

  it("is what the box-centre rule got wrong", () => {
    // The regression, pinned: centring the bounding box leaves the map a quarter of the way down.
    expect(cityFrame("box-centre").discCentre).toBeLessThan(-0.15);
  });
});
