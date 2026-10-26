<script lang="ts">
  import { onMount } from "svelte";
  import * as THREE from "three";
  import { OrbitControls } from "three/examples/jsm/controls/OrbitControls.js";
  import type { CalcResult, Project, Selection } from "./types";

  let { project, result, selection }: { project: Project; result: CalcResult | null; selection: Selection } =
    $props();

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
    abutment: 0x8b5a3c,
  };

  const dark = () => window.matchMedia("(prefers-color-scheme: dark)").matches;

  // Plan (x, y) with height z maps to three.js (x, z, -y): y-up world.
  const v3 = (p: [number, number, number]) => new THREE.Vector3(p[0], p[2], -p[1]);

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
    selMat.color.set(0x3b6fe0);
    const wallMat = new THREE.MeshStandardMaterial({ color: dark() ? 0x8d93a3 : 0xeceae5, roughness: 0.9, side: THREE.DoubleSide });

    // Walls: from ground to each edge's eave height, on the wall line.
    project.parts.forEach((part) => {
      const n = part.edges.length;
      part.edges.forEach((e, i) => {
        const b = part.edges[(i + 1) % n];
        const h = Math.max(0.01, Math.min(e.eave_height, b.eave_height));
        const g = new THREE.BufferGeometry().setFromPoints([
          v3([e.start.x, e.start.y, 0]),
          v3([b.start.x, b.start.y, 0]),
          v3([b.start.x, b.start.y, h]),
          v3([e.start.x, e.start.y, h]),
        ]);
        g.setIndex([0, 1, 2, 0, 2, 3]);
        g.computeVertexNormals();
        content.add(new THREE.Mesh(g, wallMat));
      });
    });

    if (result) {
      result.parts.forEach((rp, pi) => {
        for (const f of rp.geometry.faces) {
          if (f.polygon.length < 3) continue;
          const contour = f.polygon.map((p) => new THREE.Vector2(p[0], p[1]));
          const tris = THREE.ShapeUtils.triangulateShape(contour, []);
          const g = new THREE.BufferGeometry().setFromPoints(f.polygon.map(v3));
          g.setIndex(tris.flat());
          g.computeVertexNormals();
          const sel = selection?.part === pi && selection?.edge === f.edge;
          content.add(new THREE.Mesh(g, sel ? selMat : roofMat));
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
    scene.add(content);
    if (!framed && result) frame();
  }

  function frame() {
    const box = new THREE.Box3().setFromObject(content);
    if (box.isEmpty()) return;
    const c = box.getCenter(new THREE.Vector3());
    const size = box.getSize(new THREE.Vector3()).length();
    controls.target.copy(c);
    camera.position.set(c.x + size * 0.9, c.y + size * 0.75, c.z + size * 1.1);
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

    let raf = 0;
    const loop = () => {
      controls.update();
      renderer.render(scene, camera);
      raf = requestAnimationFrame(loop);
    };
    loop();
    return () => {
      cancelAnimationFrame(raf);
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
    build();
  });
</script>

<div class="host" bind:this={host}>
  <button class="fit icon-btn" title="Přizpůsobit" aria-label="Přizpůsobit zobrazení" onclick={frame}>
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
  }
  .host :global(canvas) {
    display: block;
  }
  .fit {
    position: absolute;
    right: 10px;
    top: 10px;
    z-index: 2;
    background: var(--surface);
    border: 1px solid var(--border);
  }
</style>
