// Three-way merge of project JSON, for an external edit (AI agent, CLI) that
// arrives while the editor still has unsaved local edits: both sides started
// from `base` (the last saved file). Values changed on one side only are
// taken from that side; where both changed the same value, the external edit
// wins (it is shown for review and the local edit stays in the undo history).

type Json = unknown;

const isObject = (v: Json): v is Record<string, Json> => !!v && typeof v === "object" && !Array.isArray(v);

/** Drops what the store leaves out of its files (null, empty lists), so the comparison ignores it. */
function canon(v: Json): Json {
  if (Array.isArray(v)) return v.map(canon);
  if (isObject(v)) {
    const out: Record<string, Json> = {};
    for (const k of Object.keys(v).sort()) {
      const x = v[k];
      if (x === null || x === undefined || (Array.isArray(x) && x.length === 0)) continue;
      out[k] = canon(x);
    }
    return out;
  }
  return v;
}

const same = (a: Json, b: Json) => JSON.stringify(canon(a ?? null)) === JSON.stringify(canon(b ?? null));

export function merge3<T>(base: T, local: T, remote: T): T {
  return mergeValue(base, local, remote) as T;
}

function mergeValue(base: Json, local: Json, remote: Json): Json {
  if (same(local, base)) return remote;
  if (same(remote, base)) return local;
  if (isObject(local) && isObject(remote)) {
    const b = isObject(base) ? base : {};
    const out: Record<string, Json> = {};
    for (const k of new Set([...Object.keys(local), ...Object.keys(remote)])) {
      const v = mergeValue(b[k], local[k], remote[k]);
      if (v !== undefined) out[k] = v;
    }
    return out;
  }
  if (Array.isArray(local) && Array.isArray(remote) && Array.isArray(base) && local.length === base.length && remote.length === base.length) {
    return base.map((b, i) => mergeValue(b, local[i], remote[i]));
  }
  // Both sides changed the same value in different ways.
  return remote;
}
