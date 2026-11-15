// A closed 3D view must give its WebGL context back at once: `dispose()`
// alone leaves the context alive until garbage collection, and WebKit and
// WebView2 keep only about 16 of them, so switching views and jobs ran into
// "Too many active WebGL contexts" and lost the live view's context (ERRU-008).

/** What `releaseRenderer` needs of a three.js WebGLRenderer. */
export interface Releasable {
  dispose(): void;
  forceContextLoss(): void;
  domElement: { remove(): void };
}

/** Frees the renderer's GPU resources, loses its context now and removes its canvas. */
export function releaseRenderer(r: Releasable): void {
  r.dispose();
  r.forceContextLoss();
  r.domElement.remove();
}
