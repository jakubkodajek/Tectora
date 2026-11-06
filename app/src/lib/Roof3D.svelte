<script lang="ts">
  import { isDark, ui } from "./theme.svelte";
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
  import type { CalcResult, Point2, Project, Selection } from "./types";
  import type { ItemRef, PlanTarget } from "./items";
  import { t } from "./i18n.svelte";
  import { hostHeightAt, isDormerFace, slopeAt } from "./dormers";
  import { windowOutline } from "./openings";
  import { chimneyTop, clipConvex, dormerWallLines, onWallLine, planArea, planeOf, type Poly3 } from "./scene3d";

  let {
    project,
    result,
    selection,
    item = null,
    onpick,
    oncontext,
  }: {
    project: Project;
    result: CalcResult | null;
    selection: Selection;
    /** The selected dormer, chimney, window, opening or hand-drawn shape. */
    item?: ItemRef | null;
    /** Left click on an object or a roof face (selects it). */
    onpick?: (target: PlanTarget) => void;
    /** Right click without dragging. */
    oncontext?: (target: PlanTarget, client: { x: number; y: number }, world: Point2) => void;
  } = $props();

  let host: HTMLDivElement;
  let renderer: THREE.WebGLRenderer;
  let scene: THREE.Scene;
  let camera: THREE.PerspectiveCamera;
  let controls: OrbitControls;
  let content = new THREE.Group();
  let framed = false;

  const LINE_COLORS: Record<string, number> = {
    eave: 0x2f6fd6,
    verge: 0xe39b2b,
    ridge: 0xd6402f,
    hip: 0x2c9a5b,
    valley: 0x8a46c9,
    pult: 0xc2417a,
    knick: 0x1f9aa8,
    gorge: 0x6b4fd8,
    abutment: 0x8b5a3c,
    abutment_front: 0x8b5a3c,
    abutment_back: 0x8b5a3c,
    abutment_side: 0x8b5a3c,
  };
  const SELECTED = 0x3b6fe0;

  const dark = isDark;

  // Plan (x, y) with height z maps to three.js (x, z, -y): y-up world.
  const v3 = (p: [number, number, number]) => new THREE.Vector3(p[0], p[2], -p[1]);

  const same = (a: ItemRef | null, b: ItemRef) => !!a && a.kind === b.kind && a.index === b.index;

  /** A mesh of plan-triangulated polygons lying in planes (`pts` in plan x, y, z). */
  function polyMesh(outer: Poly3, holes: Poly3[], mat: THREE.Material, target?: PlanTarget): THREE.Mesh {
    const contour = outer.map((p) => new THREE.Vector2(p[0], p[1]));
    const hs = holes.map((h) => h.map((p) => new THREE.Vector2(p[0], p[1])));
    const tris = THREE.ShapeUtils.triangulateShape(contour, hs);
    const g = new THREE.BufferGeometry().setFromPoints([...outer, ...holes.flat()].map(v3));
    g.setIndex(tris.flat());
    g.computeVertexNormals();
    const m = new THREE.Mesh(g, mat);
    if (target) m.userData.target = target;
    return m;
  }

  /** A vertical or sloped quad strip p0 p1 p2 p3 (plan x, y, z). */
  function quad(ps: Poly3, mat: THREE.Material, target?: PlanTarget): THREE.Mesh {
    const g = new THREE.BufferGeometry().setFromPoints(ps.map(v3));
    g.setIndex(ps.length === 4 ? [0, 1, 2, 0, 2, 3] : [0, 1, 2]);
    g.computeVertexNormals();
    const m = new THREE.Mesh(g, mat);
    if (target) m.userData.target = target;
    return m;
  }

  /** A prism over a plan polygon from z0 to z1. */
  function prism(corners: Point2[], z0: number, z1: number, mat: THREE.Material, target?: PlanTarget): THREE.Mesh {
    const shape = new THREE.Shape(corners.map((p) => new THREE.Vector2(p.x, p.y)));
    const g = new THREE.ExtrudeGeometry(shape, { depth: Math.max(0.01, z1 - z0), bevelEnabled: false });
    // Shape plane (x, y) extruded along +z; turn it into the y-up world.
    g.rotateX(-Math.PI / 2);
    g.translate(0, z0, 0);
    const m = new THREE.Mesh(g, mat);
    if (target) m.userData.target = target;
    return m;
  }

  function build() {
    if (!scene) return;
    scene.remove(content);
    content.traverse((o) => {
      const m = o as THREE.Mesh;
      m.geometry?.dispose();
      const mat = m.material as THREE.Material | THREE.Material[] | undefined;
      if (Array.isArray(mat)) mat.forEach((x) => x.dispose());
      else mat?.dispose();
    });
    content = new THREE.Group();

    const roofMat = new THREE.MeshStandardMaterial({
      color: 0xc65a32,
      roughness: 0.75,
      metalness: 0.0,
      side: THREE.DoubleSide,
      polygonOffset: true,
      polygonOffsetFactor: 1,
    });
    const selMat = roofMat.clone();
    selMat.color.set(SELECTED);
    const wallMat = new THREE.MeshStandardMaterial({ color: dark() ? 0x8d93a3 : 0xeceae5, roughness: 0.9, side: THREE.DoubleSide });
    const selWallMat = wallMat.clone();
    selWallMat.color.set(0x9db5ec);
    const brickMat = new THREE.MeshStandardMaterial({ color: 0x8e4636, roughness: 0.85 });
    const capMat = new THREE.MeshStandardMaterial({ color: 0x5c5b5a, roughness: 0.6 });
    const frameMat = new THREE.MeshStandardMaterial({ color: 0x3f4247, roughness: 0.5, metalness: 0.3, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -2 });
    const glassMat = new THREE.MeshStandardMaterial({ color: 0x5f86ad, roughness: 0.1, metalness: 0.6, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -4 });
    const holeMat = new THREE.MeshStandardMaterial({ color: 0x15171c, roughness: 1, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -2 });
    const selObjMat = new THREE.MeshStandardMaterial({ color: SELECTED, roughness: 0.6, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -4 });

    // Walls: from ground to each edge's eave height, on the wall line.
    project.parts.forEach((part, pi) => {
      const n = part.edges.length;
      part.edges.forEach((e, i) => {
        const b = part.edges[(i + 1) % n];
        const h = Math.max(0.01, Math.min(e.eave_height, b.eave_height));
        content.add(quad([[e.start.x, e.start.y, 0], [b.start.x, b.start.y, 0], [b.start.x, b.start.y, h], [e.start.x, e.start.y, h]], wallMat, { type: "edge", part: pi, edge: i }));
      });
    });

    const holes = (project.roof_openings ?? []).map((c, index) => ({ index, corners: c.corners, area: planArea(c.corners) }));
    const dormerLines = project.dormers.map(dormerWallLines);

    if (result) {
      result.parts.forEach((rp, pi) => {
        for (const f of rp.geometry.faces) {
          if (f.polygon.length < 3) continue;
          const plane = planeOf(f.polygon);
          const dormer = isDormerFace(f.edge)
            ? dormerLines.findIndex((ls) => f.polygon.some((p, k) => onWallLine(ls, p, f.polygon[(k + 1) % f.polygon.length])))
            : -1;
          // Roof openings: a hole in the face when it lies wholly inside,
          // else a dark patch where it crosses the face.
          const cut: Poly3[] = [];
          if (plane) {
            for (const h of holes) {
              const inter = clipConvex(f.polygon.map((p) => ({ x: p[0], y: p[1] })), h.corners);
              const a = planArea(inter);
              if (a < 1e-4) continue;
              const lifted: Poly3 = inter.map((p) => [p.x, p.y, plane(p)]);
              const target: PlanTarget = { type: "item", item: { kind: "hole", index: h.index } };
              const selected = same(item, target.item);
              if (Math.abs(a - h.area) < 1e-4) {
                cut.push(lifted);
                // The opening's lining, 15 cm down from the roof surface.
                lifted.forEach((p, k) => {
                  const q = lifted[(k + 1) % lifted.length];
                  content.add(quad([p, q, [q[0], q[1], q[2] - 0.15], [p[0], p[1], p[2] - 0.15]], selected ? selObjMat : holeMat, target));
                });
              } else {
                content.add(polyMesh(lifted, [], selected ? selObjMat : holeMat, target));
              }
            }
          }
          const sel = dormer >= 0 ? same(item, { kind: "dormer", index: dormer }) : selection?.part === pi && selection?.edge === f.edge;
          const target: PlanTarget = dormer >= 0 ? { type: "item", item: { kind: "dormer", index: dormer } } : { type: "edge", part: pi, edge: f.edge };
          content.add(polyMesh(f.polygon, cut, sel ? selMat : roofMat, isDormerFace(f.edge) && dormer < 0 ? undefined : target));

          // Dormer cheeks and front: walls hanging from the dormer roof's
          // edges on its wall lines down to the roof it stands on.
          if (dormer >= 0) {
            const wm = same(item, { kind: "dormer", index: dormer }) ? selWallMat : wallMat;
            f.polygon.forEach((p, k) => {
              const q = f.polygon[(k + 1) % f.polygon.length];
              if (!onWallLine(dormerLines[dormer], p, q)) return;
              const zp = hostHeightAt(result, { x: p[0], y: p[1] });
              const zq = hostHeightAt(result, { x: q[0], y: q[1] });
              if (zp === null || zq === null || (p[2] - zp < 0.01 && q[2] - zq < 0.01)) return;
              content.add(quad([p, q, [q[0], q[1], Math.min(zq, q[2])], [p[0], p[1], Math.min(zp, p[2])]], wm, { type: "item", item: { kind: "dormer", index: dormer } }));
            });
          }
        }
        for (const poly of rp.geometry.gables) {
          // Triangulate in the wall plane: u along the wall, v = height.
          const [p0, p1] = poly;
          const len = Math.hypot(p1[0] - p0[0], p1[1] - p0[1]) || 1;
          const d = [(p1[0] - p0[0]) / len, (p1[1] - p0[1]) / len];
          const contour = poly.map((p) => new THREE.Vector2((p[0] - p0[0]) * d[0] + (p[1] - p0[1]) * d[1], p[2]));
          const tris = THREE.ShapeUtils.triangulateShape(contour, []);
          const g = new THREE.BufferGeometry().setFromPoints(poly.map(v3));
          g.setIndex(tris.flat());
          g.computeVertexNormals();
          content.add(new THREE.Mesh(g, wallMat));
        }
        for (const l of rp.geometry.lines) {
          const g = new THREE.BufferGeometry().setFromPoints([v3(l.a), v3(l.b)]);
          content.add(new THREE.Line(g, new THREE.LineBasicMaterial({ color: LINE_COLORS[l.kind] ?? 0x333333 })));
        }
      });
    }

    // Chimneys: a brick prism from inside the roof up to its top, with a cap.
    project.chimneys.forEach((c, index) => {
      if (c.corners.length < 3) return;
      const target: PlanTarget = { type: "item", item: { kind: "chimney", index } };
      const { bottom, top } = chimneyTop(c, (p) => hostHeightAt(result, p));
      const selected = same(item, target.item);
      content.add(prism(c.corners, bottom, top - 0.08, selected ? selObjMat : brickMat, target));
      const cx = c.corners.reduce((s, p) => s + p.x, 0) / c.corners.length;
      const cy = c.corners.reduce((s, p) => s + p.y, 0) / c.corners.length;
      const cap = c.corners.map((p) => {
        const l = Math.hypot(p.x - cx, p.y - cy) || 1;
        return { x: p.x + ((p.x - cx) / l) * 0.05, y: p.y + ((p.y - cy) / l) * 0.05 };
      });
      content.add(prism(cap, top - 0.08, top, selected ? selObjMat : capMat, target));
    });

    // Roof windows: frame and glass lying on the face under the anchor.
    (project.windows ?? []).forEach((w, index) => {
      if (!result) return;
      const anchor = { x: w.x + 1e-3, y: w.y + 1e-3 };
      const z0 = hostHeightAt(result, anchor);
      const s = slopeAt(result, anchor);
      if (z0 === null || !s) return;
      const target: PlanTarget = { type: "item", item: { kind: "window", index } };
      const lift = (p: Point2, up: number): [number, number, number] => [p.x, p.y, z0 + s.tan * ((p.x - w.x) * s.up.x + (p.y - w.y) * s.up.y) + up];
      const outline = windowOutline(result, w);
      const frame = outline.map((p) => lift(p, 0.03));
      content.add(quad(frame, same(item, target.item) ? selObjMat : frameMat, target));
      // Glass inset by 6 cm in the roof plane.
      const [a, b, , d] = frame;
      const U = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const V = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
      const lu = Math.hypot(...U) || 1;
      const lv = Math.hypot(...V) || 1;
      const m = 0.06;
      const at = (su: number, sv: number): [number, number, number] => [0, 1, 2].map((i) => a[i] + U[i] * su + V[i] * sv + (i === 2 ? 0.01 : 0)) as [number, number, number];
      const iu = Math.min(0.45, m / lu);
      const iv = Math.min(0.45, m / lv);
      content.add(quad([at(iu, iv), at(1 - iu, iv), at(1 - iu, 1 - iv), at(iu, 1 - iv)], glassMat, target));
    });

    scene.add(content);
    if (!framed && result) frame();
  }

  /**
   * Camera directions of DACAD's views (Ansicht › Neue Vorder-, Hinter-,
   * linke / rechte Seiten-, Grundrissansicht) in three.js axes: the front
   * view looks north, from the plan's -y side.
   */
  const VIEWS = {
    iso: [0.9, 0.75, 1.1],
    front: [0, 0.06, 1],
    back: [0, 0.06, -1],
    left: [-1, 0.06, 0],
    right: [1, 0.06, 0],
    top: [0, 1, 0.0001],
  } as const;
  type View = keyof typeof VIEWS;
  let view = $state<View>("iso");

  function frame(v: View = view) {
    view = v;
    const box = new THREE.Box3().setFromObject(content);
    if (box.isEmpty()) return;
    const c = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3()).length();
    controls.target.copy(c);
    // Distance at which the bounding sphere fits the vertical field of view.
    const fov = (camera.fov * Math.PI) / 180;
    const dist = (size * 0.6) / Math.tan(fov / 2) / Math.min(1, camera.aspect);
    const dir = new THREE.Vector3(...VIEWS[v]).normalize();
    camera.position.copy(c).addScaledVector(dir, dist);
    camera.near = size / 100;
    camera.far = size * 20;
    camera.updateProjectionMatrix();
    controls.update();
    framed = true;
  }

  onMount(() => {
    renderer = new THREE.WebGLRenderer({ antialias: true, alpha: true });
    renderer.setPixelRatio(window.devicePixelRatio);
    host.appendChild(renderer.domElement);
    scene = new THREE.Scene();
    camera = new THREE.PerspectiveCamera(40, 1, 0.1, 1000);
    controls = new OrbitControls(camera, renderer.domElement);
    controls.enableDamping = true;
    controls.maxPolarAngle = Math.PI * 0.495;

    scene.add(new THREE.HemisphereLight(0xffffff, 0x8899aa, 1.6));
    const sun = new THREE.DirectionalLight(0xffffff, 2.2);
    sun.position.set(30, 50, 20);
    scene.add(sun);
    const ground = new THREE.Mesh(
      new THREE.CircleGeometry(200, 64),
      new THREE.MeshStandardMaterial({ color: dark() ? 0x1a1f2b : 0xe9ece6, roughness: 1 }),
    );
    ground.rotation.x = -Math.PI / 2;
    ground.position.y = -0.01;
    scene.add(ground);

    const resize = () => {
      const w = host.clientWidth;
      const h = host.clientHeight;
      renderer.setSize(w, h);
      camera.aspect = w / Math.max(h, 1);
      camera.updateProjectionMatrix();
    };
    const ro = new ResizeObserver(resize);
    ro.observe(host);
    resize();
    build();

    // Click (no orbit or pan in between) picks what is under the pointer:
    // left selects it, right (or Ctrl+click) opens its menu.
    const el = renderer.domElement;
    const ray = new THREE.Raycaster();
    let press: { x: number; y: number; button: number } | null = null;
    const hit = (e: PointerEvent) => {
      const r = el.getBoundingClientRect();
      ray.setFromCamera(new THREE.Vector2(((e.clientX - r.left) / r.width) * 2 - 1, -((e.clientY - r.top) / r.height) * 2 + 1), camera);
      const first = ray.intersectObjects(content.children, false).find((h) => h.object.userData.target);
      if (!first) return null;
      return { target: first.object.userData.target as PlanTarget, world: { x: +first.point.x.toFixed(3), y: +(-first.point.z).toFixed(3) } };
    };
    const down = (e: PointerEvent) => {
      press = { x: e.clientX, y: e.clientY, button: e.button === 0 && e.ctrlKey ? 2 : e.button };
    };
    const up = (e: PointerEvent) => {
      if (!press || Math.hypot(e.clientX - press.x, e.clientY - press.y) > 4) return (press = null);
      const b = press.button;
      press = null;
      const h = hit(e);
      if (b === 0 && h) onpick?.(h.target);
      else if (b === 2) {
        // Nothing hit: the point on the ground under the pointer.
        const g = ray.ray.intersectPlane(new THREE.Plane(new THREE.Vector3(0, 1, 0), 0), new THREE.Vector3());
        const world = h?.world ?? (g ? { x: +g.x.toFixed(3), y: +(-g.z).toFixed(3) } : { x: 0, y: 0 });
        oncontext?.(h?.target ?? { type: "canvas" }, { x: e.clientX, y: e.clientY }, world);
      }
    };
    const noMenu = (e: Event) => e.preventDefault();
    el.addEventListener("pointerdown", down, true);
    el.addEventListener("pointerup", up, true);
    el.addEventListener("contextmenu", noMenu);

    let raf = 0;
    const loop = () => {
      controls.update();
      renderer.render(scene, camera);
      raf = requestAnimationFrame(loop);
    };
    loop();
    return () => {
      cancelAnimationFrame(raf);
      el.removeEventListener("pointerdown", down, true);
      el.removeEventListener("pointerup", up, true);
      el.removeEventListener("contextmenu", noMenu);
      ro.disconnect();
      controls.dispose();
      renderer.dispose();
    };
  });

  $effect(() => {
    // Rebuild whenever inputs change.
    void project;
    void result;
    void selection;
    void item;
    void ui.dark;
    build();
  });
