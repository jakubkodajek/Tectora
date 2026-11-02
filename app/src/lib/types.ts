export interface Point2 {
  x: number;
  y: number;
}

export interface HalfHip {
  height: number;
  pitch_deg: number;
}

export interface PitchBreak {
  height: number;
  pitch_deg: number;
}

export interface EdgeDef {
  start: Point2;
  pitch_deg: number;
  eave_height: number;
  overhang: number;
  half_hip?: HalfHip | null;
  breaks?: PitchBreak[];
  priority?: number;
  /** Gable covered with roof material: its area above the eave is roof area. */
  clad?: boolean;
  /** Roof material number, DACAD's list from 1 (1 = tiles, the default). */
  material?: number;
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
  dormers: Dormer[];
  chimneys: Chimney[];
  windows?: RoofWindow[];
  /** Holes in the roof (DACAD: Öffnung), shaped like chimneys. */
  roof_openings?: Chimney[];
  /** Lines drawn by hand on the calculated roof (DACAD: Dachlinieneingabe). */
  manual_lines?: ManualLine[];
  /** Roof faces added by hand (DACAD: Neue Dachfläche). */
  manual_faces?: ManualFace[];
  /** Calculate as a flat roof at the highest eave (DACAD: Flachdach). */
  flat_roof?: boolean;
  /** Own names of the roof materials, index 0 = material 1 (DACAD: Materialnamen). */
  material_names?: string[];
}

/** A line drawn by hand on the roof; `code` is DACAD's line code. */
export interface ManualLine {
  a: [number, number, number];
  b: [number, number, number];
  code: string;
  /** Length typed in instead of the measured one. */
  length?: number | null;
  label?: string;
}

/** A roof face added by hand: plan polygon and pitch. */
export interface ManualFace {
  points: Point2[];
  pitch_deg: number;
  label?: string;
}

/** A dormer as DACAD stores it: an open wall chain (cheek, front, cheek). */
export interface Dormer {
  edges: EdgeDef[];
  end: Point2;
}

export interface Chimney {
  corners: Point2[];
  height: number;
}

export interface RoofWindow {
  x: number;
  y: number;
  width: number;
  height: number;
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

export type LineKind = "eave" | "verge" | "ridge" | "hip" | "valley" | "pult" | "knick" | "gorge" | "abutment" | "abutment_front" | "abutment_back" | "abutment_side";

export interface RoofLine {
  kind: LineKind;
  a: [number, number, number];
  b: [number, number, number];
  side?: "left" | "right" | "oblique";
}

export interface RoofFace {
  edge: number;
  pitch_deg: number;
  polygon: [number, number, number][];
  area: number;
  deducted?: number;
  material?: number;
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
  pult_length: number;
  knick_length: number;
  gorge_length: number;
  abutment_length: number;
  ridge_height: number;
  /** Roof area per material number, sorted by number. */
  area_by_material?: [number, number][];
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
    windows: [],
  };
}

export const GABLE_PITCH = 90;
/** A vertical wall (pitch 90°, as the engine's GABLE_PITCH_DEG tolerance). */
export const isGable = (e: EdgeDef) => Math.abs(e.pitch_deg - 90) <= 0.001;
/** A zero-length wall that only carries a roof direction (DACAD: Richtungswechsel). */
export const isMarker = (e: EdgeDef) => e.direction_rad != null;

export type Selection = { part: number; edge: number } | null;
