/**
 * Three.js scene for the Migration room: community spheres by order, edges as lines.
 * Layout comes from `migrationLayout` (pure); this class only draws and picks.
 */
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";
import type { MigrationLayout, MigrationLayoutNode } from "../migrationLayout";
import { meshRadius, orderHue } from "../migrationLayout";

export type MigrationPick = MigrationLayoutNode | null;

/** Pure flag resolution — unit-tested without constructing WebGL. */
export function resolveReducedMotion(opts?: { reducedMotion?: boolean }): boolean {
  if (opts?.reducedMotion !== undefined) return opts.reducedMotion;
  return typeof matchMedia !== "undefined" && matchMedia("(prefers-reduced-motion: reduce)").matches;
}

export class MigrationScene {
  readonly renderer: THREE.WebGLRenderer;
  readonly camera: THREE.PerspectiveCamera;
  readonly reducedMotion: boolean;
  private readonly scene = new THREE.Scene();
  private readonly controls: OrbitControls;
  private readonly root = new THREE.Group();
  private readonly pickables: { mesh: THREE.Object3D; node: MigrationLayoutNode }[] = [];
  private readonly raycaster = new THREE.Raycaster();
  private readonly pointer = new THREE.Vector2();
  private readonly disposables: { dispose(): void }[] = [];
  private frame = 0;
  private mounted = false;
  private onHover: (n: MigrationPick) => void = () => {};
  private onPick: (n: MigrationPick) => void = () => {};

