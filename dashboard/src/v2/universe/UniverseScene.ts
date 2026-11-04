/**
 * The universe canvas: three.js, mounted imperatively.
 *
 * Deliberately not `@react-three/fiber`. The scene is a handful of instanced-free meshes, one line-segment
 * set per layer and a camera; there is no React state in it, and R3F would put a reconciler on top of a
 * dependency that is already the single largest thing in this bundle. `docs/design/dashboard-v2.md` records
 * the measurement behind that choice.
 *
 * What it draws, and why each is drawn the way it is:
 *   * **volumes** — a derived namespace group, translucent shell plus wireframe, area from its members;
 *   * **packages** — one solid each: height = log(members), footprint = sqrt(functions) clamped to local
 *     spacing, coloured by whether the package shares its community;
 *   * **buckets** (`(other)`, `root`, absolute paths) — in a band of their own, wireframe, never inside a
 *     volume: they belong to no namespace because they are not namespaces;
 *   * **the web** — every edge between real packages, brightness = log(weight) on an additive blend. Edges
 *     that touch a bucket are excluded and the room says how many and why.
 */
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import { LineSegments2 } from "three/addons/lines/LineSegments2.js";
import { LineSegmentsGeometry } from "three/addons/lines/LineSegmentsGeometry.js";
import { LineMaterial } from "three/addons/lines/LineMaterial.js";
import {
  communityHueIndex,
  footprintScale,
  heightScale,
  hueFor,
  layoutVolumes,
  sunflower,
} from "../architectureLayers";
import type { ArchitectureView, CommunityGroup, PackageNode } from "../architectureTypes";
import { frameLevel, UNIVERSE_FOG } from "./framing";

export type UniverseLayer = "packages" | "communities";

/** One drawable line: both ends and the weight its brightness is taken from. */
type Segment = [[number, number, number], [number, number, number], number];

export interface UniverseInput {
  view: ArchitectureView;
  layer: UniverseLayer;
  edges: boolean;
  height: (size: number) => number;
  footprint: (functions: number) => number;
}

/** A drawn member, carrying what the dashboard needs to turn it into a Focus. */
export interface MemberEntry {
  index: number;
  name: string;
  nodeTypeName: string;
  filePath: string | null;
  /**
   * Its slot in the level's `InstancedMesh`.
   *
   * Separate from `index`: members are drawn as a *subset* of a district (the truncation ceiling binds on real
   * corpora), so a graph index is not an instance id and resolving one to the other here would be arithmetic
   * nobody checked. It is the handle a highlight addresses.
   */
  instance?: number;
}

export interface HoverEntry {
  kind: "package" | "volume" | "bucket" | "community";
  title: string;
  lines: string[];
  /** Metanode / aggregate id, when one exists — the handle an expand is addressed by. */
  id?: number;
}

const SINGLE = 0x67788c; // a package alone in its community — a finding, not a failure
/**
 * How long a press waits before it flies the camera.
 *
 * Long enough to swallow the second press of a double-click (a 240 ms gap is a slow double-click), short enough
 * that a single click still feels like a move rather than a lag. The *mark* is immediate either way, so nothing
 * waits on this except the camera.
 */
const DOUBLE_CLICK_WINDOW_MS = 240;

/**
 * Width of the coupling lines that leave an open level, in device-independent pixels.
 *
 * One pixel is what `LineBasicMaterial` gives you on every platform that matters, and one pixel is what flickers:
 * at this level's framing a segment can land between two pixel centres and blink as the camera moves. 2.5 is thick
 * enough to hold a stable line and thin enough not to read as a beam.
 */
const LEVEL_EDGE_WIDTH_PX = 2.5;

export class UniverseScene {
  private readonly scene = new THREE.Scene();
  private readonly camera: THREE.PerspectiveCamera;
  private readonly renderer: THREE.WebGLRenderer;
  private readonly controls: OrbitControls;
  private readonly world = new THREE.Group();
  /**
   * The scene's fog, held by its own type.
   *
   * `Scene.fog` is `Fog | FogExp2 | null`, so every range change through it needs a narrowing that is dead
   * code here — and, worse, implies the range is the scene's business. It is the *level's*: see `frameLevel`.
   */
  private readonly fog = new THREE.Fog(0x060a12, UNIVERSE_FOG[0], UNIVERSE_FOG[1]);
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly pickables: { mesh: THREE.Mesh; entry: HoverEntry }[] = [];
  private readonly disposables: { dispose(): void }[] = [];
  private frame = 0;
  private mounted = true;
  private reducedMotion = false;
  private onHover: (entry: HoverEntry | null) => void = () => {};
  private onPick: (entry: HoverEntry | null) => void = () => {};
  private onExplodeRequest: (entry: HoverEntry | null) => void = () => {};
  private onMemberPick: (member: MemberEntry | null) => void = () => {};
  private onMemberOpenRequest: (member: MemberEntry | null) => void = () => {};
  private onPress: () => void = () => {};

  constructor(container: HTMLElement) {
    this.reducedMotion = globalThis.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    this.scene.background = new THREE.Color(0x060a12);
    this.scene.fog = this.fog;

    this.camera = new THREE.PerspectiveCamera(44, 1, 1, 90000);
    this.camera.position.set(760, 620, 900);

    this.renderer = new THREE.WebGLRenderer({ antialias: true });
    this.renderer.setPixelRatio(Math.min(globalThis.devicePixelRatio ?? 1, 2));
    container.appendChild(this.renderer.domElement);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    Object.assign(this.controls, {
      enableDamping: true, dampingFactor: 0.08, minDistance: 120, maxDistance: 12000,
      autoRotate: true, autoRotateSpeed: 0.28, maxPolarAngle: Math.PI * 0.495,
    });

    this.scene.add(new THREE.HemisphereLight(0x9dbfe6, 0x121a26, 0.95));
    const key = new THREE.DirectionalLight(0xffffff, 1.1);
    key.position.set(600, 900, 500);
    this.scene.add(key);
    const fill = new THREE.DirectionalLight(0x8fb4ff, 0.55);
    fill.position.set(-700, 400, -500);
    this.scene.add(fill);
    this.scene.add(this.world);

    this.resize(container);
    this.loop();
  }

  onHoverChange(fn: (entry: HoverEntry | null) => void): void {
    this.onHover = fn;
  }

  onPickChange(fn: (entry: HoverEntry | null) => void): void {
    this.onPick = fn;
  }

  /** Double-click asks for the next level. The room decides whether the subject *has* one. */
  onExplode(fn: (entry: HoverEntry | null) => void): void {
    this.onExplodeRequest = fn;
  }

  /** A single click on a member: name it. This room has two levels of intent on one surface. */
  onMemberPickChange(fn: (member: MemberEntry | null) => void): void {
    this.onMemberPick = fn;
  }

