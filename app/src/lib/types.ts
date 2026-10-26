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

export interface Project {
  schema_version: number;
  name: string;
  notes: string;
  parts: RoofPart[];
  dormers: unknown[];
  chimneys: { position: Point2; width: number; depth: number }[];
}

export interface ProjectSummary {
  id: string;
  name: string;
  notes: string;
  modified_ms: number;
  roof_area: number;
  parts: number;
  outlines: [number, number][][];
}

export interface ExampleInfo {
  id: string;
  name: string;
  description: string;
  outlines: [number, number][][];
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
}

export const GABLE_PITCH = 90;
export const isGable = (e: EdgeDef) => e.pitch_deg >= 89.999;

export type Selection = { part: number; edge: number } | null;