</script>

<div class="host" bind:this={host}>
  <div class="views" role="radiogroup" aria-label={t("view.title")}>
    {#each Object.keys(VIEWS) as v}
      <button class:on={view === v} role="radio" aria-checked={view === v} onclick={() => frame(v as View)} title={t(`view.${v}Hint`)}>{t(`view.${v}`)}</button>
    {/each}
  </div>
  <button class="fit icon-btn" title={t("plan.fit")} aria-label={t("plan.fit")} onclick={() => frame()}>
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8"><path d="M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5" /></svg>
  </button>
</div>

<style>
  .host {
    position: relative;
    width: 100%;
    height: 100%;
    background: linear-gradient(180deg, color-mix(in srgb, var(--focus) 8%, var(--surface)) 0%, var(--surface) 70%);
    overflow: hidden;
    container-type: inline-size;
  }
  .host :global(canvas) {
    display: block;
  }
  .views {
    position: absolute;
    left: 50%;
    bottom: 10px;
    transform: translateX(-50%);
    z-index: 2;
    display: flex;
    border: 1px solid var(--border);
    border-radius: 8px;
    overflow: hidden;
    background: var(--surface);
  }
  .views button {
    padding: 4px 9px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font-size: 12px;
    cursor: pointer;
    white-space: nowrap;
  }
  .views button + button {
    border-left: 1px solid var(--border);
  }
  .views button.on {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .fit {
    position: absolute;
    right: 10px;
    top: 10px;
    z-index: 2;
    background: var(--surface);
    border: 1px solid var(--border);
  }
  /* Small 3D view (half-screen split): view buttons stay inside the view. */
  @container (max-width: 420px) {
    .views {
      max-width: calc(100% - 16px);
      overflow-x: auto;
      scrollbar-width: none;
    }
    .views button {
      padding: 4px 6px;
      font-size: 11px;
    }
  }
</style>
