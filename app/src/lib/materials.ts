// Roof materials as DACAD numbers them: 1 tiles, 2 sheet metal, 3 stone,
// 4 slate, 5 solar panels (DACAD.exe string table 1601-1605). A project
// can rename them (DACAD: Optionen › Materialnamen).

import { t } from "./i18n.svelte";
import type { Project } from "./types";

/** Materials in DACAD's default list. */
export const DACAD_MATERIALS = 5;

/** Material numbers a wall can take in this project. */
export function materialIds(project: Project | null): number[] {
  const n = Math.max(DACAD_MATERIALS, project?.material_names?.length ?? 0);
  return Array.from({ length: n }, (_, i) => i + 1);
}

/** DACAD's default name of material `m`, in the UI language. */
export function defaultMaterialName(m: number): string {
  return m >= 1 && m <= DACAD_MATERIALS ? t(`material.${m}`) : t("material.n", { n: m });
}

/** The name of material `m`: the project's own, else DACAD's default. */
export function materialName(project: Project | null, m: number): string {
  const own = project?.material_names?.[m - 1]?.trim();
  return own ? own : defaultMaterialName(m);
}
