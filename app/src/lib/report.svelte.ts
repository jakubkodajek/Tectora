// The bug report dialog's state. The open editor registers what a report
// needs from it (the job, the pictures of its two views, its unsaved copy);
// pressing the top-bar button takes those before the dialog covers the
// views, then the dialog (mounted in App.svelte) shows them.
import { fitPng, rescalePng } from "./bugreport";

export interface ReportContext {
  jobId: string;
  jobName: string;
  layout: string;
  /** The plan view as a standalone SVG, null when it is not shown. */
  planSvg: () => string | null;
  /** The 3D view as a PNG, null when it is not shown or WebGL is missing. */
  modelPng: () => Promise<Uint8Array | null>;
  /** The editor's job as JSON (its unsaved copy included). */
  editorProject: () => string | null;
  /** Saves the pending edits first. */
  prepare: () => Promise<unknown>;
}

/** What the dialog is about, taken when the button was pressed. */
export interface Capture {
  jobId: string | null;
  jobName: string | null;
  screen: "editor" | "library";
  layout: string;
  planSvg: string | null;
  modelPng: Uint8Array | null;
  editorProject: string | null;
  /** Whether a 3D picture could be taken (false: no WebGL or the view is hidden). */
  webgl: boolean;
}

let context: ReportContext | null = null;

export const report = $state<{ capture: Capture | null }>({ capture: null });

/** Registered by the editor while it is open; `release` removes it only if it is still the registered one. */
export function setReportContext(c: ReportContext | null) {
  context = c;
}
export function releaseReportContext(c: ReportContext) {
  if (context === c) context = null;
}

/** Longest wait for the 3D picture, ms: a view that never answers must not hold the dialog back. */
const PICTURE_WAIT = 2500;

export async function openReport() {
  if (report.capture) return;
  const c = context;
  if (!c) {
    report.capture = { jobId: null, jobName: null, screen: "library", layout: "", planSvg: null, modelPng: null, editorProject: null, webgl: true };
    return;
  }
  try {
    await c.prepare();
  } catch {
    // The edits stay in memory; the report carries the editor's copy.
  }
  let png: Uint8Array | null = null;
  try {
    png = await Promise.race([c.modelPng(), new Promise<null>((r) => setTimeout(() => r(null), PICTURE_WAIT))]);
    if (png) png = await fitPng(png, (side) => rescalePng(png as Uint8Array, side));
  } catch {
    png = null;
  }
  let plan: string | null = null;
  try {
    plan = c.planSvg();
  } catch {
    plan = null;
  }
  report.capture = {
    jobId: c.jobId,
    jobName: c.jobName,
    screen: "editor",
    layout: c.layout,
    planSvg: plan,
    modelPng: png,
    editorProject: c.editorProject(),
    webgl: png !== null || c.layout === "plan",
  };
}

export function closeReport() {
  report.capture = null;
}
