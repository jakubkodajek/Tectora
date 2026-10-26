export interface Point2 {
  x: number;
  y: number;
}

export interface HalfHip {
  height: number;
  pitch_deg: number;
}

export interface EdgeDef {
  start: Point2;
  pitch_deg: number;
  eave_height: number;
  overhang: number;
  half_hip?: HalfHip | null;
  direction_rad?: number | null;
}

export interface RoofPart {
  name: string;
  edges: EdgeDef[];
}

export type JobStatus = "draft" | "offer" | "ordered" | "done";
export const JOB_STATUSES: JobStatus[] = ["draft", "offer", "ordered", "done"];

export interface JobInfo {
  number: string;
  customer: string;
  address: string;
  status: JobStatus;
}

export interface Project {
  schema_version: number;
  name: string;
  notes: string;
  job: JobInfo;
  parts: RoofPart[];
  dormers: unknown[];
  chimneys: { position: Point2; width: number; depth: number }[];
}

export interface ActivityInfo {
  ts_ms: number;
  actor: string;
  summary: string;
}

export interface ProjectSummary {
  id: string;
  name: string;
  notes: string;
  job: JobInfo;
  last_activity: ActivityInfo | null;
  modified_ms: number;
  roof_area: number;
  parts: number;
  outlines: [number, number][][];
  lines: [[number, number], [number, number]][];
}

export interface ExampleInfo {
  id: string;
  name: string;
  description: string;
  outlines: [number, number][][];
  lines: [[number, number], [number, number]][];
}

export type LineKind = "eave" | "verge" | "ridge" | "hip" | "valley" | "abutment";

export interface RoofLine {
  kind: LineKind;
  a: [number, number, number];
  b: [number, number, number];
}

export interface RoofFace {
  edge: number;
  pitch_deg: number;
  polygon: [number, number, number][];
  area: number;
}

export interface RoofQuantities {
  roof_area: number;
  ridge_length: number;
  hip_length: number;
  hip_count: number;
  valley_length: number;
  valley_count: number;
  eave_length: number;
  verge_length: number;
  abutment_length: number;
  ridge_height: number;
}

export interface CalcResult {
  parts: { name: string; geometry: { faces: RoofFace[]; lines: RoofLine[]; gables: [number, number, number][][] }; quantities: RoofQuantities }[];
  totals: RoofQuantities;
  /** Line lengths by DACAD code (T, F, G, K, O, W). */
  codes: Record<string, number>;
}

export interface Change {
  kind: "added" | "removed" | "modified";
  field: string;
  part?: number;
  edge?: number;
  before: unknown;
  after: unknown;
}

export interface ActivityItem extends ActivityInfo {
  changes: Change[];
}

export interface LiveStatus {
  ts_ms: number;
  actor: string;
  message: string;
  project_id: string | null;
}

export type StandardKind = "gable" | "shed" | "hip" | "pyramid" | "half_hip";
export const STANDARD_KINDS: StandardKind[] = ["gable", "hip", "half_hip", "shed", "pyramid"];

export interface Presets {
  pitch_deg: number;
  eave_height: number;
  overhang: number;
}

export const USER_ACTOR = "user";
export const isAi = (actor: string) => actor !== USER_ACTOR;

export function emptyProject(name: string): Project {
  return {
    schema_version: 1,
    name,
    notes: "",
    job: { number: "", customer: "", address: "", status: "draft" },
    parts: [],
    dormers: [],
    chimneys: [],
  };
}

export const GABLE_PITCH = 90;
export const isGable = (e: EdgeDef) => e.pitch_deg >= 89.999;

export type Selection = { part: number; edge: number } | null;