  /** A double-click on a member: go to its code. The join between the universe and the rest of the dashboard. */
  onMemberOpen(fn: (member: MemberEntry | null) => void): void {
    this.onMemberOpenRequest = fn;
  }

  /**
   * Any press on the canvas, before it is interpreted.
   *
   * The room uses it to stop the drift: a model that keeps turning while you are trying to click a box is the
   * difference between a map you can use and one that moves under the pointer. Reported rather than done here,
   * because the drift is the room's toggle and one of them has to be the truth.
   */
  onPressChange(fn: () => void): void {
    this.onPress = fn;
  }

  /**
   * Which member a ray hit, if any.
   *
   * Members are one `InstancedMesh`, so a hit reports an `instanceId` rather than an object: the mapping back
   * to a node is the array order the instances were built in, which is why it is stored beside the mesh
   * instead of being re-derived here.
   */
  private memberAt(): { instance: number; entry: MemberEntry } | null {
    if (!this.drill) return null;
    const hit = this.raycaster.intersectObject(this.drill.instanced, false)[0];
    if (!hit || hit.instanceId === undefined) return null;
    const entry = this.drill.members[hit.instanceId];
    return entry ? { instance: hit.instanceId, entry } : null;
  }

  resize(container: HTMLElement): void {
    const width = Math.max(1, container.clientWidth);
    const height = Math.max(1, container.clientHeight);
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    // `updateStyle` must stay on (the default). With `false`, three sets the *backing store* to width x dpr
    // and never writes `style.width/height`, so the element's CSS size falls back to its attribute size: on a
    // 2x display the canvas renders at twice the size of its container, is clipped by the panel's
    // `overflow: hidden`, and the model ends up parked in the bottom-right of the viewport.
    this.renderer.setSize(width, height);
    // `Line2` widths are in screen pixels, so a resize has to tell the material what the screen is now — the
    // drawing buffer, not the CSS box, because that is what the shader divides by on a 2x display.
    this.renderer.getDrawingBufferSize(this.size);
    this.levelLines?.resolution.set(this.size.x, this.size.y);
  }

  /** Rebuild the scene for a new view. Cheap enough (256 solids, 6,524 segments) to do on every toggle. */
  /** Which layer is currently drawn. Only the first build frames the camera. */
  private currentLayer: UniverseLayer | null = null;

  update(input: UniverseInput): void {
    // Frame **once**, on the first build. Every later update — switching the layer, toggling the web — now
    // leaves the camera exactly where the reader put it. Re-framing on a layer change was still a recentre,
    // and the reader who had just orientated the view did not ask for a new one.
    const refit = this.currentLayer === null;
    this.currentLayer = input.layer;
    this.clear();
    this.pickables.length = 0;
    if (input.layer === "packages") this.drawPackages(input);
    else this.drawCommunities(input);
    if (refit) this.frameWorld();
  }

  private drawPackages(input: UniverseInput): void {
    const { view, height, footprint } = input;
    const seat = (n: number) => (20 / 2.4) * Math.sqrt(n);
    const layout = layoutVolumes(view.volumes, view.standalone.length, { seatRadius: seat, pad: 16 });
    const positions = new Map<number, [number, number, number]>();

    for (const volume of view.volumes) {
      const placed = layout.volumes.find((v) => v.key === volume.key);
      if (!placed) continue;
      const top = Math.max(...volume.packages.map((p) => height(p.size))) + 26;
      const box = new THREE.BoxGeometry(placed.radius * 2 + 40, top, placed.radius * 2 + 40);
      const shell = new THREE.Mesh(box, new THREE.MeshBasicMaterial({
        color: 0x8fd0ff, transparent: true, opacity: 0.05, depthWrite: false,
      }));
      shell.position.set(placed.centre[0], top / 2 - 4, placed.centre[1]);
      this.add(shell);
      this.coreObjects.push(shell);
      const wire = new THREE.LineSegments(
        new THREE.EdgesGeometry(box),
        new THREE.LineBasicMaterial({ color: 0xa8dcff, transparent: true, opacity: 0.5 }),
      );
      wire.position.copy(shell.position);
      this.add(wire);
      this.pickables.push({
        mesh: shell,
        entry: {
          kind: "volume",
          title: volume.key,
          lines: [
            `namespace group · ${volume.packages.length} packages`,
            `${volume.members.toLocaleString()} members · ${volume.functions.toLocaleString()} functions`,
            "derived from the package labels — not an exported object",
          ],
        },
      });

      volume.packages.forEach((pkg, index) => {
        positions.set(pkg.id, sunflower(placed.centre, seat(volume.packages.length), index, volume.packages.length));
      });
    }

    view.standalone.forEach((pkg, index, all) => {
      const angle = (index / Math.max(1, all.length)) * Math.PI * 2;
      positions.set(pkg.id, [Math.cos(angle) * layout.artRadius, 0, Math.sin(angle) * layout.artRadius]);
    });
    const buckets = [...(view.remainder ? [view.remainder] : []), ...view.artifacts];
    buckets.forEach((pkg, index) => {
      const angle = Math.PI * 0.5 + (index - buckets.length / 2) * 0.06;
      positions.set(pkg.id, [
        Math.cos(angle) * layout.artifactRadius, 0, Math.sin(angle) * layout.artifactRadius,
      ]);
    });

    // `view.packages` already includes every standalone package (kind === "package");
    // do not spread `view.standalone` again or we double-mesh those solids.
    const drawn = [...view.packages, ...buckets];
    for (const pkg of drawn) {
      const pos = positions.get(pkg.id);
      if (!pos) continue;
      this.solid(pkg, pos, height(pkg.size), footprint(pkg.functions));
    }

    if (input.edges) this.web(drawn, positions, view);
  }

  private drawCommunities(input: UniverseInput): void {
    const { view } = input;
    const groups = view.communities;
    // The aggregated layer has its own distribution — a community holds the *sum* of its packages' members,
    // so reusing the package height scale would push every solid past the top of it.
    const height = heightScale(groups.map((g) => g.members));
    const footprint = footprintScale(groups.map((g) => g.functions));
    const positions = new Map<number, [number, number, number]>();
    const extent = 70 * Math.sqrt(Math.max(1, groups.length));

    groups.forEach((group, index) => {
      const radius = 60 * Math.sqrt(index + 0.6);
      const angle = index * 2.39996;
      const pos: [number, number, number] = [Math.cos(angle) * radius, 0, Math.sin(angle) * radius];
      positions.set(group.id, pos);
      const side = footprint(group.functions);
      const geo = new THREE.BoxGeometry(side, height(group.members), side);
      const mesh = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({
        color: new THREE.Color().setHSL(hueFor(communityHueIndex(group.id)) / 360, group.shared ? 0.72 : 0.08, group.shared ? 0.58 : 0.42),
        roughness: 0.55, metalness: 0.12,
      }));
      mesh.position.set(pos[0], height(group.members) / 2, pos[2]);
      this.add(mesh);
      this.coreObjects.push(mesh);
      this.pickables.push({ mesh, entry: this.communityEntry(group) });
    });

