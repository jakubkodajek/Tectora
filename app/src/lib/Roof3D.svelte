<script lang="ts">
  import { ui } from "./theme.svelte";
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
  import { LineMaterial } from "three/examples/jsm/lines/LineMaterial.js";
  import { LineSegments2 } from "three/examples/jsm/lines/LineSegments2.js";
  import { LineSegmentsGeometry } from "three/examples/jsm/lines/LineSegmentsGeometry.js";
  import type { CalcResult, Point2, Project, Selection } from "./types";
  import type { ItemRef, PlanTarget } from "./items";
  import { t } from "./i18n.svelte";
  import { hostHeightAt, isDormerFace, slopeAt } from "./dormers";
  import { isConvex, windowOutline } from "./openings";
  import { chimneyTop, clipConvex, crossings, dormerWallLines, manualFacePoly, onWallLine, outgrows, planArea, planeOf, wallTriangles, wallUpToRoof, type Extent, type Poly3 } from "./scene3d";
  import { isGable } from "./types";
  import { presets } from "./settings.svelte";

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
  /** The extent the camera was last framed for; a scene growing past it reframes (UI-007). */
  let framedFor: Extent | null = null;
  const extentOf = (b: THREE.Box3): Extent => ({ min: [b.min.x, b.min.y, b.min.z], max: [b.max.x, b.max.y, b.max.z] });

  /** Roof line kind → its plan colour token, so 3D and the plan legend agree. */
  const LINE_TOKENS: Record<string, string> = {
    eave: "--line-eave",
    verge: "--line-verge",
    ridge: "--line-ridge",
    hip: "--line-hip",
    valley: "--line-valley",
    pult: "--line-pult",
    knick: "--line-knick",
    gorge: "--line-gorge",
    abutment: "--line-abutment",
    abutment_front: "--line-abutment",
    abutment_back: "--line-abutment",
    abutment_side: "--line-abutment",
  };
  /** How far a roof window's frame stands above the roof and its frame width, m
   *  [no source: display only, typical roof-window proportions]. */
  const WINDOW_PROUD = 0.09;
  const WINDOW_FRAME = 0.07;
  /** Screen width of the roof lines, px. */
  const LINE_PX = 2.5;

  let groundMat: THREE.MeshStandardMaterial;
  const lineMats: LineMaterial[] = [];

  /** A colour token from app.css (light or dark, as currently applied). */
  function token(name: string, fallback = "#808080"): THREE.Color {
    const v = host ? getComputedStyle(host).getPropertyValue(name).trim() : "";
    const c = new THREE.Color();
    try {
      c.setStyle(v || fallback);
    } catch {
      c.setStyle(fallback);
    }
    return c;
  }

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

  /** A mesh of the given triangles (indices into `ps`, plan x, y, z). */
  function indexedMesh(ps: Poly3, idx: number[], mat: THREE.Material, target?: PlanTarget): THREE.Mesh {
    const g = new THREE.BufferGeometry().setFromPoints(ps.map(v3));
    g.setIndex(idx);
    g.computeVertexNormals();
    const m = new THREE.Mesh(g, mat);
    if (target) m.userData.target = target;
    return m;
  }

  /** True when the vertical polygon `g` stands on the wall line a–b (within 2 cm). */
  function onWall(g: Poly3, a: Point2, b: Point2): boolean {
    const len = Math.hypot(b.x - a.x, b.y - a.y);
    if (len < 1e-6) return false;
    return g.every((p) => Math.abs((p[0] - a.x) * (b.y - a.y) - (p[1] - a.y) * (b.x - a.x)) / len < 0.02);
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
    lineMats.length = 0;
    groundMat?.color.copy(token("--scene-ground"));

    // Colours come from the theme tokens (app.css, --scene-*), as in the plan:
    // a selected wall or roof face is --focus, a selected object --accent.
    const roofMat = new THREE.MeshStandardMaterial({
      color: token("--scene-roof"),
      roughness: 0.8,
      metalness: 0.0,
      side: THREE.DoubleSide,
      polygonOffset: true,
      polygonOffsetFactor: 1,
      polygonOffsetUnits: 1,
    });
    const selMat = roofMat.clone();
    selMat.color.copy(token("--focus"));
    const wallMat = new THREE.MeshStandardMaterial({ color: token("--scene-wall"), roughness: 0.9, side: THREE.DoubleSide });
    // A selected dormer is an object: --accent, like in the plan.
    const selDormerMat = roofMat.clone();
    selDormerMat.color.copy(token("--accent"));
    const selWallMat = wallMat.clone();
    selWallMat.color.lerp(token("--accent"), 0.45);
    const brickMat = new THREE.MeshStandardMaterial({ color: token("--scene-brick"), roughness: 0.85 });
    const capMat = new THREE.MeshStandardMaterial({ color: token("--scene-metal"), roughness: 0.6 });
    const frameMat = new THREE.MeshStandardMaterial({ color: token("--scene-metal"), roughness: 0.45, metalness: 0.2, side: THREE.DoubleSide });
    // Glass glows a little so a window reads as glass on every face, lit or not.
    const glass = token("--window");
    const glassMat = new THREE.MeshStandardMaterial({
      color: glass,
      emissive: glass.clone().multiplyScalar(0.35),
      roughness: 0.15,
      metalness: 0.1,
      side: THREE.DoubleSide,
    });
    const holeMat = new THREE.MeshStandardMaterial({ color: token("--scene-hole"), roughness: 1, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -2 });
    const selObjMat = new THREE.MeshStandardMaterial({ color: token("--accent"), roughness: 0.6, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -4 });

    // Walls: from ground to each edge's eave height, on the wall line. A
    // gable wall the calculation returns no gable polygon for (a shed
    // roof's verges and high wall) goes up to the roof, so the building is
    // closed (UI-010).
    project.parts.forEach((part, pi) => {
      const n = part.edges.length;
      const rp = result?.parts[pi];
      part.edges.forEach((e, i) => {
        const b = part.edges[(i + 1) % n];
        const target: PlanTarget = { type: "edge", part: pi, edge: i };
        if (rp && result && isGable(e) && !rp.geometry.gables.some((g) => onWall(g, e.start, b.start))) {
          const own = { ...result, parts: [rp] };
          const cuts = crossings(e.start, b.start, rp.geometry.lines.map((l) => [l.a, l.b]));
          const wall = wallUpToRoof(e.start, b.start, 0, cuts, (p) => hostHeightAt(own, p));
          const idx = wall && wallTriangles(wall);
          if (wall && idx) {
            content.add(indexedMesh(wall, idx, wallMat, target));
            return;
          }
        }
        const h = Math.max(0.01, Math.min(e.eave_height, b.eave_height));
        content.add(quad([[e.start.x, e.start.y, 0], [b.start.x, b.start.y, 0], [b.start.x, b.start.y, h], [e.start.x, e.start.y, h]], wallMat, target));
      });
    });

    // Manual faces: counted in the roof area, so shown in 3D too (UX-009),
    // as neutral planes of their pitch from the lowest eave.
    const eaves = project.parts.flatMap((p) => p.edges.map((e) => e.eave_height));
    const faceBase = eaves.length ? Math.min(...eaves) : presets.eave_height;
    const manualMat = new THREE.MeshStandardMaterial({ color: token("--scene-wall"), roughness: 0.9, side: THREE.DoubleSide, polygonOffset: true, polygonOffsetFactor: -1 });
    (project.manual_faces ?? []).forEach((f, index) => {
      const poly = manualFacePoly(f.points, f.pitch_deg, faceBase);
      if (!poly || planArea(f.points) < 1e-6) return;
      const target: PlanTarget = { type: "item", item: { kind: "face", index } };
      content.add(polyMesh(poly, [], same(item, target.item) ? selObjMat : manualMat, target));
    });

    // A non-convex opening stops the whole roof calculation (warned in the editor).
    const holes = (project.roof_openings ?? [])
      .map((c, index) => ({ index, corners: c.corners, area: planArea(c.corners) }))
      .filter((h) => h.corners.length >= 3 && h.area > 1e-6 && isConvex(h.corners));
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
                // Closed at the bottom, so the hole reads as an opening and
                // not as a view of the underside of the far roof face.
                content.add(polyMesh(lifted.map((p) => [p[0], p[1], p[2] - 0.15]), [], selected ? selObjMat : holeMat, target));
              } else {
                content.add(polyMesh(lifted, [], selected ? selObjMat : holeMat, target));
              }
            }
          }
          const sel = dormer >= 0 ? same(item, { kind: "dormer", index: dormer }) : selection?.part === pi && selection?.edge === f.edge;
          const target: PlanTarget = dormer >= 0 ? { type: "item", item: { kind: "dormer", index: dormer } } : { type: "edge", part: pi, edge: f.edge };
          content.add(polyMesh(f.polygon, cut, sel ? (dormer >= 0 ? selDormerMat : selMat) : roofMat, isDormerFace(f.edge) && dormer < 0 ? undefined : target));

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
          // Triangulated in the wall plane (scene3d.wallTriangles, tested).
          const idx = wallTriangles(poly);
          if (idx) content.add(indexedMesh(poly, idx, wallMat));
        }
        // Roof lines, grouped by kind, a few px wide so each line code reads
        // against the roof like in the plan.
        const byKind = new Map<string, number[]>();
        for (const l of rp.geometry.lines) {
          const tok = LINE_TOKENS[l.kind] ?? "--text-2";
          const a = v3(l.a);
          const b = v3(l.b);
          if (!byKind.has(tok)) byKind.set(tok, []);
          byKind.get(tok)!.push(a.x, a.y, a.z, b.x, b.y, b.z);
        }
        for (const [tok, pos] of byKind) {
          const g = new LineSegmentsGeometry().setPositions(pos);
          const mat = new LineMaterial({ color: token(tok).getHex(), linewidth: LINE_PX });
          mat.resolution.set(host.clientWidth, host.clientHeight);
          lineMats.push(mat);
          const ls = new LineSegments2(g, mat);
          // Lines are not pickable: a click goes to the face under them.
          ls.raycast = () => {};
          content.add(ls);
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

    // Roof windows: a frame box standing 9 cm proud of the roof face under
    // the anchor, in the frame colour, with glowing glass inset 7 cm, so a
    // window stands out from the roof and from roof openings at any angle.
    (project.windows ?? []).forEach((w, index) => {
      if (!result) return;
      const anchor = { x: w.x + 1e-3, y: w.y + 1e-3 };
      const z0 = hostHeightAt(result, anchor);
      const s = slopeAt(result, anchor);
      if (z0 === null || !s) return;
      const target: PlanTarget = { type: "item", item: { kind: "window", index } };
      const fm = same(item, target.item) ? selObjMat : frameMat;
      const lift = (p: Point2, up: number): [number, number, number] => [p.x, p.y, z0 + s.tan * ((p.x - w.x) * s.up.x + (p.y - w.y) * s.up.y) + up];
      const outline = windowOutline(result, w);
      const base = outline.map((p) => lift(p, -0.02));
      const top = outline.map((p) => lift(p, WINDOW_PROUD));
      content.add(quad(top, fm, target));
      top.forEach((p, k) => {
        const q = top[(k + 1) % 4];
        content.add(quad([base[k], base[(k + 1) % 4], q, p], fm, target));
      });
      const [a, b, , d] = top;
      const U = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const V = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
      const iu = Math.min(0.4, WINDOW_FRAME / (Math.hypot(...U) || 1));
      const iv = Math.min(0.4, WINDOW_FRAME / (Math.hypot(...V) || 1));
      const at = (su: number, sv: number): [number, number, number] =>
        [0, 1, 2].map((i) => a[i] + U[i] * su + V[i] * sv + (i === 2 ? 0.012 : 0)) as [number, number, number];
      content.add(quad([at(iu, iv), at(1 - iu, iv), at(1 - iu, 1 - iv), at(iu, 1 - iv)], glassMat, target));
    });

    scene.add(content);
    if (result && (!framed || growsPastFrame())) frame();
  }

  /** True when the built scene reaches outside the extent the camera was framed for. */
  function growsPastFrame(): boolean {
    const box = new THREE.Box3().setFromObject(content);
    return !box.isEmpty() && outgrows(framedFor, extentOf(box));
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
    framedFor = extentOf(box);
    // The framed plan extent (x0, y0, x1, y1), for tests of what the view shows.
    host.dataset.frame = [box.min.x, -box.max.z, box.max.x, -box.min.z].map((v) => v.toFixed(2)).join(",");
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

    // Soft sky plus a stronger sun, so roof planes facing different ways
    // get clearly different shades.
    scene.add(new THREE.HemisphereLight(0xffffff, 0x8899aa, 1.1));
    const sun = new THREE.DirectionalLight(0xffffff, 2.8);
    sun.position.set(-25, 45, 35);
    scene.add(sun);
    const ground = new THREE.Mesh(
      new THREE.CircleGeometry(200, 64),
      (groundMat = new THREE.MeshStandardMaterial({ color: token("--scene-ground"), roughness: 1 })),
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
      for (const m of lineMats) m.resolution.set(w, h);
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
