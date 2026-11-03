/**
 * Where the camera, the clip planes and the fog sit for a model of a given radius.
 *
 * Separated from the scene because the interesting part is arithmetic and it has already been wrong, in a way
 * that no amount of reading the scene code caught: **fog is a `Scene` property**. The district level solved its
 * own camera and clip planes and inherited the universe's fog range (2200…7200) — and it solved that camera
 * against a `Box3` of the whole level, which includes the coupling lines that deliberately run out past the
 * members, so it stood ~12 000 units away. Past the fog's far plane, every member renders at 100% fog colour,
 * which is the background: the screen was not empty, it was covered. Either mistake alone dims the level; the
 * two together erase it.
 *
 * So the four numbers come from one solve, the radius is the model's own extent rather than its outermost line,
 * and the invariants are pinned in `__tests__/framing.test.ts`.
 */

/** The universe's own ranges, and what every level is restored to when it closes. */
export const UNIVERSE_FOG: readonly [number, number] = [2200, 7200];

export interface Framing {
  /** Distance from the centre of the framed sphere to the camera. */
  distance: number;
  near: number;
  far: number;
  fogNear: number;
  fogFar: number;
}

/**
 * @param radius  half-extent of what is being framed, in world units — not a box diagonal.
 * @param fovDegrees the camera's vertical field of view.
 * @param margin  1 puts the sphere's silhouette exactly on the panel edge; the default leaves room for the
 *                tilt, because a flat disc seen from above needs space toward its near edge.
 */
export function frameLevel(radius: number, fovDegrees: number, margin = 1.45): Framing {
  const extent = Math.max(radius, 1);
  const fov = (fovDegrees * Math.PI) / 180;
  const distance = (extent / Math.sin(fov / 2)) * margin;
  return {
    distance,
    // The near plane sits inside the near face of the sphere, the far plane outside its far face: a model that
    // is clipped by its own camera is a worse failure than one framed loosely.
    near: Math.max(0.1, extent / 500),
    far: Math.max(90000, distance + extent * 4),
    // Just past the far side of the sphere, so nothing *in the level* is fogged, while the coupling lines that
    // deliberately run out past the members still get a little depth help on the way.
    fogNear: distance + extent * 1.1,
    fogFar: distance + extent * 5.5,
  };
}
