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
}

export interface HoverEntry {
  kind: "package" | "volume" | "bucket" | "community";
  title: string;
  lines: string[];
  /** Metanode / aggregate id, when one exists — the handle an expand is addressed by. */
  id?: number;
}

const SINGLE = 0x67788c; // a package alone in its community — a finding, not a failure

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

  /** A click on a member. This is the join between the universe and the rest of the dashboard. */
  onMemberPickChange(fn: (member: MemberEntry | null) => void): void {
    this.onMemberPick = fn;
  }

  /**
   * Which member a ray hit, if any.
   *
   * Members are one `InstancedMesh`, so a hit reports an `instanceId` rather than an object: the mapping back
   * to a node is the array order the instances were built in, which is why it is stored beside the mesh
   * instead of being re-derived here.
   */
  private memberAt(): MemberEntry | null {
    if (!this.drill) return null;
    const hit = this.raycaster.intersectObject(this.drill.instanced, false)[0];
    if (!hit || hit.instanceId === undefined) return null;
    return this.drill.members[hit.instanceId] ?? null;
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
  private outline: THREE.LineSegments | null = null;
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
    const perEdge: Array<[number, number]> = [];
    for (const { direction, weight } of boundary) {
      // Three segments each, dimmer with distance: the web continues past this level and thins out.
      const steps: Array<[number, number, number]> = [
        [spread, 0.5, 1.0],
        [1.6, 0.26, 0.55],
        [2.4, 0.1, 0.3],
      ];
      let previous = spread;
      for (const [factor, opacity, taper] of steps) {
        const next = spread * factor;
        const a = direction.clone().multiplyScalar(previous);
        const b = direction.clone().multiplyScalar(next);
        const geo = new THREE.BufferGeometry();
        geo.setAttribute(
          "position",
          new THREE.Float32BufferAttribute([a.x, 2, a.z, b.x, 2, b.z], 3),
        );
        group.add(
          new THREE.LineSegments(
            geo,
            new THREE.LineBasicMaterial({
              color: 0xffd89a,
              transparent: true,
              opacity,
            }),
          ),
        );
        perEdge.push([weight, taper]);
        previous = next;
      }
    }

    this.scene.add(group);
    this.drill = { group, subjectId, cameraBefore, instanced, members: members.map((m) => ({
      index: m.index,
      name: m.name ?? `node ${m.index}`,
      nodeTypeName: m.nodeTypeName ?? "node",
      filePath: m.filePath ?? null,
    })) };

    this.frameSphere(new THREE.Vector3(0, tallest / 2, 0), Math.hypot(spread + slot, tallest / 2), 1.45);
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
    if (this.outline) {
      this.world.remove(this.outline);
      this.outline.traverse((child) => {
        const line = child as THREE.LineSegments;
        line.geometry?.dispose();
        const m = line.material as THREE.Material | undefined;
        m?.dispose();
      });
      this.outline = null;
    }
    for (const object of this.coreObjects) {
      const material = (object as THREE.Mesh).material as THREE.Material | undefined;
      if (!material || Array.isArray(material)) continue;
      const m = material as THREE.Material & { opacity: number };
      let own = this.remembered.get(m);
      if (!own) {
        own = { opacity: m.opacity, transparent: m.transparent };
        this.remembered.set(m, own);
      }
      m.opacity = id === null ? own.opacity : own.opacity * 0.55;
      m.transparent = own.transparent || id !== null;
    }
    if (id === null || this.drill) return;
    const box = this.boxFor(id);
    if (!box) return;
    const material = (this.objectFor(id) as THREE.Mesh).material as
      | (THREE.Material & { opacity: number })
      | undefined;
    const own = material ? this.remembered.get(material) : undefined;
    if (material) material.opacity = own?.opacity ?? 1;
    const size = box.getSize(new THREE.Vector3());
    const centre = box.getCenter(new THREE.Vector3());
    // A fixed +10 pad is invisible on a 100-unit solid and overwhelming on a 6-unit one.
    const pad = Math.max(6, Math.max(size.x, size.z) * 0.22);
    const halo = new THREE.LineSegments(
      new THREE.EdgesGeometry(new THREE.BoxGeometry(size.x + pad, size.y + pad, size.z + pad)),
      new THREE.LineBasicMaterial({ color: 0x8fd0ff }),
    );
    halo.position.copy(centre);
    this.world.add(halo);
    this.outline = halo;
  }

  /** Each dimmed material's own look, so a deselect restores it instead of flattening it to opaque. */
  private readonly remembered = new Map<THREE.Material, { opacity: number; transparent: boolean }>();

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
    this.highlightSolid(id);
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

  private frameWorld(): void {
    const box = new THREE.Box3();
    for (const object of this.coreObjects) box.expandByObject(object);
    if (box.isEmpty()) box.setFromObject(this.world);
    if (box.isEmpty()) return;
    // A tight bound, not the box's circumscribed sphere: the diagonal of a wide, flat box is up to sqrt(3)
    // longer than the geometry it holds, which framed this disc at about two-thirds of the panel height and
    // read as an empty screen. Each object's own sphere, measured from the box centre, is the real extent.
    const centre = box.getCenter(new THREE.Vector3());
    let radius = 0;
    for (const object of this.coreObjects) {
      const own = new THREE.Box3().setFromObject(object).getBoundingSphere(new THREE.Sphere());
      if (Number.isFinite(own.radius)) radius = Math.max(radius, own.center.distanceTo(centre) + own.radius);
    }
    radius = Math.max(radius, 1);

    this.controls.target.copy(centre);
    // OrbitControls clamps maxDistance silently, so the ceiling has to admit this framing
    this.controls.maxDistance = Math.max(9000, radius * 10);

    // 1.25, not 1.0: a flat disc seen from above needs room toward its near edge, and a sphere fit does not
    // know about the tilt — at 1.02 the near rim touched the bottom of the panel.
    const fov = (this.camera.fov * Math.PI) / 180;
    const distance = (radius / Math.sin(fov / 2)) * 1.25;
    const direction = new THREE.Vector3(0.55, 0.62, 0.75).normalize();
    this.camera.position.copy(centre).addScaledVector(direction, distance);
    this.camera.near = Math.max(1, radius / 500);
    this.camera.far = Math.max(90000, radius * 40);
    this.camera.updateProjectionMatrix();
    this.fog.near = UNIVERSE_FOG[0];
    this.fog.far = UNIVERSE_FOG[1];
    this.controls.update();
  }

  attachPointer(target: HTMLElement): void {
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
          title: member.name,
          lines: [
            `${member.nodeTypeName}${member.filePath ? ` · ${member.filePath}` : ""}`,
            "a member of the open district — click to open it in Understand",
          ],
        });
        return;
      }
      const hit = this.raycaster.intersectObjects(this.pickables.map((p) => p.mesh), false)[0];
      const found = hit ? this.pickables.find((p) => p.mesh === hit.object) : undefined;
      this.onHover(found?.entry ?? null);
    };
    const leave = () => this.onHover(null);
    const click = () => {
      this.raycaster.setFromCamera(this.pointer, this.camera);
      // Inside an open district, a click means "this member", not "the package behind it".
      const member = this.memberAt();
      if (member) {
        this.onMemberPick(member);
        return;
      }
      this.onMemberPick(null);
      const hit = this.raycaster.intersectObjects(this.pickables.map((p) => p.mesh), false)[0];
      const found = hit ? this.pickables.find((p) => p.mesh === hit.object) : undefined;
      // Report only. The room answers by calling `focusSolid`, so a click in the scene and a row in a list
      // take the same route and cannot drift apart — the click handler used to fly on its own while the lists
      // called a method that did not exist.
      this.onPick(found?.entry ?? null);
    };
    const dblclick = () => {
      this.raycaster.setFromCamera(this.pointer, this.camera);
      const hit = this.raycaster.intersectObjects(this.pickables.map((p) => p.mesh), false)[0];
      const found = hit ? this.pickables.find((p) => p.mesh === hit.object) : undefined;
      this.onExplodeRequest(found?.entry ?? null);
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