  /**
   * @param opts.reducedMotion — when true, disables control damping (no animated camera settle).
   *   Auto-rotate is never enabled. Defaults from `prefers-reduced-motion: reduce`.
   */
  constructor(host: HTMLElement, opts?: { reducedMotion?: boolean }) {
    this.reducedMotion = resolveReducedMotion(opts);

    this.scene.background = new THREE.Color(0x060a12);

    this.renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio, 2));
    const w = Math.max(host.clientWidth, 1);
    const h = Math.max(host.clientHeight, 1);
    this.renderer.setSize(w, h);
    this.renderer.domElement.style.display = "block";
    this.renderer.domElement.style.width = "100%";
    this.renderer.domElement.style.height = "100%";
    host.appendChild(this.renderer.domElement);

    this.camera = new THREE.PerspectiveCamera(50, w / h, 0.5, 5000);
    this.camera.position.set(80, 70, 110);

    this.controls = new OrbitControls(this.camera, this.renderer.domElement);
    this.controls.enableDamping = !this.reducedMotion;
    this.controls.dampingFactor = 0.08;
    this.controls.autoRotate = false;

    this.scene.add(new THREE.AmbientLight(0xffffff, 0.65));
    const key = new THREE.DirectionalLight(0xffffff, 0.95);
    key.position.set(60, 90, 40);
    this.scene.add(key);
    const fill = new THREE.DirectionalLight(0x88aacc, 0.35);
    fill.position.set(-40, 20, -30);
    this.scene.add(fill);
    this.scene.add(this.root);
  }

  setCallbacks(hover: (n: MigrationPick) => void, pick: (n: MigrationPick) => void): void {
    this.onHover = hover;
    this.onPick = pick;
  }

  resize(width: number, height: number): void {
    if (width < 1 || height < 1) return;
    this.camera.aspect = width / height;
    this.camera.updateProjectionMatrix();
    this.renderer.setSize(width, height);
  }

  setLayout(layout: MigrationLayout): void {
    this.clear();
    const posById = new Map<number, THREE.Vector3>();

    for (const node of layout.nodes) {
      const hue = orderHue(node.orderNorm);
      const color = new THREE.Color().setHSL(hue / 360, 0.62, 0.55);
      const radius = meshRadius(node.sizeNorm);
      const geo = new THREE.SphereGeometry(radius, 20, 16);
      const mat = new THREE.MeshStandardMaterial({
        color,
        roughness: 0.4,
        metalness: 0.12,
        emissive: color.clone().multiplyScalar(0.18),
      });
      const mesh = new THREE.Mesh(geo, mat);
      mesh.position.set(...node.position);
      mesh.userData.communityId = node.communityId;
      this.root.add(mesh);
      this.pickables.push({ mesh, node });
      this.disposables.push(geo, mat);
      posById.set(node.communityId, mesh.position.clone());
    }

    if (layout.edges.length > 0) {
      const maxW = Math.max(...layout.edges.map((e) => e.weight), 1);
      const positions: number[] = [];
      const colors: number[] = [];
      const tmp = new THREE.Color();
      for (const e of layout.edges) {
        const a = posById.get(e.source);
        const b = posById.get(e.target);
        if (!a || !b) continue;
        positions.push(a.x, a.y, a.z, b.x, b.y, b.z);
        // log brightness like Architecture — weak edges stay visible but dim
        const t = Math.log1p(e.weight) / Math.log1p(maxW);
        tmp.setHSL(0.58, 0.15, 0.25 + t * 0.45);
        colors.push(tmp.r, tmp.g, tmp.b, tmp.r, tmp.g, tmp.b);
      }
      if (positions.length > 0) {
        const geo = new THREE.BufferGeometry();
        geo.setAttribute("position", new THREE.Float32BufferAttribute(positions, 3));
        geo.setAttribute("color", new THREE.Float32BufferAttribute(colors, 3));
        const mat = new THREE.LineBasicMaterial({
          vertexColors: true,
          transparent: true,
          opacity: 0.28,
          depthWrite: false,
        });
        const lines = new THREE.LineSegments(geo, mat);
        this.root.add(lines);
        this.disposables.push(geo, mat);
      }
    }

    this.frameCamera();
  }

  private clear(): void {
    while (this.root.children.length) {
      this.root.remove(this.root.children[0]!);
    }
    this.pickables.length = 0;
    for (const d of this.disposables) d.dispose();
    this.disposables.length = 0;
  }

  private frameCamera(): void {
    if (this.pickables.length === 0) return;
    // Fit the XZ disk (the layout is a flat sunflower), not the AABB diagonal —
    // a tall score axis + diagonal corner would push the camera too far and leave a pea-sized graph.
    let cx = 0;
    let cz = 0;
    for (const p of this.pickables) {
      cx += p.mesh.position.x;
      cz += p.mesh.position.z;
    }
    cx /= this.pickables.length;
    cz /= this.pickables.length;
    let diskR = 0;
    for (const p of this.pickables) {
      const mesh = p.mesh as THREE.Mesh;
      const geo = mesh.geometry as THREE.SphereGeometry;
      const r = geo.parameters?.radius ?? 2;
      diskR = Math.max(
        diskR,
        Math.hypot(mesh.position.x - cx, mesh.position.z - cz) + r,
      );
    }
    diskR = Math.max(diskR, 16);
    const centre = new THREE.Vector3(cx, 0, cz);
    this.controls.target.copy(centre);
    this.controls.maxDistance = Math.max(500, diskR * 8);
    this.controls.minDistance = Math.max(10, diskR * 0.2);
    const fov = (this.camera.fov * Math.PI) / 180;
    // 1.05 fills the panel; slight tilt needs a little extra, not 1.35+
    const distance = (diskR / Math.sin(fov / 2)) * 1.08;
    const dir = new THREE.Vector3(0.35, 0.85, 0.55).normalize();
    this.camera.position.copy(centre).addScaledVector(dir, distance);
    this.camera.near = Math.max(0.5, diskR / 200);
    this.camera.far = Math.max(8000, diskR * 40);
    this.camera.updateProjectionMatrix();
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
      const hit = this.raycaster.intersectObjects(
        this.pickables.map((p) => p.mesh),
        false,
      )[0];
      const found = hit ? this.pickables.find((p) => p.mesh === hit.object) : undefined;
      this.onHover(found?.node ?? null);
    };
    const leave = () => this.onHover(null);
    const click = () => {
      this.raycaster.setFromCamera(this.pointer, this.camera);
      const hit = this.raycaster.intersectObjects(
        this.pickables.map((p) => p.mesh),
        false,
      )[0];
      const found = hit ? this.pickables.find((p) => p.mesh === hit.object) : undefined;
      this.onPick(found?.node ?? null);
    };
    target.addEventListener("pointermove", move);
    target.addEventListener("pointerleave", leave);
    target.addEventListener("pointerdown", click);
  }

  start(): void {
    this.mounted = true;
    this.loop();
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
