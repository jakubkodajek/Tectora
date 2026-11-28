import { minOf } from "./extent";

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
  /** Building height (top of the walls), m; absent: its lowest eave. Changes no roof figure. */
  height?: number | null;
}

/** The building's height: its own, else its lowest eave (core `RoofPart::building_height`). */
export function buildingHeight(part: RoofPart): number | null {
  if (part.height != null) return part.height;
  return part.edges.length ? minOf(part.edges, (e) => e.eave_height) : null;
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
  /** Towers and turrets standing through the roof, each with its own roof profile (core `tower`). */
  towers?: Tower[];
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

/** Plan shape of a tower (core `tower::TowerShape`). */
export type TowerShape = "rectangle" | "polygon" | "circle";

/** One tier of a tower's roof profile, from the top of the tier below up (core `tower::TowerTier`). */
export interface TowerTier {
  /** Height the tier rises, m (0: a flat ledge). */
  rise: number;
  /** Width of the plan at the tier's top, m (0: it ends in a point). */
  top_width: number;
  /** The profile's arc height over its chord, m; + outward (onion), − inward (bell). */
  bulge?: number;
  /** Roof material number (2 sheet metal by default). */
  material?: number;
}

/** A tower or turret (core `tower::Tower`). */
export interface Tower {
  label?: string;
  center: Point2;
  /** Turn of the tower's x axis, degrees anticlockwise. */
  rotation_deg?: number;
  shape?: TowerShape;
  /** Shaft width, m (rectangle: along its x axis; polygon: across the flats; circle: diameter). */
  width: number;
  /** Depth of a rectangle, m; absent: a square. */
  depth?: number | null;
  /** Sides of a polygon (8 when absent). */
  sides?: number;
  corner_radius?: number;
  /** Height where the tower's roof starts (top of the shaft), m. */
  base_height: number;
  /** Eave overhang of the tower's roof beyond the shaft, m (0 or absent: the roof ends at the shaft; core `tower::Tower::overhang`). */
  overhang?: number;
  tiers: TowerTier[];
}

/** The pitch and area of some sides of a tier (core `tower::SidePitch`). */
export interface SidePitch {
  pitch_deg: number;
  area: number;
}

/** One calculated tier (core `tower::TierResult`). */
export interface TierResult {
  index: number;
  kind: "roof" | "wall";
  material: number;
  pitch_deg: number;
  area: number;
  /**
   * A rectangle that is not square: the pitch and area of its sides along
   * the depth (core `tower::SidePitch`); `pitch_deg` is then that of the
   * other two sides, and `area` still the whole tier's.
   */
  depth_sides?: SidePitch | null;
  bottom_height: number;
  top_height: number;
  bottom_perimeter: number;
  top_perimeter: number;
  hip_count: number;
  hip_length: number;
  /** Plan rings [x, y, z] from the tier's bottom to its top, to draw. */
  rings: [number, number, number][][];
}

/** A tower's calculated roof (core `tower::TowerResult`). */
export interface TowerResult {
  top_height: number;
  base_perimeter: number;
  base_area: number;
  tiers: TierResult[];
  cap?: { area: number; height: number; material: number; ring: [number, number, number][] };
  lines: { code: string; tier: number; length: number; points: [number, number, number][]; closed: boolean }[];
}

/** A tower in the calculation's answer (core `report::tower_report`); `result` null for a tower with problems. */
export interface TowerReport {
  label: string;
  result: TowerResult | null;
  roof_area?: number;
  wall_area?: number;
  shaft?: {
    /** The footprint's corners where the shaft leaves the roof or the ground. */
    bottom: [number, number, number][];
    wall_area: number;
    on_roof: boolean;
    base_below_roof: boolean;
    /** The eave of its overhang lies under the roof around it (warning tower_eave_below_roof). */
    eave_below_roof?: boolean;
    /** The building under it has no calculated roof: not beside the roofs, and `wall_area` is 0. */
    roof_failed?: boolean;
  };
}

/** The tower dialog's preview (core `commands::tower_preview`). */
export interface TowerPreview {
  result: TowerResult;
  roof_area: number;
  wall_area: number;
}

export type TowerPresetKind = "pyramid" | "octagon" | "cone" | "helm" | "onion" | "baroque";
export const TOWER_PRESETS: TowerPresetKind[] = ["pyramid", "octagon", "cone", "helm", "onion", "baroque"];

/** A curved-wall edit of the plan (core `commands::ArcEdit`). */
export type ArcEdit =
  | { op: "round_corner"; index: number; radius: number; segments?: number | null }
  | { op: "bend_wall"; index: number; bulge: number; segments?: number | null }
  | { op: "straighten_arc"; index: number }
  | { op: "reshape_arc"; index: number; bulge: number; segments?: number | null }
  | { op: "change_corner_radius"; index: number; radius: number; segments?: number | null };

/** The arc a wall belongs to (core `edit::arcs::ArcRun`): `walls` walls from `start`, its bulge (+ outward) and chord, m. */
export interface ArcRun {
  start: number;
  walls: number;
  bulge: number;
  chord: number;
}

export interface RoofWindow {
  x: number;
  y: number;
  width: number;
  height: number;
}

/**
 * What Tectora wrote a change-log entry as (store.rs `EntryKind`), from the
 * entry's fields, never from its words (audit 2026-11-23 ARCH-1401). An
 * entry without it is a change, and an AI's summary is its own words.
 */
export type EntryKind = "revert" | "restore" | "refresh";

export interface ActivityInfo {
  ts_ms: number;
  actor: string;
  summary: string;
  /** `null` for a change (store.rs `ActivityEntry::kind`, always sent since ARCH-1601); absent only from an older bridge. */
  kind?: EntryKind | null;
}

export interface ProjectSummary {
  id: string;
  name: string;
  notes: string;
  job: JobInfo;
  last_activity: ActivityInfo | null;
  /** The newest change when an AI client made it, by the core's one rule (store.rs `changed_by_ai`): the library's AI filter, count and badge (UX-806). */
  changed_by_ai?: ActivityInfo | null;
  /** How many AI steps wait for the user's review (the banner's check, store.rs `ProjectSummary::review_pending`); absent when nothing waits (UX-2003). */
  review_pending?: number | null;
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
  /** The stored result's roof area (the offer's figure), m²; null without one. */
  stored_area?: number | null;
  /** The current figures differ from the stored result's. */
  result_differs?: boolean;
  /** The roof took longer than the listing waits: no figures yet, a later listing has them. */
  figures_pending?: boolean;
  /** Buildings the core gave no roof for: as many as `parts`, the roof is not calculated; fewer, the area leaves them out (ERRU-005). */
  parts_failed?: number;
  /** Too complex to calculate (core work bound): no figures, `parts_failed` equals `parts`. */
  too_complex?: boolean;
  /** Hand-drawn faces: with any, the roof is (partly) calculated even when every building is without a roof (ERR-202). */
  faces?: number;
  /** The design is outside the limits: nothing is calculated, hand-drawn faces neither. */
  refused?: boolean;
}

/** A deleted job or removed file in the library's trash (core `store::TrashEntry`). */
export interface TrashEntry {
  /** Pass it to `restoreDeleted`. */
  entry: string;
  id: string;
  /** The job's name when it was deleted. */
  name: string;
  /** Set when only an attached file was removed. */
  file?: string;
  ts_ms: number;
  /** "user" for the app, otherwise the AI client's name. */
  actor: string;
  summary: string;
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

/** A roof line by its part and its index in that part's `geometry.lines`. */
export interface LineRef {
  part: number;
  index: number;
}

/** What the editor shows about one roof line (core `lineinfo::LineDetail`). */
export interface LineDetail extends LineRef {
  kind: LineKind;
  /** DACAD's line code (T, F, G, K, OL, OR, OS, P, c, S, W, WV, WH, WS). */
  code: string;
  a: [number, number, number];
  b: [number, number, number];
  length: number;
  /** The length the results count: `length` less the parts on or under chimneys, openings and dormers. */
  counted_length: number;
  plan_length: number;
  /** The line's own inclination against the horizontal, degrees. */
  slope_deg: number;
  z_low: number;
  z_high: number;
  /** The faces the line bounds; `wall` is absent for a dormer's own faces. */
  faces: { part: number; face: number; wall?: number; pitch_deg: number; material: number }[];
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
  /** With several buildings: the ones whose roof fails on its own too (from 0). */
  parts?: number[];
  /** The outer and inner wall (from 0) whose parallel planes never meet, where the core can tell. */
  edges?: number[];
  /** non_convex_opening: the towers (from 0) whose shaft, cut out like a chimney, causes it. */
  towers?: number[];
}

/** An input the roof engine cannot work with (that part is not calculated). */
export interface Problem {
  item: "part" | "dormer" | "chimney" | "opening" | "window" | "tower";
  index: number;
  edge?: number;
  code:
    | "too_few_edges"
    | "zero_length_wall"
    | "walls_cross"
    | "walls_overlap"
    | "start_nan"
    | "pitch_range"
    | "eave_nan"
    | "eave_negative"
    | "overhang_negative"
    | "direction_nan"
    | "stacked_nan"
    | "break_below_eave"
    | "break_duplicate"
    | "half_hip_below_eave"
    | "end_nan"
    | "bad_corners"
    | "bad_size"
    | "tower_nan"
    | "tower_size"
    | "tower_sides"
    | "tower_corner_radius"
    | "tower_base"
    | "tower_no_tiers"
    | "tower_tiers"
    | "tower_overhang"
    | "tower_tier_size"
    | "tower_flat_bulge"
    | "tower_bulge";
}

/** What looks wrong with a calculated roof (tectora_core::check). */
export interface RoofWarning {
  kind:
    | "outside_outline"
    | "overlapping_faces"
    | "area_mismatch"
    | "merged_walls"
    | "plane_over_wall"
    | "merged_planes"
    | "wing_joined"
    | "covered"
    | "cut_exceeds_face"
    | "dormer_off_roof"
    | "window_off_roof"
    | "chimney_off_roof"
    | "opening_off_roof"
    | "not_calculated_by_core"
    | "doubled_line"
    | "far_from_origin"
    | "no_roof_area"
    | "objects_overlap"
    | "dormer_above_roof"
    | "inside_another"
    | "merged_materials"
    | "hidden_under_neighbour"
    | "uncovered_plan"
    | "manual_face_crosses"
    | "manual_faces_overlap"
    | "manual_face_over_roof"
    | "tower_base_below_roof"
    | "tower_eave_below_roof";
  /** m outside, m² overlapping, the relative area difference, m² below zero (cut_exceeds_face), m higher (plane_over_wall), m apart (merged_planes), m counted twice (doubled_line), m from the origin (far_from_origin), pairs (objects_overlap), m above the roof (dormer_above_roof), m² hidden (hidden_under_neighbour), m² without roof (uncovered_plan), m² of plan shared (manual_faces_overlap) or over the roof (manual_face_over_roof), else 0. */
  value: number;
  /** merged_materials: the edge whose material the plane has, then the edge whose material is not used; merged_walls: the edge whose pitch is used, then the edge whose pitch and breaks are ignored; plane_over_wall, merged_planes: the outer wall, then the inner wall; wing_joined: the wing's walls; cut_exceeds_face: the walls of the faces. */
  edges?: number[];
  /** dormer_off_roof: the dormer's index. */
  dormer?: number;
  /** window_off_roof, chimney_off_roof, opening_off_roof: the object's index in its list. */
  item?: number;
  /** manual_face_crosses, manual_faces_overlap, manual_face_over_roof: the hand-drawn face's index in `manual_faces`. */
  face?: number;
  /** manual_faces_overlap: the second hand-drawn face of the pair. */
  other_face?: number;
  /** no_roof_area: the towers (from 0) whose shaft took all of the roof; tower_base_below_roof: the tower whose roof starts below this roof; tower_eave_below_roof: the tower whose overhang's eave lies below this roof. */
  towers?: number[];
}

/**
 * Where an object sits on its roof face (core `edit::position::Placement`):
 * from the lowest edge of the face along the slope (and in plan), and level
 * from its left and right edges seen up the slope. The lines are plan
 * dimension lines [from, to].
 */
export interface Placement {
  part: number;
  edge: number;
  pitch_deg: number;
  from_eave: number;
  from_eave_plan: number;
  from_left: number;
  from_right: number;
  eave_line: [Point2, Point2];
  left_line: [Point2, Point2];
  right_line: [Point2, Point2];
}

export interface Placements {
  dormers: (Placement | null)[];
  chimneys: (Placement | null)[];
  roof_openings: (Placement | null)[];
  windows: (Placement | null)[];
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
  /** Every face added by hand with its area (already in `totals`). */
  manual_faces?: { label: string; pitch_deg: number; area: number }[];
  /** Every tower's roof and shaft, in project order (their roof areas and lines are already in `totals`). */
  towers?: TowerReport[];
  /** Warnings that belong to no building (a crossing hand-drawn face in a job without one). */
  warnings?: RoofWarning[];
  totals: RoofQuantities;
  /** Line lengths by DACAD code (T, F, G, K, O, W). */
  codes: Record<string, number>;
  problems: Problem[];
  /** Each object's distances on its roof face (core `edit::position`), by index; null off the roof. */
  placements?: Placements;
  /** Every roof line's heights, slope and faces, part by part in line order. */
  line_details?: LineDetail[];
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
  /** What in the library's own folders blocks work (`id` is its path in the library). */
  foreign?: ForeignItem[];
}

/** What a [`ForeignItem`] blocks (store.rs `ForeignItem::kind`). */
export type ForeignKind = "activity" | "files" | "trash" | "exports" | "job_files" | "lock" | "lock_fix" | "new_lock";

/** A link, file or folder in the library's own folders that blocks work (store.rs `foreign_entries`). */
export interface ForeignItem extends BrokenFile {
  kind: ForeignKind;
  /** The job it is about (`job_files`, `lock`). */
  job?: string;
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
  /** The times of the AI steps waiting for review, oldest first (store.rs `PendingReview::steps`): a step the user reverted by itself is not one (UX-1501). Absent from an older bridge. */
  steps?: number[];
  before: Project | null;
  accepted_ms: number;
  /** Figures before the pending changes against now; null when unknown. */
  differences: FigureDifference[] | null;
  /** What differs between `before` (for an AI's new job the job as created) and the job now: the banner's change line and the plan's marks, the pair of `differences` (UX-2001). Absent from an older bridge. */
  changes?: Change[];
  /** A digest of `before`: it moves with the user's own edits while `since_ts_ms` and `count` stay (ARCH-2004). Absent from an older bridge. */
  before_rev?: string;
  /** False when none of the listed steps stands any more (they are history; the change is only `before` against now). Absent from an older bridge. */
  steps_stand?: boolean;
}

export interface ActivityItem extends ActivityInfo {
  changes: Change[];
  /** Only on the engine's recalculation of the stored result: the offer's date and engine, and each figure old -> new. */
  refreshed?: ResultRefresh;
  /** For a revert: the time of the oldest entry it undid. */
  undid_from?: number;
  /** For "offer result restored": the time of the engine's entry whose old result it put back. */
  restores?: number;
  /** The AI's reason of a revert or a put-back, as it ends the summary (store.rs `ActivityEntry::reason`, HUNT-904). */
  reason?: string;
  /** A later revert undid it (store.rs `undone_by`): muted in the history, and no Revert is drawn on it (UX-1505, UI-1501). */
  undone?: boolean;
  /** The entry created the job (no version before it): no Revert is drawn on it and Revert all stops after it (HUNT-1601). */
  created?: boolean;
  /** For a revert: the times of the changes it took out of the job, oldest first, at most 20 (store.rs `ActivityItem::took_out`, UX-1701). Absent for a change and from an older bridge. */
  took_out?: number[];
  /** For a revert: how many changes it took out (`took_out` is capped). */
  took_out_count?: number;
  /** For a revert: the times of the changes it put back into the job (a take-back of a revert), oldest first, at most 20. */
  put_back?: number[];
  /** For a revert: how many changes it put back. */
  put_back_count?: number;
}

/**
 * The engine's recalculation the job's stored result is (store.rs
 * `Store::recalculation`): the result card's "recalculated" line and the
 * history's "put the offer's figures back" show for this alone.
 */
export interface Recalculation extends ResultRefresh {
  /** The engine's change-log entry. */
  entry_ts_ms: number;
  /** When it was recalculated. */
  at: number;
  /** `taken_ms` of the recalculated result, the one in the job file. */
  taken_ms: number;
}

/** What opening a job recalculated (store.rs `RefreshedEntry`): the engine's change-log entry. */
export interface RefreshedEntry {
  entry_ts_ms: number;
  summary: string;
}

/** What the engine's recalculation of a stored result changed (the app's `refresh_figures`). */
export interface ResultRefresh {
  /** When the offer's result was stored. */
  offer_ms: number;
  from: string;
  to: string;
  differences: FigureDifference[];
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
  /** The agent saved this job or asked to show it (follow mode may open it). */
  follow?: boolean;
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
  /** A folder change an AI client asked for, waiting for the user. */
  folder_request?: FolderRequest;
  /** A file outside the job folders an AI client asked to read, waiting for the user. */
  file_request?: FileRequest;
  /** Files outside the job folders the user allowed AI clients to read. */
  allowed_files?: string[];
  /** Settings changes by AI clients, oldest first (core `AiSettingsChange`). */
  ai_changes?: AiSettingsChange[];
}

/** A settings change an AI client made, for the user to accept or revert. */
export interface AiSettingsChange {
  id: number;
  by: string;
  summary: string;
  /** Changed values by key: `locale`, `theme`, `follow_ai`, `roof_defaults.<key>`. */
  before: Record<string, unknown>;
  after: Record<string, unknown>;
  state: "open" | "accepted" | "reverted";
  /** Keys a Revert left as they were (changed again since); all of them: nothing was put back (UX-104). */
  kept?: string[];
}

export interface FileRequest {
  path: string;
  by: string;
  /** read_drawing, attach or import. */
  purpose: string;
}

export interface FolderRequest {
  changes: { projects_dir?: string | null; drawings_dir?: string | null };
  by: string;
  summary: string;
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
/** Actor of the entries Tectora writes itself (store.rs `ENGINE_ACTOR`): neither the user nor an AI. */
export const ENGINE_ACTOR = "tectora";
/** What an engine's entry of a received bundle is renamed to (store.rs `RECEIVED_ENGINE_ACTOR`): history of the sender's Tectora, never this engine's. */
export const RECEIVED_ENGINE_ACTOR = "tectora (received)";
/** What the sender's own entries of a received bundle are renamed to (store.rs `RECEIVED_USER_ACTOR`): a colleague's, never this user's (UX-1801). */
export const RECEIVED_USER_ACTOR = "user (received)";
/** An entry by this user or, received, by the sender: a user's revert or put-back, no AI's. */
export const byAUser = (actor: string) => actor === USER_ACTOR || actor === RECEIVED_USER_ACTOR;
export const isAi = (actor: string) => !byAUser(actor) && actor !== ENGINE_ACTOR && actor !== RECEIVED_ENGINE_ACTOR;

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
  /** Towers and their tiers (core `breakdown::TowerStep`; absent without towers); their roof areas are in `roof_area`. */
  towers?: BreakdownTower[];
  materials: {
    material: number;
    name?: string;
    /** `tower`: tower index from 0, `face` then its tier number from 1 (the flat top after the tiers). */
    items: { part: number | null; tower?: number; face: number | null; area: number }[];
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

/** A tower in the breakdown (core `breakdown::TowerStep`). */
export interface BreakdownTower {
  /** Tower number from 1. */
  no: number;
  label: string;
  tiers: {
    no: number;
    kind: "roof" | "wall";
    material: number;
    pitch_deg: number;
    /** A rectangle that is not square: pitch and area of its sides along the depth. */
    depth_sides?: SidePitch | null;
    bottom_height: number;
    top_height: number;
    bottom_perimeter: number;
    top_perimeter: number;
    area: number;
    /** The tier's row in the export's list ("Z3.01"); null for a vertical tier (core `breakdown::TowerTierStep::list_row`). */
    list_row?: string | null;
  }[];
  cap?: number;
  /** The flat top's row in the export's list. */
  cap_list_row?: string;
  /** Σ of the roof tiers and the flat top (the vertical tiers are wall area). */
  roof_area: number;
  wall_area: number;
  calculated: boolean;
}

export interface BreakdownLine {
  part: number | null;
  /** A tower's line: tower index and tier index from 0. Its length is the outline's or the profile's, not √(Δx² + Δy² + Δz²). */
  tower?: [number, number];
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

// Wire shapes of the core's calculation helpers (`tectora_core::edit::helper`,
// `edit::round`): the UI only sends values and shows what comes back.

export type ArcMode = "default" | "radius" | "ridge" | "length";

export interface ArcInput {
  /** Distance between the two wall lines, m. */
  span: number;
  overhang: number;
  eave_height: number;
  /** Segments on each side of the ridge; 0 = the core's count. */
  segments: number;
  mode: ArcMode;
  /** The radius, ridge height or developed length, by mode. */
  value: number;
}

export interface Arc {
  radius: number;
  ridge: number;
  /** Developed length of the whole arc, eave tip to eave tip. */
  length: number;
  segments: number;
  pitch_deg: number;
  breaks: PitchBreak[];
}

/** A plan map as the core takes it (`edit::transform::PlanMap`): angles in degrees. */
export type PlanMapSpec =
  | { kind: "move"; dx: number; dy: number }
  | { kind: "rotate"; centre: Point2; deg: number }
  | { kind: "mirror"; centre: Point2; deg: number };

export type CalcTarget = "pitch" | "eave" | "overhang";
export type ParallelTarget = CalcTarget | "position";
export type ParallelField = "width_a" | "width_b" | "rafter_a" | "rafter_b" | "ridge" | "top";
export type BuildUpEave = "normal" | "horizontal" | "vertical";

/** A helper and its values (`edit::helper::HelperRequest`). */
export type HelperRequest =
  | { tool: "triangle"; width?: number | null; rafter?: number | null; ridge?: number | null; width_includes_overhang: boolean }
  | { tool: "parallel"; target: ParallelTarget; steps: { field: ParallelField; value: number }[]; width_includes_overhang: boolean }
  | { tool: "point"; target: ParallelTarget; point: [number, number, number] }
  | { tool: "merge"; target: ParallelTarget; walls: number[] }
  | { tool: "hip"; neighbour: "next" | "prev"; by: "rafters" | "eaves"; hip: number; first: number; second: number }
  | { tool: "build_up"; thickness: number; eave: BuildUpEave }
  | { tool: "round_roof"; eave_height: number; overhang: number; segments: number; mode: ArcMode; value: number };

/** The 2 Parallele dialog's state (`edit::calc::ParallelState`). */
export interface ParallelState {
  span: number;
  top: number;
  w_a: number;
  w_b: number;
  h: number;
  a: { pitch_deg: number; eave_height: number; overhang: number };
  b: { pitch_deg: number; eave_height: number; overhang: number };
  shift_a: number;
  shift_b: number;
}

export interface HelperPreview {
  /** New values by wall; only the fields that differ from the walls now. */
  patches: Map<number, Partial<EdgeDef>>;
  /** Values were found and equal what the walls have now. */
  unchanged: boolean;
  triangle?: { pitch_deg: number; width: number; rafter: number; ridge: number };
  state?: ParallelState;
  rafters?: [number, number];
  /** Why worked-out values were not taken (an eave below a neighbouring gable's). */
  refused?: string;
}

export interface HelperContext {
  opposite: number | null;
  span: number;
  current: { w: number; h: number } | null;
  breaks: boolean;
  mates: number[];
  roof_points?: { p: [number, number, number]; kind: string }[];
}

export interface RoundArcAnswer {
  arc: Arc | null;
  default_arc: Arc | null;
  min_radius: number;
}
