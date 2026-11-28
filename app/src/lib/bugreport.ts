// Helpers of the bug report (crates/tectora-core/src/bugreport.rs): the
// request the core takes, the size caps the core enforces (so a report is
// never refused for a picture), and the plan as a standalone SVG file.
import { t } from "./i18n.svelte";
import { recentActions, recentNotices } from "./reportlog";

export const MAX_DESCRIPTION = 5000;
/** The core's cap for the 3D picture, bytes. */
export const MAX_PNG = 8 * 1024 * 1024;
/** Longest side of a picture that had to be scaled down, px. */
export const PNG_SIDE = 2000;
/** Most elements of the plan the standalone SVG is built from. */
export const MAX_PLAN_NODES = 6000;

export interface Include {
  job: boolean;
  customer: boolean;
  files: boolean;
  history: boolean;
  snapshot: boolean;
  log: boolean;
}

export interface UiState {
  locale: string;
  theme: string;
  screen: string;
  layout: string;
  window: number[];
  webgl: boolean;
}

/** A part of the report the UI could not take, with a reason code of the core (`bugreport::REASONS`). */
export interface LeftOut {
  part: string;
  reason: string;
}

export interface ReportInput {
  description: string;
  jobId: string | null;
  include: Include;
  planSvg: string | null;
  modelPng: Uint8Array | null;
  editorProject: string | null;
  ui: UiState;
  /** What the UI could not picture (the core copies only the plan and the 3D picture). */
  leftOut?: LeftOut[];
}

/** The request of the core's `bugreport::Request`. */
export interface ReportRequest {
  description: string;
  job_id: string | null;
  include: Include;
  source: "app";
  reporter: string;
  local_stamp: string;
  client: { ui: UiState; actions: unknown[]; notices: unknown[]; left_out: LeftOut[] };
  editor_project: string | null;
  plan_svg: string | null;
  model_png: number[] | null;
}

const two = (n: number) => String(n).padStart(2, "0");

/** `YYYYMMDD-HHMMSS` in the user's local time, for the file name. */
export function reportStamp(d: Date = new Date()): string {
  return `${d.getFullYear()}${two(d.getMonth() + 1)}${two(d.getDate())}-${two(d.getHours())}${two(d.getMinutes())}${two(d.getSeconds())}`;
}

export function buildRequest(i: ReportInput): ReportRequest {
  const hasJob = !!i.jobId;
  const include: Include = {
    job: hasJob && i.include.job,
    customer: hasJob && i.include.customer,
    files: hasJob && i.include.files,
    history: hasJob && i.include.history,
    snapshot: i.include.snapshot,
    log: i.include.log,
  };
  return {
    description: i.description.length > MAX_DESCRIPTION ? i.description.slice(0, MAX_DESCRIPTION) : i.description,
    job_id: hasJob ? i.jobId : null,
    include,
    source: "app",
    reporter: "user",
    local_stamp: reportStamp(),
    // Steps and notices are part of the technical record.
    client: {
      ui: i.ui,
      actions: include.log ? recentActions() : [],
      notices: include.log ? recentNotices() : [],
      left_out: include.snapshot ? (i.leftOut ?? []) : [],
    },
    editor_project: hasJob && include.job ? i.editorProject : null,
    plan_svg: include.snapshot ? i.planSvg : null,
    model_png: include.snapshot && i.modelPng ? Array.from(i.modelPng) : null,
  };
}

/**
 * A picture within the core's cap: as it is when it fits, else scaled down
 * by `rescale` (at most [`PNG_SIDE`] px on the long side), else dropped.
 */
export async function fitPng(
  png: Uint8Array,
  rescale: (maxSide: number) => Promise<Uint8Array | null>,
): Promise<Uint8Array | null> {
  if (png.length <= MAX_PNG) return png;
  try {
    const smaller = await rescale(PNG_SIDE);
    return smaller && smaller.length <= MAX_PNG ? smaller : null;
  } catch {
    return null;
  }
}

