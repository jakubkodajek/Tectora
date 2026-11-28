// The bug report dialog's state. The open editor registers what a report
// needs from it (the job, the pictures of its two views, its unsaved copy);
// pressing the top-bar button takes those before the dialog covers the
// views, then the dialog (mounted in App.svelte) shows them.
import { fitPng, rescalePng, type LeftOut } from "./bugreport";

/** What a view gives: the thing, why it could not, or null when the view is not shown. */
export type Taken<K extends string, T> = ({ [P in K]: T } | { reason: string }) | null;

export interface ReportContext {
  jobId: string;
  jobName: string;
  layout: string;
  /** The plan view as a standalone SVG. */
  planSvg: () => Taken<"svg", string>;
  /** The 3D view as a PNG. */
  modelPng: () => Promise<Taken<"png", Uint8Array>>;
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
  /** What could not be pictured, with the reason. */
  leftOut: LeftOut[];
  /** False only when the 3D view has no WebGL. */
  webgl: boolean;
}

let context: ReportContext | null = null;

export const report = $state<{ capture: Capture | null; busy: boolean; draft: string }>({
  capture: null,
  busy: false,
  /** What was typed, kept until the report is saved: closing the dialog keeps it. */
  draft: "",
});

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
  if (report.capture || report.busy) return;
  const c = context;
  if (!c) {
    report.capture = { jobId: null, jobName: null, screen: "library", layout: "", planSvg: null, modelPng: null, editorProject: null, leftOut: [], webgl: true };
    return;
  }
  // The button shows busy from the first press: preparing takes a moment.
  report.busy = true;
  try {
    try {
      await c.prepare();
    } catch {
      // The edits stay in memory; the report carries the editor's copy.
    }
    const leftOut: LeftOut[] = [];
    let png: Uint8Array | null = null;
    let webgl = true;
    try {
      const taken = await Promise.race([c.modelPng(), new Promise<{ reason: string }>((r) => setTimeout(() => r({ reason: "timeout" }), PICTURE_WAIT))]);
      if (taken && "png" in taken) {
        png = await fitPng(taken.png, (side) => rescalePng(taken.png, side));
        if (!png) leftOut.push({ part: "model", reason: "too_large" });
      } else if (taken) {
        if (taken.reason === "no_webgl") webgl = false;
        leftOut.push({ part: "model", reason: taken.reason });
      }
    } catch {
      leftOut.push({ part: "model", reason: "failed" });
    }
    let plan: string | null = null;
    try {
      const taken = c.planSvg();
      if (taken && "svg" in taken) plan = taken.svg;
      else if (taken) leftOut.push({ part: "plan", reason: taken.reason });
    } catch {
      leftOut.push({ part: "plan", reason: "failed" });
    }
    report.capture = {
      jobId: c.jobId,
      jobName: c.jobName,
      screen: "editor",
      layout: c.layout,
      planSvg: plan,
      modelPng: png,
      editorProject: c.editorProject(),
      leftOut,
      webgl,
    };
  } finally {
    report.busy = false;
  }
}

export function closeReport() {
  report.capture = null;
}
