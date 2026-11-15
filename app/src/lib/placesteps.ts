// What the bottom bar says while a placing tool is active (review UX-012):
// the tool's name and the next click, as translation keys.

export type PlaceKind = "dormer" | "chimney" | "window" | "hole" | "line" | "face";

/** The bar's title and step keys for a placing tool; `counted` when the tool picks several points. */
export function placeSteps(kind: PlaceKind, points: number): { title: string; text: string; counted: boolean } {
  const text =
    kind === "dormer"
      ? "plan.placeDormer"
      : kind === "chimney"
        ? "plan.placeChimney"
        : kind === "hole"
          ? "plan.placeHole"
          : kind === "line"
            ? points > 0
              ? "plan.placeLineEnd"
              : "plan.placeLine"
            : kind === "face"
              ? "plan.placeFace"
              : "plan.placeWindow";
  return { title: `tool.${kind}`, text, counted: kind === "line" || kind === "face" };
}