/** A PNG scaled to at most `maxSide` px on its long side, through a canvas. */
export async function rescalePng(png: Uint8Array, maxSide: number): Promise<Uint8Array | null> {
  const bitmap = await createImageBitmap(new Blob([png as BlobPart], { type: "image/png" }));
  const k = Math.min(1, maxSide / Math.max(bitmap.width, bitmap.height));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(bitmap.width * k));
  canvas.height = Math.max(1, Math.round(bitmap.height * k));
  canvas.getContext("2d")?.drawImage(bitmap, 0, 0, canvas.width, canvas.height);
  const blob = await new Promise<Blob | null>((res) => canvas.toBlob(res, "image/png"));
  return blob ? new Uint8Array(await blob.arrayBuffer()) : null;
}

/**
 * The plan as a file that opens on its own: `xmlns` present and every
 * `var(--token)` replaced by the value `resolve` gives (else the variable's
 * own fallback, else `none`).
 */
export function standalonePlanSvg(svg: string, resolve: (name: string) => string): string {
  let out = svg.replace(/var\(\s*(--[\w-]+)\s*(?:,\s*([^()]*(?:\([^()]*\)[^()]*)*))?\)/g, (_m, name: string, fallback?: string) => {
    const v = resolve(name).trim();
    return v || (fallback ?? "").trim() || "none";
  });
  if (!/^\s*<svg[^>]*\sxmlns=/.test(out)) out = out.replace(/^\s*<svg/, '<svg xmlns="http://www.w3.org/2000/svg"');
  return out.trimStart();
}

const INLINED = [
  "fill",
  "fill-opacity",
  "stroke",
  "stroke-width",
  "stroke-opacity",
  "stroke-dasharray",
  "stroke-linecap",
  "stroke-linejoin",
  "opacity",
  "font-family",
  "font-size",
  "font-weight",
  "text-anchor",
  "visibility",
  "display",
] as const;

/**
 * The plan view's SVG as the user sees it, as a standalone file: the
 * computed look of every element is written into its `style`, theme
 * variables are resolved. Null when the plan has more than
 * [`MAX_PLAN_NODES`] elements (the core's own preview is in the report
 * anyway).
 */
export function serializePlan(svg: SVGSVGElement): string | null {
  const from = [svg, ...svg.querySelectorAll("*")];
  if (from.length > MAX_PLAN_NODES) return null;
  const copy = svg.cloneNode(true) as SVGSVGElement;
  const to = [copy, ...copy.querySelectorAll("*")];
  from.forEach((el, i) => {
    const cs = getComputedStyle(el);
    const style = INLINED.map((p) => `${p}:${cs.getPropertyValue(p)}`).join(";");
    to[i]?.setAttribute("style", style);
  });
  const box = svg.getBoundingClientRect();
  if (!copy.getAttribute("viewBox") && box.width > 0) copy.setAttribute("viewBox", `0 0 ${Math.round(box.width)} ${Math.round(box.height)}`);
  const root = getComputedStyle(document.documentElement);
  return standalonePlanSvg(copy.outerHTML, (name) => root.getPropertyValue(name));
}

const PARTS = ["files", "history", "log", "plan", "model", "preview", "editor_project", "job"];
const REASONS = ["too_large", "too_many", "unreadable", "unavailable", "invalid", "failed", "timeout", "too_many_nodes", "no_webgl"];

/**
 * A line for what the report left out, in the user's language: the part and
 * why. An entry of an older core (a plain string) gives the part it names;
 * an unknown reason gives the part alone.
 */
export function leftOutText(e: LeftOut | string): string {
  if (typeof e === "string") {
    const part = PARTS.find((p) => e.includes(p)) ?? "job";
    return `${t(`report.part.${part}`)}: ${t("report.left.failed")}`;
  }
  const part = PARTS.includes(e.part) ? t(`report.part.${e.part}`) : e.part;
  return REASONS.includes(e.reason) ? `${part}: ${t(`report.left.${e.reason}`)}` : part;
}
