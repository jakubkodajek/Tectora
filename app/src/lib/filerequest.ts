// The sentence shown for a file an AI client asked to read, by what it
// wants the file for (tectora-core `Settings::file_request`).

const PURPOSES: Record<string, string> = {
  read_drawing: "fileReq.readDrawing",
  attach: "fileReq.attach",
  import: "fileReq.import",
};

/** The translation key of a request's lead sentence; an unknown purpose reads as plain "read". */
export function fileRequestPurpose(purpose: string): string {
  // Own keys only: the purpose is read from settings.json, and a hand-edited
  // "constructor" would find Object.prototype's member (TEST-1403).
  return Object.hasOwn(PURPOSES, purpose) ? PURPOSES[purpose] : "fileReq.read";
}