    if (input.edges) {
      // Same anchoring rule as the package layer: leave each solid part-way up, or the web hides at the
      // base of the very solids it connects.
      const lift = new Map(groups.map((g) => [g.id, Math.min(height(g.members) * 0.5, 46)]));
      const pairs = this.communityPairs(view);
      this.webSegments(pairs.flatMap(([a, b, weight]) => {
        const from = positions.get(a);
        const to = positions.get(b);
        if (!from || !to) return [];
        const segment: Segment = [
          [from[0], lift.get(a) ?? 6, from[2]],
          [to[0], lift.get(b) ?? 6, to[2]],
          weight,
        ];
        return [segment];
      }));
    }
    void extent;
  }

  /**
   * The aggregate layer's edges: the exported package edges, collapsed to community pairs.
   *
   * Read from the exported edge list rather than from `incidence`, which only the package layer fills — so
   * this layer no longer depends on the other one having been drawn first (opening the room on the
   * aggregated view drew no edges at all).
   */
  private communityPairs(view: ArchitectureView): [number, number, number][] {
    const byId = new Map(view.packages.map((p) => [p.id, p]));
    const pairs = new Map<string, number>();
    for (const edge of view.edgeList) {
      const a = byId.get(edge.source);
      const b = byId.get(edge.target);
      if (!a || !b || a.communityId === b.communityId) continue;
      const key = a.communityId < b.communityId
        ? `${a.communityId}|${b.communityId}` : `${b.communityId}|${a.communityId}`;
      pairs.set(key, (pairs.get(key) ?? 0) + edge.weight);
    }
    return [...pairs.entries()].map(([key, weight]) => {
      const [from, to] = key.split("|").map(Number);
      return [from, to, weight] as [number, number, number];
    });
  }

  /** Filled by `web()` so the aggregate layer can reuse the same edges. */
  private readonly incidence = new Map<number, { id: number; weight: number }[]>();

  /** Solid height per node, so an edge can leave a solid part-way up instead of at the floor. */
  private readonly heights = new Map<number, number>();

  /**
   * What the camera frames: the mapped structure, not the artifact band.
   *
   * The band holds the producer's buckets and the path fallbacks — on a repo whose labels all resolve to
   * absolute paths it is the *majority* of the geometry, and framing it would shrink the architecture to a
   * thumbnail in the middle of its own viewport.
   */
  private readonly coreObjects: THREE.Object3D[] = [];

  private solid(
    pkg: PackageNode, pos: [number, number, number], height: number, side: number,
  ): void {
    const isBucket = pkg.kind !== "package";
    const colour = isBucket
      ? new THREE.Color(0x6b5f7a)
      : pkg.sharesCommunity
        ? new THREE.Color().setHSL(hueFor(communityHueIndex(pkg.communityId)) / 360, 0.72, 0.58)
        : new THREE.Color(SINGLE);
    const geo = new THREE.BoxGeometry(side, height, side);
    const mesh = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({
      color: colour, roughness: 0.55, metalness: 0.12,
    }));
    mesh.position.set(pos[0], height / 2, pos[2]);
    this.add(mesh);
    this.heights.set(pkg.id, height);
    this.solids.set(pkg.id, { pos: [pos[0], 0, pos[2]], side, height, mesh });
    if (pkg.kind === "package") this.coreObjects.push(mesh);
    if (isBucket) {
      const wire = new THREE.LineSegments(
        new THREE.EdgesGeometry(geo),
        new THREE.LineBasicMaterial({ color: 0xffcf7a, transparent: true, opacity: 0.85 }),
      );
      wire.position.copy(mesh.position);
      this.add(wire);
    }
    this.pickables.push({
      mesh,
      entry: {
        kind: isBucket ? "bucket" : "package",
        id: pkg.id,
        title: pkg.label,
        lines: [
          isBucket
            ? "not a package — a producer bucket"
            : `package · ${pkg.size.toLocaleString()} members · ${pkg.functions.toLocaleString()} functions`,
          isBucket
            ? `${pkg.size.toLocaleString()} members · ${pkg.functions.toLocaleString()} functions`
            : pkg.communityLabel && pkg.communityLabel !== String(pkg.communityId)
              ? `namespace ${pkg.namespace} · community ${pkg.communityId} (${pkg.communityLabel})`
              : `namespace ${pkg.namespace} · community ${pkg.communityId}`,
          isBucket
            ? "its coupling is aggregated over unrelated files, so it is not drawn"
            : pkg.sharesCommunity ? "shares its community" : "alone in its community",
        ],
      },
    });
  }

  private communityEntry(group: CommunityGroup): HoverEntry {
    return {
      kind: "community",
      title: group.label,
      lines: [
        `community · ${group.members.toLocaleString()} members over ${group.packageCount} node${group.packageCount === 1 ? "" : "s"}`,
        group.shared ? "holds more than one node" : "holds exactly one node",
        group.containsBucket ? "includes a producer bucket, not only packages" : "packages only",
      ],
    };
  }

  private web(
    nodes: PackageNode[], positions: Map<number, [number, number, number]>, view: ArchitectureView,
  ): void {
    void view;
    this.incidence.clear();
    const segments: Segment[] = [];
    const edges = this.edgeList;
    for (const edge of edges) {
      const from = positions.get(edge.source);
      const to = positions.get(edge.target);
      if (!from || !to) continue;
      const a = this.byId.get(edge.source);
      const b = this.byId.get(edge.target);
      if (!a || !b) continue;
      const pair = this.incidence.get(edge.source) ?? [];
      pair.push({ id: edge.target, weight: edge.weight });
      this.incidence.set(edge.source, pair);
      // Leave each solid part-way up, capped: an edge at y=0 hides behind the solids it connects, and one
      // anchored to the top of a 126-unit tower sweeps the whole scene.
      const ha = Math.min((this.heights.get(edge.source) ?? 10) * 0.5, 46);
      const hb = Math.min((this.heights.get(edge.target) ?? 10) * 0.5, 46);
      segments.push([[from[0], ha, from[2]], [to[0], hb, to[2]], edge.weight]);
    }
    void nodes;
    this.webSegments(segments);
  }

  private webSegments(segments: Segment[]): void {
    if (segments.length === 0) return;
    const weights = segments.map((s) => s[2]).sort((a, b) => a - b);
    const low = Math.log(weights[0] || 1);
    const high = Math.log(weights[weights.length - 1] || 1);
    const span = Math.max(1e-6, high - low);

    const position = new Float32Array(segments.length * 6);
    const colour = new Float32Array(segments.length * 6);
    const tint = new THREE.Color();
    segments.forEach(([from, to, weight], index) => {
      const t = (Math.log(weight) - low) / span;
      position.set([from[0], from[1], from[2], to[0], to[1], to[2]], index * 6);
      // A visibility floor, stated rather than hidden: at the bottom of the weight range a `log` brightness
      // rule puts a weight-1 edge at ~2% lightness on an additive blend, which on a dark canvas is nothing.
      // The order is still log(weight) — what changed is that the weakest edge is now *visible* rather than
      // absent, and the room says the floor exists instead of claiming the web is a pure function of weight.
      tint.setHSL(0.09 + 0.03 * t, 0.85, 0.26 + 0.26 * t);
      colour.set([tint.r, tint.g, tint.b, tint.r, tint.g, tint.b], index * 6);
    });
    const geo = new THREE.BufferGeometry();
    geo.setAttribute("position", new THREE.BufferAttribute(position, 3));
    geo.setAttribute("color", new THREE.BufferAttribute(colour, 3));
    const material = new THREE.LineBasicMaterial({
      vertexColors: true, transparent: true, opacity: 0.5,
      blending: THREE.AdditiveBlending, depthWrite: false,
    });
    this.add(new THREE.LineSegments(geo, material));
  }

  /** Edges between real packages, filled by the room before `update`. */
  private edgeList: { source: number; target: number; weight: number }[] = [];
  private readonly byId = new Map<number, PackageNode>();
  /** Where each drawn solid sits — an expanded district grows from its own footprint, not the origin. */
  private readonly solids = new Map<
    number,
    { pos: [number, number, number]; side: number; height: number; mesh: THREE.Object3D }
  >();
  /** The selection halo, if a district is highlighted. */
  private outline: THREE.Group | null = null;
  /**
   * The open district, if any: its own scene, the camera to return to, and what a pick can hit.
   *
   * Distinct from "the universe with something dimmed in it" — a drill hides the universe entirely, because a
   * lower level sharing a viewport with 255 districts it is not part of is what made navigating impossible.
   */
  private drill: {
    group: THREE.Group;
    subjectId: number;
    cameraBefore: {
      position: THREE.Vector3;
      target: THREE.Vector3;
      near: number;
      far: number;
      maxDistance: number;
      fogNear: number;
      fogFar: number;
      autoRotate: boolean;
      autoRotateSpeed: number;
    };
    instanced: THREE.InstancedMesh;
    members: MemberEntry[];
    /**
     * Graph node index -> instance slot.
     *
     * The room's member list knows a member by its graph index, not by where it was drawn; the scene knows the
     * slot. Both have to be able to mark the same box, so the mapping is built once with the level rather than
     * searched for on every click.
     */
    slots: Map<number, number>;
  } | null = null;

  setEdges(edges: { source: number; target: number; weight: number }[], byId: Map<number, PackageNode>): void {
    this.edgeList = edges;
    this.byId.clear();
    for (const [id, pkg] of byId) this.byId.set(id, pkg);
  }

  /**
   * Drill into a district: **the district becomes the scene**.
   *
   * An earlier version expanded in place — members on a plinth inside the universe, everything else dimmed.
   * That is the wrong model for navigating: the camera, the orbit target and the framing all still belonged
   * to the universe, so a district with 1 500 members in it shared a viewport with 255 districts that were
   * no longer the subject. Reading a lower level needs a level change, not an overlay.
   *
   * So: the universe is hidden (not dimmed), members are laid out around the origin at their own scale, the
   * camera, clip planes and fog are solved for them (`frameLevel` — see `framing.ts` for what inheriting the
   * universe's fog range cost), and the district's *outgoing* coupling is drawn as edges leaving the cluster
   * and fading with distance — the reminder that this level sits inside something larger.
   *
   * The incoming aggregate edges are the level-0 coupling edges that touch this district; they are package-level
   * facts, not member-level ones, and the room says so.
   *
   * Members are one `InstancedMesh`, not N meshes: the largest district on the measured corpus holds 15 686
   * members, and a mesh each would be a frame-rate cliff plus a picking array that is scanned on every
   * pointer move.
   */
  expand(
    subjectId: number,
    members: Array<{ index: number; complexity: number; name?: string; nodeTypeName?: string; filePath?: string | null }>,
    edges: Array<{ source: number; target: number }>,
    options: { color?: THREE.Color; maxHeight?: number } = {},
  ): { drawn: number; edges: number; measuredHeight: boolean; boundary: number } {
    this.collapse();
    const seat = this.solids.get(subjectId);
    if (!seat || members.length === 0) return { drawn: 0, edges: 0, measuredHeight: false, boundary: 0 };

    // Stop the drift: a district is something to read, and a slowly turning model is the difference between
    // inspecting and chasing.
    const autoRotate = this.controls.autoRotate;
    const autoRotateSpeed = this.controls.autoRotateSpeed;
    this.controls.autoRotate = false;

    const cameraBefore = {
      position: this.camera.position.clone(),
      target: this.controls.target.clone(),
      near: this.camera.near,
      far: this.camera.far,
      maxDistance: this.controls.maxDistance,
      fogNear: this.fog.near,
      fogFar: this.fog.far,
      autoRotate,
      autoRotateSpeed,
    };

    // The universe steps out of the way entirely.
    this.world.visible = false;

    const group = new THREE.Group();
    const maxComplexity = Math.max(0, ...members.map((m) => m.complexity || 0));
    /**
     * Height channel: complexity when the bundle carries it, a constant when it does not.
     *
     * It does not, on the corpus this was built against — every member reports `complexity: 0` — and scaling by
     * `0 / 0` drew a field of one-unit cubes that read as a rendering bug.
     */
    const measured = maxComplexity > 0;

    // Lay the members out around the **origin**, sized for a screen of their own rather than a footprint.
    const count = members.length;
    const spread = Math.max(60, 26 * Math.sqrt(count));
    const slot = (spread * 2) / Math.sqrt(count);
    /**
     * Heights are a fraction of the *cell*, not an absolute number.
     *
     * The constant used to be 9 world units, chosen when this level was a plinth inside the universe with a
     * footprint tens of units across. Once the district became the scene, its cells grew to ~50 units and a
     * 9-unit pillar on a 2 000-unit disc is a coin seen edge-on: the level read as a flat sheet of confetti.
     */
    const tallest = measured ? slot * 2.4 : slot * 1.1;
    const geometry = new THREE.BoxGeometry(1, 1, 1);
    const material = new THREE.MeshStandardMaterial({
      color: options.color ?? new THREE.Color(0x8fd0ff),
      roughness: 0.5,
      metalness: 0.15,
    });
    const instanced = new THREE.InstancedMesh(geometry, material, count);
    instanced.instanceMatrix.setUsage(THREE.DynamicDrawUsage);

    const positions = new Map<number, THREE.Vector3>();
    const matrix = new THREE.Matrix4();
    const scale = new THREE.Vector3();
    members.forEach((member, i) => {
      // Sunflower: the room's own placement primitive, used at this level too.
      const radius = spread * Math.sqrt((i + 0.5) / count);
      const angle = i * 2.39996;
      const x = Math.cos(angle) * radius;
      const z = Math.sin(angle) * radius;
      const memberHeight = measured
        ? Math.max(slot * 0.15, (member.complexity / maxComplexity) * (options.maxHeight ?? tallest))
        : tallest;
      // 0.55, not 0.8: at 0.8 of the cell the members touch, and a district reads as one solid ring rather
      // than as the members it is made of.
      scale.set(slot * 0.55, memberHeight, slot * 0.55);
      matrix.compose(new THREE.Vector3(x, memberHeight / 2, z), new THREE.Quaternion(), scale);
      instanced.setMatrixAt(i, matrix);
      positions.set(member.index, new THREE.Vector3(x, memberHeight, z));
    });
    instanced.instanceMatrix.needsUpdate = true;
    group.add(instanced);

    // Member edges inside the district.
    const segments: number[] = [];
    for (const edge of edges) {
      const from = positions.get(edge.source);
      const to = positions.get(edge.target);
      if (!from || !to) continue;
      segments.push(from.x, from.y * 0.6, from.z, to.x, to.y * 0.6, to.z);
    }
    if (segments.length > 0) {
      const geo = new THREE.BufferGeometry();
      geo.setAttribute("position", new THREE.Float32BufferAttribute(segments, 3));
      group.add(
        new THREE.LineSegments(
          geo,
          new THREE.LineBasicMaterial({ color: 0x9fe8c8, transparent: true, opacity: 0.55 }),
        ),
      );
    }

    // Outgoing coupling: level-0 aggregate edges that touch this district, drawn outward and fading.
    const boundary: Array<{ direction: THREE.Vector3; weight: number }> = [];
    for (const edge of this.edgeList) {
      const otherId = edge.source === subjectId ? edge.target : edge.target === subjectId ? edge.source : null;
      if (otherId === null) continue;
      const other = this.solids.get(otherId);
      if (!other) continue;
      const direction = new THREE.Vector3(
        other.pos[0] - seat.pos[0],
        0,
        other.pos[2] - seat.pos[2],
      );
      if (direction.lengthSq() < 1e-6) continue;
      boundary.push({ direction: direction.normalize(), weight: edge.weight });
    }
    /**
     * The coupling that leaves the level, drawn thin-but-thick enough to stop flickering.
     *
     * `LineBasicMaterial.linewidth` cannot do this: WebGL core profile ignores it and every line is one device
     * pixel, so at the distance this level is framed from these segments land on sub-pixel positions and blink as
     * the camera drifts. `Line2` draws them as screen-space quads of a real width, which is the fix — and the fade
     * that used to need three separate materials (0.5, 0.26, 0.1) is now a per-vertex colour on one geometry, so
     * the whole set is one draw call per level instead of three per edge.
     */
    if (boundary.length > 0) {
      const positions: number[] = [];
      const colours: number[] = [];
      const tint = new THREE.Color(0xffd89a);
      // Three steps out into the void, each fading: the web continues past this level and thins out.
      const steps = [1, 1.6, 2.4];
      const fades = [1, 0.55, 0.3];
      for (const { direction } of boundary) {
        for (let i = 0; i < steps.length - 1; i += 1) {
          const a = direction.clone().multiplyScalar(spread * steps[i]);
          const b = direction.clone().multiplyScalar(spread * steps[i + 1]);
          positions.push(a.x, 2, a.z, b.x, 2, b.z);
          // Vertex colours carry the fade, so one material covers every segment.
          colours.push(tint.r, tint.g, tint.b, tint.r * fades[i + 1], tint.g * fades[i + 1], tint.b * fades[i + 1]);
        }
      }
      const geometry = new LineSegmentsGeometry();
      geometry.setPositions(positions);
      geometry.setColors(colours);
      const material = new LineMaterial({
        color: 0xffffff,
        vertexColors: true,
        linewidth: LEVEL_EDGE_WIDTH_PX,
        transparent: true,
        opacity: 0.55,
        depthWrite: false,
        worldUnits: false,
      });
      // Screen-space widths need the viewport, or every line is the wrong thickness after a resize.
      material.resolution.set(this.size.x, this.size.y);
      this.levelLines = material;
      group.add(new LineSegments2(geometry, material));
    }

    this.scene.add(group);
    const slots = new Map<number, number>();
    members.forEach((m, instance) => slots.set(m.index, instance));
    this.drill = { group, subjectId, cameraBefore, instanced, slots, members: members.map((m, instance) => ({
      index: m.index,
      name: m.name ?? `node ${m.index}`,
      nodeTypeName: m.nodeTypeName ?? "node",
      filePath: m.filePath ?? null,
      // Its slot in the InstancedMesh, so a mark can address it from anywhere — a scene click and a row in
      // the level's list both end up here.
      instance,
    })) };

    // Aim at the members' floor, and take the height into the radius — the same rule the universe frames by, so a
    // level change does not move the reading point. Centring the middle of the bounding box instead put the field
    // a quarter of the way down the panel; see `frameWorld`.
    this.frameSphere(new THREE.Vector3(0, 0, 0), Math.hypot(spread + slot, tallest), 1.45);
    return {
      drawn: count,
      edges: segments.length / 6,
      measuredHeight: measured,
      boundary: boundary.length,
    };
  }

  /**
   * Put the level back exactly as it was: universe visible, camera, clip planes and fog where the reader left
   * them, and the drift setting they had chosen.
   *
   * `cameraBefore` carries the fog as well as the camera, because the level changed both. Restoring only the
   * camera would have left the universe drawn through a fog range solved for a district — the exact failure
   * that made an open district look empty, in reverse.
   */
  collapse(): void {
    if (!this.drill) return;
    const { group, cameraBefore } = this.drill;
    this.scene.remove(group);
    group.traverse((child) => {
      const mesh = child as THREE.Mesh;
      mesh.geometry?.dispose();
      const m = mesh.material as THREE.Material | THREE.Material[] | undefined;
      if (Array.isArray(m)) m.forEach((x) => x.dispose());
      else m?.dispose();
    });
    this.world.visible = true;
    this.camera.position.copy(cameraBefore.position);
    this.controls.target.copy(cameraBefore.target);
    this.camera.near = cameraBefore.near;
    this.camera.far = cameraBefore.far;
    this.controls.maxDistance = cameraBefore.maxDistance;
    this.fog.near = cameraBefore.fogNear;
    this.fog.far = cameraBefore.fogFar;
    this.camera.updateProjectionMatrix();
    this.controls.update();
    // The level's line material dies with the level's group; drop the reference so a later resize cannot write a
    // viewport into a disposed material.
    this.levelLines = null;
    this.controls.autoRotate = cameraBefore.autoRotate;
    this.controls.autoRotateSpeed = cameraBefore.autoRotateSpeed;
    this.drill = null;
  }

  /** Whether a district is open — the room labels its own control from this. */
  get isExpanded(): boolean {
    return this.drill !== null;
  }

  /**
   * Emphasise one solid and quiet the rest.
   *
   * Selecting a district has to be *visible*: moving the camera is not enough when the target is one box among
   * 256, and a row picked from a list gives no sense of which box it became. The outline is the emphasis; the
   * slight dimming is what makes the outline readable against its neighbours.
   *
   * What gets dimmed is remembered per material and restored from that record. Setting every material back to
   * `opacity = 1` on deselect looked equivalent and was not: the namespace shells are drawn at 0.05, so
   * deselecting turned every volume into an opaque pale-blue wall across the map.
   */
  highlightSolid(id: number | null): void {
    this.disposeOutline();
    // The rest of the map recedes hard — 0.18 of its own opacity, not 0.55. At 0.55 a selected district sat in a
    // field of near-equals and the outline had to carry the whole claim; the point of dimming is that the eye
    // lands on one box without having to be told which.
    for (const object of this.coreObjects) {
      const material = (object as THREE.Mesh).material as THREE.Material | undefined;
      if (!material || Array.isArray(material)) continue;
      const m = material as THREE.Material & { opacity: number; emissive?: THREE.Color; emissiveIntensity?: number };
      const own = this.remember(m);
      m.opacity = id === null ? own.opacity : own.opacity * 0.18;
      m.transparent = own.transparent || id !== null;
      if (m.emissive) {
        m.emissive.setHex(own.emissive);
        m.emissiveIntensity = own.emissiveIntensity;
      }
    }
    if (id === null || this.drill) return;
    const box = this.boxFor(id);
    if (!box) return;
    const mesh = this.objectFor(id) as THREE.Mesh;
    const material = mesh.material as
      | (THREE.Material & { opacity: number; emissive?: THREE.Color; emissiveIntensity?: number })
      | undefined;
    if (material) {
      const own = this.remember(material);
      material.opacity = own.opacity;
      material.transparent = own.transparent;
      // A lit interior, not only an outline: the box itself brightens, which is what reads as a glow at the
      // distance this room frames from. No bloom pass — one emissive material costs nothing.
      if (material.emissive) {
        material.emissive.setHex(0x2ea8ff);
        material.emissiveIntensity = 0.85;
      }
    }
    const halo = this.glowHalo(box, 0x8fd0ff);
    this.world.add(halo);
    this.outline = halo;
  }

  /**
   * A border glow, without a bloom pass.
   *
   * Three nested edge boxes at growing offsets and falling opacity, additive and `depthTest: false`, so the
   * halo reads as light escaping the silhouette rather than as a wireframe sitting on it. One `LineSegments`
   * with a fat shader would be the other way; this needs no custom material and no post-processing.
   */
  private glowHalo(box: THREE.Box3, colour: number): THREE.Group {
    const group = new THREE.Group();
    const size = box.getSize(new THREE.Vector3());
    const centre = box.getCenter(new THREE.Vector3());
    // Offsets scale with the object: a fixed pad is invisible on a 100-unit district and overwhelming on a 6.
    const unit = Math.max(3, Math.max(size.x, size.z) * 0.045);
    for (const [step, opacity] of [[0, 1], [1, 0.45], [2.2, 0.18]] as Array<[number, number]>) {
      const geometry = new THREE.EdgesGeometry(
        new THREE.BoxGeometry(size.x + unit * step, size.y + unit * step, size.z + unit * step),
      );
      const lines = new THREE.LineSegments(
        geometry,
        new THREE.LineBasicMaterial({
          color: colour,
          transparent: true,
          opacity,
          blending: THREE.AdditiveBlending,
          depthTest: false,
          depthWrite: false,
        }),
      );
      lines.position.copy(centre);
      lines.renderOrder = 2;
      group.add(lines);
    }
    return group;
  }

  private disposeOutline(): void {
    if (!this.outline) return;
    this.outline.parent?.remove(this.outline);
    this.outline.traverse((child) => {
      const line = child as THREE.LineSegments;
      line.geometry?.dispose();
      const m = line.material as THREE.Material | undefined;
      m?.dispose();
    });
    this.outline = null;
  }

  /**
   * Mark one member of the open district.
   *
   * A click inside a level asks "what is this?" — so it needs an answer on the surface, not just in a panel
   * somewhere else. The mark is the same glow the universe uses for a selected district, so the two levels read
   * alike.
   */
  highlightMember(member: MemberEntry | null): void {
    if (this.memberOutline) {
      this.memberOutline.parent?.remove(this.memberOutline);
      this.memberOutline.traverse((child) => {
        const line = child as THREE.LineSegments;
        line.geometry?.dispose();
        (line.material as THREE.Material | undefined)?.dispose();
      });
      this.memberOutline = null;
    }
    if (!member || !this.drill) return;
    const slot = member.instance ?? this.drill.slots.get(member.index);
    if (slot === undefined) return;
    const matrix = new THREE.Matrix4();
    this.drill.instanced.getMatrixAt(slot, matrix);
    const centre = new THREE.Vector3();
    const scale = new THREE.Vector3();
    matrix.decompose(centre, new THREE.Quaternion(), scale);
    const box = new THREE.Box3().setFromCenterAndSize(centre, scale);
    const halo = this.glowHalo(box, 0xffcf7a);
    this.drill.group.add(halo);
    this.memberOutline = halo;
  }

  private memberOutline: THREE.Group | null = null;
  /** The level's outgoing coupling material, kept so a resize can tell it the new viewport. */
  private levelLines: LineMaterial | null = null;
  /** Last known drawing-buffer size, in pixels. */
  private readonly size = new THREE.Vector2(1, 1);

  /**
   * Each dimmed material's own look, so a deselect restores it instead of flattening it to opaque.
   *
   * Emissive belongs here for the same reason opacity does: the glow writes to the same material it dimmed.
   */
  private readonly remembered = new Map<
    THREE.Material,
    { opacity: number; transparent: boolean; emissive: number; emissiveIntensity: number }
  >();

  private remember(material: THREE.Material): {
    opacity: number;
    transparent: boolean;
    emissive: number;
    emissiveIntensity: number;
  } {
    const existing = this.remembered.get(material);
    if (existing) return existing;
    const lit = material as THREE.Material & { emissive?: THREE.Color; emissiveIntensity?: number };
    const own = {
      opacity: (material as THREE.Material & { opacity: number }).opacity,
      transparent: material.transparent,
      emissive: lit.emissive?.getHex() ?? 0,
      emissiveIntensity: lit.emissiveIntensity ?? 1,
    };
    this.remembered.set(material, own);
    return own;
  }

  /**
   * The drawn object behind an id: a package solid, a bucket, or a namespace volume.
   *
   * Not just `solids`, which only the package layer fills: a namespace shell is a pickable with an id of its
   * own, and clicking one has to be as answerable as clicking a package.
   */
  private objectFor(id: number): THREE.Object3D | null {
    return this.solids.get(id)?.mesh ?? this.pickables.find((p) => p.entry.id === id)?.mesh ?? null;
  }

  /** Its world-space box, or `null` when this id is not drawn in the current layer. */
  private boxFor(id: number): THREE.Box3 | null {
    const object = this.objectFor(id);
    if (!object) return null;
    const box = new THREE.Box3().setFromObject(object);
    return box.isEmpty() ? null : box;
  }

  /**
   * Select something from *anywhere*: mark it, then go to it.
   *
   * The lists in this room name packages, and naming a place is asking to be taken there. Highlight without a
   * move left the reader with a marked row in a list and no idea where the box it names sits among 256 of them.
   * The standoff is derived from the thing's own footprint, because "close enough to read it" is a property of
   * what is being approached and not of the camera.
   */
  focusSolid(id: number | null): void {
    this.cancelPendingFocus();
    this.highlightSolid(id);
    this.flyToSolid(id);
  }

  /** Mark now, fly in a beat — the shape of a press that might be half of a double-click. */
  private focusSoon(id: number | null): void {
    this.cancelPendingFocus();
    this.highlightSolid(id);
    if (id === null) return;
    this.flyTimer = globalThis.setTimeout(() => {
      this.flyTimer = 0;
      this.flyToSolid(id);
    }, DOUBLE_CLICK_WINDOW_MS);
  }

  private cancelPendingFocus(): void {
    if (!this.flyTimer) return;
    globalThis.clearTimeout(this.flyTimer);
    this.flyTimer = 0;
  }

  private flyTimer = 0;

  private flyToSolid(id: number | null): void {
    if (id === null) return;
    const box = this.boxFor(id);
    if (!box) return;
    const size = box.getSize(new THREE.Vector3());
    const centre = box.getCenter(new THREE.Vector3());
    const reach = Math.max(size.x, size.z);
    this.flyTo(centre, Math.min(1400, Math.max(180, reach * 3.4)));
  }

  /** Drift on or off. Explicit, because "it stopped moving" and "it is broken" look the same. */
  setDrift(on: boolean): void {
    this.controls.autoRotate = on;
  }

  /**
   * Fly to a target, keeping the angle the reader chose.
   *
   * The distance is the caller's, because "close enough to read it" is a property of the thing being approached
   * and not of the camera: a district 100 units across and one 6 units across do not want the same standoff.
   */
  private flyTo(target: THREE.Vector3, distance: number): void {
    const camera = this.camera;
    const dir = new THREE.Vector3().subVectors(camera.position, this.controls.target).normalize();
    const goal = target.clone().addScaledVector(dir, distance);
    if (this.reducedMotion) {
      camera.position.copy(goal);
      this.controls.target.copy(target);
      this.controls.update();
      return;
    }
    const fromPosition = camera.position.clone();
    const fromTarget = this.controls.target.clone();
    const started = performance.now();
    const step = () => {
      const t = Math.min(1, (performance.now() - started) / 420);
      const eased = 1 - Math.pow(1 - t, 3);
      camera.position.lerpVectors(fromPosition, goal, eased);
      this.controls.target.lerpVectors(fromTarget, target, eased);
      this.controls.update();
      if (t < 1 && this.mounted) requestAnimationFrame(step);
    };
    step();
  }

  /**
   * What the pointer is actually over.
   *
   * `intersectObjects(...)[0]` is the *nearest* hit, and a namespace volume is a box that contains the solids drawn
   * inside it — so its near face is always nearer than the package you are aiming at, and every package inside a
   * volume was unclickable while the shell around it took the press. Preferring the nearest solid and falling back
   * to the shell only when nothing else is hit is the difference between "the volume is pickable" and "the volume
   * is a wall".
   */
  private pickAt(): HoverEntry | null {
    const hits = this.raycaster.intersectObjects(this.pickables.map((p) => p.mesh), false);
    const entries: HoverEntry[] = [];
    for (const hit of hits) {
      const found = this.pickables.find((p) => p.mesh === hit.object);
      if (found) entries.push(found.entry);
    }
    return nearestSolid(entries);
  }

  private add(object: THREE.Object3D): void {
    this.world.add(object);
    object.traverse((child: THREE.Object3D) => {
      const geometry = (child as THREE.Mesh).geometry;
      if (geometry) this.disposables.push(geometry);
      const material = (child as THREE.Mesh).material;
      if (Array.isArray(material)) this.disposables.push(...material);
      else if (material) this.disposables.push(material);
    });
  }

  private clear(): void {
    this.collapse();
    this.solids.clear();
    for (const child of [...this.world.children]) this.world.remove(child);
    for (const item of this.disposables) item.dispose();
    this.disposables.length = 0;
    this.pickables.length = 0;
    this.incidence.clear();
    this.heights.clear();
    this.coreObjects.length = 0;
  }

  /**
   * Frame what was actually built.
   *
   * This used to frame a disc of radius `core` centred on the origin — an assumption about the geometry
   * rather than a measurement of it. It was wrong in two ways at once: the aggregated layer's solids are
   * as tall as their *summed* members, which is far outside the height scale the packages were fitted to,
   * so the model grew upward past the assumption; and a camera placed on an un-normalised direction vector
   * sat 1.19x further out than the solve said. The visible result was a map parked in the lower-right of
   * the viewport.
   *
   * `Box3.setFromObject` costs one traversal of ~260 meshes and is immune to both.
   */
  /** Frame an arbitrary object set. Same measurement rule as `frameWorld`, so both levels frame alike. */
  /**
   * Point the camera at a sphere of a given radius, and take the fog, near and far planes with it.
   *
   * Not derived from a `Box3` of the level's scene graph: the objects at a level are not all of the level. A
   * district's coupling lines deliberately run out past its members, and framing those would have put the
   * camera past the fog's far plane — a scene graph that measures bigger than the thing being read.
   */
  private frameSphere(centre: THREE.Vector3, radius: number, margin = 1.45): void {
    const framing = frameLevel(Math.max(radius, 1), this.camera.fov, margin);
    this.controls.target.copy(centre);
    // OrbitControls clamps maxDistance silently, so the ceiling has to admit this framing
    this.controls.maxDistance = Math.max(9000, radius * 10);
    const direction = new THREE.Vector3(0.55, 0.62, 0.75).normalize();
    this.camera.position.copy(centre).addScaledVector(direction, framing.distance);
    this.camera.near = framing.near;
    this.camera.far = framing.far;
    this.camera.updateProjectionMatrix();
    this.fog.near = framing.fogNear;
    this.fog.far = framing.fogFar;
    this.controls.update();
  }

  /**
   * Aim at the **floor** of the map, not at the middle of its bounding box.
   *
   * `Box3.getCenter()` is a fact about the box; it is not where the thing is. This corpus makes the difference
   * large and obvious: the tallest solid is 150 units and the disc's radius is ~130, so the box centre sits in
   * the air above the city — and framing it there left the disc's own centre at NDC −0.23, a quarter of the way
   * down the panel, with the whole map hugging the bottom edge. Centring the box is not centring the map.
   *
   * So the aim is the centre of the disc at the height the solids stand on, and the fit radius is measured *from
   * that aim*: a sphere centred there still contains every corner, so nothing clips off the top even though the
   * aim moved down.
   *
   * Why measurement rather than an offset: this method has now been wrong twice — once by assuming the geometry
   * was a disc of a computed radius, once by assuming its box centre was its middle. Both fixes are the same
   * rule, so it is worth stating: derive the camera from the box, and put the *centre of interest* in the middle
   * of the panel — for a city, that is the floor.
   */
  private frameWorld(): void {
    const box = new THREE.Box3();
    for (const object of this.coreObjects) box.expandByObject(object);
    if (box.isEmpty()) box.setFromObject(this.world);
    if (box.isEmpty()) return;

    const centre = box.getCenter(new THREE.Vector3());
    const aim = new THREE.Vector3(centre.x, box.min.y, centre.z);

    // A tight bound, not the box's diagonal: a wide, flat model's diagonal is up to sqrt(3) longer than the
    // geometry it holds, which framed this disc at about two-thirds the panel height and read as an empty screen.
    // Each corner's own distance from the aim is the real extent.
    let radius = 0;
    for (const x of [box.min.x, box.max.x]) {
      for (const y of [box.min.y, box.max.y]) {
        for (const z of [box.min.z, box.max.z]) {
          radius = Math.max(radius, new THREE.Vector3(x, y, z).distanceTo(aim));
        }
      }
    }
    radius = Math.max(radius, 1);

    this.controls.target.copy(aim);
    // OrbitControls clamps maxDistance silently, so the ceiling has to admit this framing
    this.controls.maxDistance = Math.max(9000, radius * 10);

    // 1.25, not 1.0: a flat disc seen from above needs room toward its near edge, and a sphere fit does not know
    // about the tilt — at 1.02 the near rim touched the bottom of the panel.
    const fov = (this.camera.fov * Math.PI) / 180;
    const distance = (radius / Math.sin(fov / 2)) * 1.25;
    const direction = new THREE.Vector3(0.55, 0.62, 0.75).normalize();
    this.camera.position.copy(aim).addScaledVector(direction, distance);
    this.camera.near = Math.max(1, radius / 500);
    this.camera.far = Math.max(90000, radius * 40);
    this.camera.updateProjectionMatrix();
    this.fog.near = UNIVERSE_FOG[0];
    this.fog.far = UNIVERSE_FOG[1];
    this.controls.update();
  }

  attachPointer(target: HTMLElement): void {
    /** What the last press hit. A double-click reports *this*, not a fresh ray — see `dblclick` below. */
    let pressedMember: MemberEntry | null = null;
    let pressedEntry: HoverEntry | null = null;

    const move = (event: PointerEvent) => {
      const rect = target.getBoundingClientRect();
      this.pointer.set(
        ((event.clientX - rect.left) / rect.width) * 2 - 1,
        -((event.clientY - rect.top) / rect.height) * 2 + 1,
      );
      this.raycaster.setFromCamera(this.pointer, this.camera);
      const member = this.memberAt();
      if (member) {
        this.onHover({
          kind: "package",
          title: member.entry.name,
          lines: [
            `${member.entry.nodeTypeName}${member.entry.filePath ? ` · ${member.entry.filePath}` : ""}`,
            "click for its name · double-click to open it in Understand",
          ],
        });
        return;
      }
      // Inside a level, the only things that exist for the pointer are the members. `world.visible = false` does
      // not remove the universe from the ray: a press on empty space inside a district would otherwise hit one of
      // the 256 hidden solids, select it, and fly the camera out of the level to a place it cannot show.
      if (this.drill) {
        this.onHover(null);
        return;
      }
      this.onHover(this.pickAt());
    };
    const leave = () => this.onHover(null);
    const click = () => {
      this.raycaster.setFromCamera(this.pointer, this.camera);
      // Before anything else: a press means "I am reading this", so the drift stops.
      this.onPress();
      // Inside an open district, a click means "this member", not "the package behind it".
      const member = this.memberAt();
      if (member) {
        pressedMember = member.entry;
        pressedEntry = null;
        this.onMemberPick(member.entry);
        return;
      }
      pressedMember = null;
      // Cleared before the level check too, or a press on empty space inside a district would leave the
      // previous press's universe solid armed for the next double-click.
      pressedEntry = null;
      this.onMemberPick(null);
      if (this.drill) return;
      pressedEntry = this.pickAt();
      // Report, and mark. The flight waits one double-click window: a press that is half of a double-click
      // must not move the camera out from under the second press, which is what made "double-click to expand"
      // land on a neighbour or on empty space.
      this.onPick(pressedEntry);
      this.focusSoon(pressedEntry && pressedEntry.id !== undefined ? pressedEntry.id : null);
    };
    /**
     * A double-click acts on what the press hit, not on a fresh ray.
     *
     * A press now also *flies the camera* to what it hit, so by the time `dblclick` fires the camera is a third
     * of the way through a 420 ms move and a fresh ray lands somewhere else — on a neighbour, or on nothing at
     * all, which read as "double-click does not expand". The press already answered the question.
     */
    const dblclick = () => {
      this.cancelPendingFocus();
      if (pressedMember) {
        this.onMemberOpenRequest(pressedMember);
        return;
      }
      if (pressedEntry) this.onExplodeRequest(pressedEntry);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerleave", leave);
    target.addEventListener("pointerdown", click);
    target.addEventListener("dblclick", dblclick);
  }

  private loop = (): void => {
    if (!this.mounted) return;
    this.frame = requestAnimationFrame(this.loop);
    this.controls.update();
    this.renderer.render(this.scene, this.camera);
  };

  dispose(): void {
    this.mounted = false;
    this.cancelPendingFocus();
    cancelAnimationFrame(this.frame);
    this.clear();
    this.controls.dispose();
    this.renderer.dispose();
    this.renderer.domElement.remove();
  }
}

/** Whether this browser can give the canvas a context — checked before constructing the scene. */
export function webglAvailable(): boolean {
  try {
    const probe = document.createElement("canvas");
    return Boolean(
      probe.getContext("webgl2") ?? probe.getContext("webgl") ?? probe.getContext("experimental-webgl"),
    );
  } catch {
    return false;
  }
}
