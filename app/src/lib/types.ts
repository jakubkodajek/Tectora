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
  /** The result stored when the job left the draft status (the offer's figures). */
  result_snapshot?: ResultSnapshot | null;
}

export interface ResultSnapshot {
  engine_version: string;
  taken_ms: number;
  roof_area: number;
  ridge_height: number;
  lines_by_code: Record<string, number>;
  area_by_material: { material: number; area: number }[];
}

/** The stored result against the current one. */
export interface SnapshotStatus {
  stored: ResultSnapshot;
  /** key: roof_area, ridge_height, line:<code> or material:<number>. */
  differences: FigureDifference[];
  engine_changed: boolean;
}

/** A file attached to a job (drawing, photo). */
export interface Attachment {
  name: string;
  size: number;
  modified_ms: number;
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
  /** The wall the dormer sits on, where a file names it. */
  host_wall?: [Point2, Point2] | null;
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
  /** Lowest and highest roof pitch, degrees (gables left out). */
  pitch_range: [number, number] | null;
  dormers: number;
  /** Attached drawings and photos. */
  files: number;
  /** The figures an offer was made with are stored. */
  has_result: boolean;
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

/** The error DACAD's roof calculation ends with (no roof then). */
export interface CalcError {
  kind: "open_faces" | "no_solution" | "non_convex_opening" | "cut" | "geometry" | "no_end";
  /** DACAD's error code, e.g. E00002. */
  code?: string;
  message_id?: number;
  /** DACAD's message, in German as DACAD shows it. */
  message: string;
}

/** An input the roof engine cannot work with (that part is not calculated). */
export interface Problem {
  item: "part" | "dormer" | "chimney" | "opening" | "window";
  index: number;
  edge?: number;
  code:
    | "too_few_edges"
    | "start_nan"
    | "pitch_range"
    | "eave_nan"
    | "overhang_negative"
    | "direction_nan"
    | "stacked_nan"
    | "end_nan"
    | "bad_corners"
    | "bad_size";
}

/** What looks wrong with a calculated roof (tectora_core::check). */
export interface RoofWarning {
  kind:
    | "outside_outline"
    | "overlapping_faces"
    | "area_mismatch"
    | "merged_walls"
    | "covered"
    | "cut_exceeds_face"
    | "dormer_off_roof"
    | "not_calculated_by_core";
  /** m outside, m² overlapping, the relative area difference, m² below zero (cut_exceeds_face), else 0. */
  value: number;
  /** merged_walls: the edge whose pitch is used, then the edge whose pitch and breaks are ignored; cut_exceeds_face: the walls of the faces. */
  edges?: number[];
  /** dormer_off_roof: the dormer's index. */
  dormer?: number;
}

export interface CalcResult {
  parts: {
    name: string;
    geometry: {
      faces: RoofFace[];
      lines: RoofLine[];
      gables: [number, number, number][][];
      error?: CalcError;
      warnings?: RoofWarning[];
      /** The parts this roof is joined with, as the calculation joined them. */
      joined_with?: number[];
    };
    quantities: RoofQuantities;
  }[];
  totals: RoofQuantities;
  /** Line lengths by DACAD code (T, F, G, K, O, W). */
  codes: Record<string, number>;
  problems: Problem[];
  stored_result?: SnapshotStatus | null;
}

/** A project file in the library folder that could not be read. */
export interface BrokenFile {
  id: string;
  code: string;
  detail: string;
}

export interface Library {
  projects: ProjectSummary[];
  broken: BrokenFile[];
}

export interface Change {
  kind: "added" | "removed" | "modified";
  field: string;
  part?: number;
  edge?: number;
  /** Index of the dormer, chimney, window, opening, hand-drawn line or face. */
  item?: number;
  before: unknown;
  after: unknown;
}

/** One figure that differs between two versions (`tectora_core::snapshot::Difference`). */
export interface FigureDifference {
  key: string;
  stored: number;
  current: number;
}

/** AI changes waiting for the user's review (`tectora_core::store::ReviewStatus`). */
export interface ReviewStatus {
  since_ts_ms: number;
  count: number;
  actors: string[];
  before: Project | null;
  accepted_ms: number;
  /** Figures before the pending changes against now; null when unknown. */
  differences: FigureDifference[] | null;
}

export interface ActivityItem extends ActivityInfo {
  changes: Change[];
}

/** An AI app Tectora can add its MCP server to (`tectora_core::assistants`). */
export type AiAssistantId = "claude_desktop" | "codex" | "antigravity";

export interface AiAssistant {
  id: AiAssistantId;
  name: string;
  state: "not_installed" | "not_connected" | "connected" | "outdated";
  /** The configuration files Connect writes. */
  files: string[];
}

export interface LiveStatus {
  ts_ms: number;
  actor: string;
  /** English text (older servers); for a change, the agent's own summary. */
  message: string;
  project_id: string | null;
  /** What the agent does, translated in the app (`ai.live.<kind>`). */
  kind?: string;
  /** The job or file name the sentence names. */
  name?: string;
}

export type StandardKind = "gable" | "shed" | "hip" | "pyramid" | "half_hip";
export const STANDARD_KINDS: StandardKind[] = ["gable", "hip", "half_hip", "shed", "pyramid"];

export interface Presets {
  pitch_deg: number;
  eave_height: number;
  overhang: number;
}

/** App-wide settings (crates/tectora-core/src/settings.rs). */
export interface AppSettings {
  projects_dir: string | null;
  drawings_dir: string | null;
  locale: string | null;
  theme: "system" | "light" | "dark";
  roof_defaults: Presets;
  follow_ai: boolean;
}

export interface AppSettingsView {
  settings: AppSettings;
  effective: {
    projects_dir: string;
    drawings_dir: string;
    projects_dir_from_env: boolean;
    default_projects_dir: string;
    settings_file: string;
  };
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

/** A newer version of the app the update server offers (app/src-tauri/src/updates.rs). */
export interface AvailableUpdate {
  version: string;
  /** What the release says changed; empty when it says nothing. */
  notes: string;
}

/** How much of an update has been downloaded. */
export interface UpdateProgress {
  downloaded: number;
  /** The update's size, when the server says it. */
  total: number | null;
}

/** How the roof's quantities were calculated (core `breakdown`). */
/** Decimals the detailed steps show (core `breakdown::DECIMALS`). */
export interface BreakdownDecimals {
  coord: number;
  pitch: number;
  product: number;
  plan: number;
  cos: number;
  factor: number;
  area: number;
  length: number;
  max: number;
  /** Most decimals a sum is shown with ("≈" when even these do not add up). */
  sum_max: number;
}

export interface CalcBreakdown {
  decimals: BreakdownDecimals;
  parts: {
    name: string;
    walls: {
      no: number;
      length: number;
      gable: boolean;
      pitch_deg: number;
      eave_height: number;
      overhang: number;
      material: number;
      half_hip?: boolean;
      breaks?: number;
      clad?: boolean;
    }[];
    faces: BreakdownFace[];
    clad?: [number, number][];
    clad_faces?: BreakdownClad[];
    openings?: { kind: "window" | "chimney" | "dormer" | "hole"; area: number }[];
    /** Cut out where roofs are joined, not by an opening. */
    other_cuts?: number;
    /** Σ of the faces' deductions (= openings + other_cuts). */
    deducted: number;
    roof_area: number;
    /** Decimals for the part's sum; 0: adds up only to float noise ("≈"). */
    digits: number;
    calculated: boolean;
  }[];
  manual_faces: BreakdownManualFace[];
  materials: {
    material: number;
    name?: string;
    items: { part: number | null; face: number | null; area: number }[];
    area: number;
    digits: number;
  }[];
  lines: {
    code: string;
    items: BreakdownLine[];
    deducted?: number;
    /** Σ lengths − deducted, not rounded. */
    sum: number;
    digits: number;
    total: number;
  }[];
  roof_area: number;
  digits: number;
}

export interface BreakdownFace {
  no: number;
  /** Wall number from 1; null for a dormer face. */
  wall: number | null;
  pitch_deg: number;
  material: number;
  polygon: [number, number, number][];
  /** Shoelace products [x_i·y_j, x_j·y_i] per corner. */
  products: [number, number][];
  cross_sum: number;
  plan_area: number;
  cos_pitch: number;
  cos_digits: number;
  factor: number;
  gross: number;
  /** Check: area of the 3D corners (Newell). */
  area_3d: number;
  /** Largest vertical distance of a corner from the face's plane, m. */
  off_plane: number;
  deducted: number;
  area: number;
  /** The core's gross area where the formula's would not lead to the area as shown. */
  engine_gross?: number;
  /** Decimals for gross, deduction and area in the detailed steps. */
  digits: number;
  /** Near vertical: the gross is the core's, the factor only gross / plan. */
  vertical?: boolean;
}

export interface BreakdownClad {
  wall: number | null;
  material: number;
  pieces: { polygon: [number, number, number][]; uz: [number, number][]; products: [number, number][]; cross_sum: number; area: number }[];
  area: number;
  digits: number;
  /** Σ pieces where it differs from the counted area. */
  formula_area?: number;
}

export interface BreakdownManualFace {
  no: number;
  label: string;
  pitch_deg: number;
  points: [number, number][];
  products: [number, number][];
  cross_sum: number;
  plan_area: number;
  cos_pitch: number;
  cos_digits: number;
  factor: number;
  area: number;
  digits: number;
  /** The pitch gives no area (outside 0–90° or near vertical): 0 m². */
  pitch_out_of_range?: boolean;
}

export interface BreakdownLine {
  part: number | null;
  a: [number, number, number];
  b: [number, number, number];
  delta: [number, number, number];
  squares: [number, number, number];
  sum_sq: number;
  length: number;
  digits: number;
  /** Decimals for the squares and their sum. */
  sq_digits: number;
  typed?: boolean;
}

/** A face or line the plan highlights while the breakdown points at it. */
export type PlanMark = { polygon: [number, number][] } | { line: [[number, number], [number, number]] } | null;

/** A face number the plan shows at the face's centre. */
export interface PlanLabel {
  x: number;
  y: number;
  text: string;
}

/** What the detailed calculation dialog shows. */
export type BreakdownDetail =
  | { kind: "face"; face: BreakdownFace }
  | { kind: "manual"; face: BreakdownManualFace }
  | { kind: "clad"; clad: BreakdownClad }
  | { kind: "line"; code: string; line: BreakdownLine };

/** A part's deductions, listed under a face's last step. */
export interface PartDeductions {
  items: { label: string; area: number }[];
  total: number;
}
