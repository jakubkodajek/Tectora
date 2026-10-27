//! State snapshots in the emulator's dump format, and a structural comparer.
//!
//! The emulator harness (`dump_state.py`, see PORTING.md) writes a
//! `state.json` with snapshots of the whole core state at named stages.
//! A snapshot is `{stage, seq, call, g: {globals}, recs: [records]}`;
//! every record is `{id, t, <fields>}` with the field names of the
//! [`super::state`] structs, floats as their `u32` bit patterns and
//! pointers as record ids (`"FACE#17"`, `null`). Ids are allocation numbers
//! and mean nothing across implementations, except the fixed names of the
//! static faces (`SFACE#1`, `SFACE#2`, `TFACE`) and the embedded eave face
//! `EFACE#n` of `WALL#n`.
//!
//! [`to_json`] writes a [`Core`] in this format, [`from_json`] reads one
//! back (so a slice can start from the emulator's state), and [`compare`]
//! matches two snapshots structurally: it walks both from the globals in
//! the same order, labels records by first visit, and reports every field
//! that differs together with the path that reached the record.

use std::collections::{BTreeMap, HashMap, VecDeque};

use serde_json::{Map, Value, json};

use super::num::{Tol, V3};
use super::state::*;

// ---------------------------------------------------------------------------------------------
// Core -> JSON

fn f(x: f32) -> Value {
    json!(x.to_bits())
}
fn f3(v: V3) -> Value {
    json!([v[0].to_bits(), v[1].to_bits(), v[2].to_bits()])
}

struct Names {
    eave_of: HashMap<u32, u32>,
}

impl Names {
    fn face(&self, id: Option<FaceId>) -> Value {
        match id {
            None => Value::Null,
            Some(FaceId(0)) => json!("SFACE#1"),
            Some(FaceId(1)) => json!("SFACE#2"),
            Some(FaceId(2)) => json!("TFACE"),
            Some(FaceId(i)) => match self.eave_of.get(&i) {
                Some(w) => json!(format!("EFACE#{w}")),
                None => json!(format!("FACE#{i}")),
            },
        }
    }
}

fn p<T: Copy>(id: Option<T>, kind: &str, ix: impl Fn(T) -> u32) -> Value {
    match id {
        None => Value::Null,
        Some(i) => json!(format!("{kind}#{}", ix(i))),
    }
}
fn pw(id: Option<WallId>) -> Value {
    p(id, "WALL", |i| i.0)
}
fn pm(id: Option<ModelId>) -> Value {
    p(id, "MODEL", |i| i.0)
}
fn ps(id: Option<SegId>) -> Value {
    p(id, "SEG", |i| i.0)
}
fn pn(id: Option<NodeId>) -> Value {
    p(id, "NODE24", |i| i.0)
}
fn pc(id: Option<CutId>) -> Value {
    p(id, "CUT", |i| i.0)
}
fn pl(id: Option<LevelId>) -> Value {
    p(id, "LEVEL", |i| i.0)
}
fn pe(id: Option<EdgeId>) -> Value {
    p(id, "EDGE", |i| i.0)
}

fn face_json(n: &Names, id: &str, x: &Face) -> Value {
    json!({
        "id": id, "t": "FACE", "sub": x.sub, "typ": x.typ, "o": f3(x.o), "x14": f3(x.x14),
        "u": f3(x.u), "w": f3(x.w), "n": f3(x.n), "d": f(x.d), "pitch": f(x.pitch), "h": f(x.h),
        "ov": f(x.ov), "zlo": f(x.zlo), "zhi": f(x.zhi), "b": x.b, "eave_done": x.eave_done,
        "head": pn(x.head), "closed": x.closed, "segs": ps(x.segs), "holes": pn(x.holes),
        "prev": n.face(x.prev), "next": n.face(x.next), "owner": pw(x.owner), "alias": n.face(x.alias),
    })
}

/// The globals as the dump writes them.
fn globals_json(c: &Core, n: &Names) -> Value {
    let g = &c.g;
    let mut m = Map::new();
    let mut put = |k: &str, v: Value| {
        m.insert(k.to_string(), v);
    };
    put("flags", json!(g.flags));
    put("ok", json!(g.ok));
    put("pass2", json!(g.pass2));
    put("cur_model", pm(g.cur_model));
    put("ring", pw(g.ring));
    put("models", pm(g.models));
    put("holes", pn(g.holes));
    put("cuts", pc(g.cuts));
    put("cut_tail", pc(g.cut_tail));
    put("cut_cur", pc(g.cut_cur));
    put("levels", pl(g.levels));
    put("level_cur", pl(g.level_cur));
    put("edges", pe(g.edges));
    put("n_lines", json!(g.n_lines));
    put("z_lo", f(g.z_lo));
    put("z_hi", f(g.z_hi));
    put("lvl_lo", f(g.lvl_lo));
    put("z_top", f(g.z_top));
    put("chain_wall", pw(g.chain_wall));
    put("n_t_models", json!(g.n_t_models));
    put("n_g_models", json!(g.n_g_models));
    put("n_holes", json!(g.n_holes));
    put("n_pieces", json!(g.n_pieces));
    put("n_pieces2", json!(g.n_pieces2));
    put("last_model_no", json!(g.last_model_no));
    put("last_wall", pw(g.last_wall));
    put("last_face", n.face(g.last_face));
    put("last_hidden", n.face(g.last_hidden));
    put("hole_chain", ps(g.hole_chain));
    put("cur_wall", pw(g.cur_wall));
    put("cur_face", n.face(g.cur_face));
    put("prev_wall", pw(g.prev_wall));
    put("next_wall", pw(g.next_wall));
    put("face_a", n.face(g.face_a));
    put("face_b", n.face(g.face_b));
    put("seg_cur", ps(g.seg_cur));
    put("seg_cur2", ps(g.seg_cur2));
    put("hole_cur", pn(g.hole_cur));
    put("tmp_line", json!(g.tmp_line.map(f32::to_bits)));
    put("llx", f3(g.llx));
    put("x05d0", json!(g.x05d0));
    put("x02ec", json!(g.x02ec));
    put("scratch100", json!(g.scratch100));
    put("scratch0fc", json!(g.scratch0fc));
    let names = [
        "code_none", "code_t", "code_f", "code_g", "code_k", "code_p", "code_vl", "code_vr", "code_os",
        "code_14", "code_wv", "code_wh", "code_ws", "code_c", "code_13", "code_s", "code_17", "code_wall",
    ];
    for (k, v) in names.iter().zip(g.codes.to_array()) {
        put(k, json!(v));
    }
    put("polyg_eps", f(g.tol.polyg));
    put("value_eps", f(g.tol.value));
    put("angle_eps", f(g.tol.angle));
    put("point_eps", f(g.tol.point));
    put("intsct_eps", f(g.tol.intsct));
    put("counters", json!(g.counters));
    Value::Object(m)
}

/// The state as a snapshot document (`{g, recs}`).
pub fn to_json(c: &Core) -> Value {
    let n = Names {
        eave_of: c.walls.iter().enumerate().map(|(i, w)| (w.eave.0, i as u32)).collect(),
    };
    let mut recs = Vec::new();
    for (i, x) in c.faces.iter().enumerate() {
        let id = match n.face(Some(FaceId(i as u32))) {
            Value::String(s) => s,
            _ => unreachable!(),
        };
        recs.push(face_json(&n, &id, x));
    }
    for (i, x) in c.models.iter().enumerate() {
        let mut r = json!({"id": format!("MODEL#{i}"), "t": "MODEL", "typ": x.typ, "no": x.no, "x08": x.x08});
        let o = r.as_object_mut().unwrap();
        for (k, v) in x.counters.iter().enumerate() {
            o.insert(format!("c{k}"), json!(v));
        }
        o.insert("ring".into(), pw(x.ring));
        o.insert("x58".into(), json!(x.x58));
        o.insert("x5c".into(), json!(x.x5c));
        o.insert("next".into(), pm(x.next));
        o.insert("gface".into(), n.face(Some(x.gface)));
        o.insert("gface2".into(), n.face(Some(x.gface2)));
        recs.push(r);
    }
    for (i, x) in c.walls.iter().enumerate() {
        recs.push(json!({
            "id": format!("WALL#{i}"), "t": "WALL", "typ": x.typ, "no": x.no, "extra": f(x.extra),
            "visited": x.visited, "zero": x.zero, "s0": f3(x.s0), "s": f3(x.s), "e": f3(x.e),
            "s_orig": f3(x.s_orig), "e_orig": f3(x.e_orig), "h": f(x.h), "faces": n.face(x.faces),
            "last_face": n.face(x.last_face), "cur": n.face(x.cur), "left": n.face(x.left),
            "right": n.face(x.right), "act": n.face(x.act), "prev": pw(x.prev), "next": pw(x.next),
        }));
    }
    for (i, x) in c.segs.iter().enumerate() {
        recs.push(json!({
            "id": format!("SEG#{i}"), "t": "SEG", "org": x.org, "kind": x.kind, "line": x.line,
            "code": x.code, "code2": x.code2, "fl": x.fl, "p": f3(x.p), "q": f3(x.q), "len": f(x.len),
            "fa": n.face(x.fa), "fb": n.face(x.fb), "prev": ps(x.prev), "next": ps(x.next),
        }));
    }
    for (i, x) in c.nodes.iter().enumerate() {
        recs.push(json!({
            "id": format!("NODE24#{i}"), "t": "NODE24", "no": x.no, "x04": x.x04, "x08": x.x08,
            "x0c": x.x0c, "b10": x.b10, "x14": x.x14, "area": f(x.area), "segs": ps(x.segs),
            "next": pn(x.next),
        }));
    }
    for (i, x) in c.cuts.iter().enumerate() {
        recs.push(json!({
            "id": format!("CUT#{i}"), "t": "CUT", "a": n.face(x.a), "ea": x.ea, "b": n.face(x.b),
            "eb": x.eb, "p": f3(x.p), "d": f3(x.d), "x": f3(x.x), "par": x.par, "prev": pc(x.prev),
            "next": pc(x.next),
        }));
    }
    for (i, x) in c.levels.iter().enumerate() {
        recs.push(json!({"id": format!("LEVEL#{i}"), "t": "LEVEL", "lo": f(x.lo), "hi": f(x.hi), "next": pl(x.next)}));
    }
    for (i, x) in c.edges.iter().enumerate() {
        recs.push(json!({
            "id": format!("EDGE#{i}"), "t": "EDGE", "no": x.no, "x04": x.x04, "cnt": x.cnt, "xc": x.xc,
            "x10": x.x10, "x14": x.x14, "x18": x.x18, "seg": ps(x.seg), "next": pe(x.next),
        }));
    }
    json!({"g": globals_json(c, &n), "recs": recs})
}

// ---------------------------------------------------------------------------------------------
// JSON -> Core

#[derive(Debug, thiserror::Error)]
#[error("dump: {0}")]
pub struct DumpError(pub String);

fn err<T>(s: impl Into<String>) -> Result<T, DumpError> {
    Err(DumpError(s.into()))
}

struct Ids {
    map: HashMap<String, (String, u32)>,
}

impl Ids {
    fn get(&self, v: &Value, kind: &str) -> Result<Option<u32>, DumpError> {
        match v {
            Value::Null => Ok(None),
            Value::String(s) => match self.map.get(s) {
                Some((k, i)) if k == kind => Ok(Some(*i)),
                Some((k, _)) => err(format!("{s} is a {k}, expected {kind}")),
                None => err(format!("dangling pointer {s}")),
            },
            _ => err(format!("bad pointer {v}")),
        }
    }
}

struct R<'a> {
    o: &'a Map<String, Value>,
    ids: &'a Ids,
}

impl R<'_> {
    fn v(&self, k: &str) -> Result<&Value, DumpError> {
        self.o.get(k).ok_or_else(|| DumpError(format!("missing field {k}")))
    }
    fn i(&self, k: &str) -> Result<i64, DumpError> {
        self.v(k)?.as_i64().ok_or_else(|| DumpError(format!("field {k} not an integer")))
    }
    fn i32(&self, k: &str) -> Result<i32, DumpError> {
        Ok(self.i(k)? as i32)
    }
    fn u32(&self, k: &str) -> Result<u32, DumpError> {
        Ok(self.i(k)? as u32)
    }
    fn u8(&self, k: &str) -> Result<u8, DumpError> {
        Ok(self.i(k)? as u8)
    }
    fn f(&self, k: &str) -> Result<f32, DumpError> {
        Ok(f32::from_bits(self.u32(k)?))
    }
    fn f3(&self, k: &str) -> Result<V3, DumpError> {
        let a = self.v(k)?.as_array().ok_or_else(|| DumpError(format!("field {k} not an array")))?;
        let g = |i: usize| a.get(i).and_then(Value::as_u64).map(|b| f32::from_bits(b as u32));
        match (g(0), g(1), g(2)) {
            (Some(x), Some(y), Some(z)) => Ok([x, y, z]),
            _ => err(format!("field {k} not a float triple")),
        }
    }
    fn face(&self, k: &str) -> Result<Option<FaceId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "FACE")?.map(FaceId))
    }
    fn wall(&self, k: &str) -> Result<Option<WallId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "WALL")?.map(WallId))
    }
    fn model(&self, k: &str) -> Result<Option<ModelId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "MODEL")?.map(ModelId))
    }
    fn seg(&self, k: &str) -> Result<Option<SegId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "SEG")?.map(SegId))
    }
    fn node(&self, k: &str) -> Result<Option<NodeId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "NODE24")?.map(NodeId))
    }
    fn cut(&self, k: &str) -> Result<Option<CutId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "CUT")?.map(CutId))
    }
    fn level(&self, k: &str) -> Result<Option<LevelId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "LEVEL")?.map(LevelId))
    }
    fn edge(&self, k: &str) -> Result<Option<EdgeId>, DumpError> {
        Ok(self.ids.get(self.v(k)?, "EDGE")?.map(EdgeId))
    }
}

fn read_face(r: &R) -> Result<Face, DumpError> {
    Ok(Face {
        sub: r.i32("sub")?,
        typ: r.u8("typ")?,
        o: r.f3("o")?,
        x14: r.f3("x14")?,
        u: r.f3("u")?,
        w: r.f3("w")?,
        n: r.f3("n")?,
        d: r.f("d")?,
        pitch: r.f("pitch")?,
        h: r.f("h")?,
        ov: r.f("ov")?,
        zlo: r.f("zlo")?,
        zhi: r.f("zhi")?,
        b: r.i32("b")?,
        eave_done: r.i32("eave_done")?,
        head: r.node("head")?,
        closed: r.u8("closed")?,
        segs: r.seg("segs")?,
        holes: r.node("holes")?,
        prev: r.face("prev")?,
        next: r.face("next")?,
        owner: r.wall("owner")?,
        alias: r.face("alias")?,
    })
}

/// Builds a [`Core`] from a snapshot (`{g, recs}`), e.g. one stage of an
/// emulator dump. Record order within each kind follows the dump.
pub fn from_json(snap: &Value) -> Result<Core, DumpError> {
    let recs = snap["recs"].as_array().ok_or_else(|| DumpError("no recs".into()))?;
    let mut c = Core::default();
    for _ in 0..3 {
        c.faces.push(Face::blank());
    }
    // Pass 1: assign indices.
    let mut ids = Ids { map: HashMap::new() };
    let mut count: HashMap<&str, u32> = HashMap::new();
    let mut wall_eave: HashMap<String, u32> = HashMap::new();
    for r in recs {
        let id = r["id"].as_str().ok_or_else(|| DumpError("record without id".into()))?;
        let t = r["t"].as_str().ok_or_else(|| DumpError("record without type".into()))?;
        let ix = match id {
            "SFACE#1" => 0,
            "SFACE#2" => 1,
            "TFACE" => 2,
            _ if id.starts_with("EFACE#") => continue,
            _ => {
                let k = if t == "FACE" { "FACE" } else { t };
                let n = count.entry(k).or_insert(if k == "FACE" { 3 } else { 0 });
                *n += 1;
                *n - 1
            }
        };
        ids.map.insert(id.to_string(), (t.to_string(), ix));
        if t == "WALL" {
            wall_eave.insert(format!("EFACE#{}", &id[5..]), 0);
        }
    }
    // Faces and eave faces share the face arena: eave faces after the others.
    let mut nface = *count.get("FACE").unwrap_or(&3);
    for r in recs {
        if r["t"] == "WALL" {
            let id = r["id"].as_str().unwrap();
            let e = format!("EFACE#{}", &id[5..]);
            ids.map.insert(e.clone(), ("FACE".into(), nface));
            wall_eave.insert(e, nface);
            nface += 1;
        }
    }
    c.faces.resize(nface as usize, Face::blank());
    let n = |k: &str| *count.get(k).unwrap_or(&0) as usize;
    let mut models = vec![None; n("MODEL")];
    let mut walls = vec![None; n("WALL")];
    let mut segs = vec![None; n("SEG")];
    let mut nodes = vec![None; n("NODE24")];
    let mut cuts = vec![None; n("CUT")];
    let mut levels = vec![None; n("LEVEL")];
    let mut edges = vec![None; n("EDGE")];
    for rv in recs {
        let o = rv.as_object().unwrap();
        let r = R { o, ids: &ids };
        let id = o["id"].as_str().unwrap();
        let (_, ix) = ids.map[id].clone();
        let ix = ix as usize;
        match o["t"].as_str().unwrap() {
            "FACE" => c.faces[ix] = read_face(&r)?,
            "MODEL" => {
                let mut counters = [0; 18];
                for (k, x) in counters.iter_mut().enumerate() {
                    *x = r.i32(&format!("c{k}"))?;
                }
                models[ix] = Some(Model {
                    typ: r.u8("typ")?,
                    no: r.i32("no")?,
                    x08: r.u32("x08")?,
                    counters,
                    ring: r.wall("ring")?,
                    x58: r.u32("x58")?,
                    x5c: r.u32("x5c")?,
                    next: r.model("next")?,
                    gface: r.face("gface")?.ok_or_else(|| DumpError("model without gface".into()))?,
                    gface2: r.face("gface2")?.ok_or_else(|| DumpError("model without gface2".into()))?,
                })
            }
            "WALL" => {
                walls[ix] = Some(Wall {
                    typ: r.u8("typ")?,
                    no: r.i32("no")?,
                    extra: r.f("extra")?,
                    visited: r.u8("visited")?,
                    zero: r.u8("zero")?,
                    s0: r.f3("s0")?,
                    s: r.f3("s")?,
                    e: r.f3("e")?,
                    s_orig: r.f3("s_orig")?,
                    e_orig: r.f3("e_orig")?,
                    h: r.f("h")?,
                    faces: r.face("faces")?,
                    last_face: r.face("last_face")?,
                    cur: r.face("cur")?,
                    left: r.face("left")?,
                    right: r.face("right")?,
                    act: r.face("act")?,
                    eave: FaceId(wall_eave[&format!("EFACE#{}", &id[5..])]),
                    prev: r.wall("prev")?,
                    next: r.wall("next")?,
                })
            }
            "SEG" => {
                segs[ix] = Some(Seg {
                    org: r.u8("org")?,
                    kind: r.u8("kind")?,
                    line: r.i32("line")?,
                    code: r.i32("code")?,
                    code2: r.i32("code2")?,
                    fl: r.u32("fl")?,
                    p: r.f3("p")?,
                    q: r.f3("q")?,
                    len: r.f("len")?,
                    fa: r.face("fa")?,
                    fb: r.face("fb")?,
                    prev: r.seg("prev")?,
                    next: r.seg("next")?,
                })
            }
            "NODE24" => {
                nodes[ix] = Some(Node {
                    no: r.i32("no")?,
                    x04: r.u32("x04")?,
                    x08: r.u32("x08")?,
                    x0c: r.u32("x0c")?,
                    b10: r.u8("b10")?,
                    x14: r.u32("x14")?,
                    area: r.f("area")?,
                    segs: r.seg("segs")?,
                    next: r.node("next")?,
                })
            }
            "CUT" => {
                cuts[ix] = Some(Cut {
                    a: r.face("a")?,
                    ea: r.i32("ea")?,
                    b: r.face("b")?,
                    eb: r.i32("eb")?,
                    p: r.f3("p")?,
                    d: r.f3("d")?,
                    x: r.f3("x")?,
                    par: r.i32("par")?,
                    prev: r.cut("prev")?,
                    next: r.cut("next")?,
                })
            }
            "LEVEL" => {
                levels[ix] = Some(Level {
                    lo: r.f("lo")?,
                    hi: r.f("hi")?,
                    next: r.level("next")?,
                })
            }
            "EDGE" => {
                edges[ix] = Some(Edge {
                    no: r.i32("no")?,
                    x04: r.u32("x04")?,
                    cnt: r.i32("cnt")?,
                    xc: r.i32("xc")?,
                    x10: r.u32("x10")?,
                    x14: r.u32("x14")?,
                    x18: r.u32("x18")?,
                    seg: r.seg("seg")?,
                    next: r.edge("next")?,
                })
            }
            t => return err(format!("unknown record type {t}")),
        }
    }
    c.models = models.into_iter().map(Option::unwrap).collect();
    c.walls = walls.into_iter().map(Option::unwrap).collect();
    c.segs = segs.into_iter().map(Option::unwrap).collect();
    c.nodes = nodes.into_iter().map(Option::unwrap).collect();
    c.cuts = cuts.into_iter().map(Option::unwrap).collect();
    c.levels = levels.into_iter().map(Option::unwrap).collect();
    c.edges = edges.into_iter().map(Option::unwrap).collect();

    let go = snap["g"].as_object().ok_or_else(|| DumpError("no globals".into()))?;
    let r = R { o: go, ids: &ids };
    let g = &mut c.g;
    g.flags = r.u32("flags")?;
    g.ok = r.u8("ok")?;
    g.pass2 = r.u8("pass2")?;
    g.cur_model = r.model("cur_model")?;
    g.ring = r.wall("ring")?;
    g.models = r.model("models")?;
    g.holes = r.node("holes")?;
    g.cuts = r.cut("cuts")?;
    g.cut_tail = r.cut("cut_tail")?;
    g.cut_cur = r.cut("cut_cur")?;
    g.levels = r.level("levels")?;
    g.level_cur = r.level("level_cur")?;
    g.edges = r.edge("edges")?;
    g.n_lines = r.i32("n_lines")?;
    g.z_lo = r.f("z_lo")?;
    g.z_hi = r.f("z_hi")?;
    g.lvl_lo = r.f("lvl_lo")?;
    g.z_top = r.f("z_top")?;
    g.chain_wall = r.wall("chain_wall")?;
    g.n_t_models = r.i32("n_t_models")?;
    g.n_g_models = r.i32("n_g_models")?;
    g.n_holes = r.i32("n_holes")?;
    g.n_pieces = r.i32("n_pieces")?;
    g.n_pieces2 = r.i32("n_pieces2")?;
    g.last_model_no = r.i32("last_model_no")?;
    g.last_wall = r.wall("last_wall")?;
    g.last_face = r.face("last_face")?;
    g.last_hidden = r.face("last_hidden")?;
    g.hole_chain = r.seg("hole_chain")?;
    g.cur_wall = r.wall("cur_wall")?;
    g.cur_face = r.face("cur_face")?;
    g.prev_wall = r.wall("prev_wall")?;
    g.next_wall = r.wall("next_wall")?;
    g.face_a = r.face("face_a")?;
    g.face_b = r.face("face_b")?;
    g.seg_cur = r.seg("seg_cur")?;
    g.seg_cur2 = r.seg("seg_cur2")?;
    g.hole_cur = r.node("hole_cur")?;
    let tl = r.v("tmp_line")?.as_array().ok_or_else(|| DumpError("tmp_line".into()))?;
    for (k, x) in g.tmp_line.iter_mut().enumerate() {
        *x = f32::from_bits(tl.get(k).and_then(Value::as_u64).unwrap_or(0) as u32);
    }
    g.llx = r.f3("llx")?;
    g.x05d0 = r.u32("x05d0")?;
    g.x02ec = r.u32("x02ec")?;
    g.scratch100 = r.u32("scratch100")?;
    g.scratch0fc = r.u32("scratch0fc")?;
    let names = [
        "code_none", "code_t", "code_f", "code_g", "code_k", "code_p", "code_vl", "code_vr", "code_os",
        "code_14", "code_wv", "code_wh", "code_ws", "code_c", "code_13", "code_s", "code_17", "code_wall",
    ];
    let mut codes = [0; 18];
    for (x, k) in codes.iter_mut().zip(names) {
        *x = r.i32(k)?;
    }
    g.codes = Codes::from_array(codes);
    g.tol = Tol {
        polyg: r.f("polyg_eps")?,
        value: r.f("value_eps")?,
        angle: r.f("angle_eps")?,
        point: r.f("point_eps")?,
        intsct: r.f("intsct_eps")?,
    };
    let cs = r.v("counters")?.as_array().ok_or_else(|| DumpError("counters".into()))?;
    for (k, x) in g.counters.iter_mut().enumerate() {
        *x = cs.get(k).and_then(Value::as_i64).unwrap_or(0) as i32;
    }
    Ok(c)
}

// ---------------------------------------------------------------------------------------------
// Snapshot documents

/// A `state.json` document of the emulator harness.
pub struct DumpDoc {
    pub doc: Value,
}

impl DumpDoc {
    pub fn load(path: &std::path::Path) -> Result<DumpDoc, DumpError> {
        let s = std::fs::read_to_string(path).map_err(|e| DumpError(format!("{}: {e}", path.display())))?;
        let doc = serde_json::from_str(&s).map_err(|e| DumpError(format!("{}: {e}", path.display())))?;
        Ok(DumpDoc { doc })
    }

    pub fn flags(&self) -> u32 {
        self.doc["flags"].as_u64().unwrap_or(0) as u32
    }

    pub fn snaps(&self) -> &[Value] {
        self.doc["snaps"].as_array().map(Vec::as_slice).unwrap_or(&[])
    }

    /// The snapshots of one stage, in run order.
    pub fn stage(&self, name: &str) -> Vec<&Value> {
        self.snaps().iter().filter(|s| s["stage"] == name).collect()
    }
}

// ---------------------------------------------------------------------------------------------
// Structural comparison

struct Canon {
    /// Canonical label of every record reached, with the path that reached it first.
    order: Vec<(String, String, String)>, // (label, id, path)
    label: HashMap<String, String>,
}

fn ptr_fields(r: &Map<String, Value>) -> Vec<(&String, &str)> {
    r.iter()
        .filter(|(k, _)| k.as_str() != "id" && k.as_str() != "t" && k.as_str() != "bt")
        .filter_map(|(k, v)| v.as_str().map(|s| (k, s)))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect()
}

fn canon(snap: &Value) -> (Canon, HashMap<String, &Map<String, Value>>) {
    let mut by_id = HashMap::new();
    if let Some(recs) = snap["recs"].as_array() {
        for r in recs {
            if let (Some(o), Some(id)) = (r.as_object(), r["id"].as_str()) {
                by_id.insert(id.to_string(), o);
            }
        }
    }
    let mut c = Canon {
        order: Vec::new(),
        label: HashMap::new(),
    };
    for s in ["SFACE#1", "SFACE#2", "TFACE"] {
        c.label.insert(s.into(), s.into());
        c.order.push((s.into(), s.into(), s.into()));
    }
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut queue: VecDeque<String> = VecDeque::new();
    let mut visit = |c: &mut Canon, id: &str, path: String, queue: &mut VecDeque<String>| {
        if c.label.contains_key(id) {
            return;
        }
        let t = match by_id.get(id) {
            Some(r) => r["t"].as_str().unwrap_or("?").to_string(),
            None => {
                c.label.insert(id.into(), format!("?{id}"));
                return;
            }
        };
        let n = counts.entry(t.clone()).or_default();
        let l = format!("{t}@{n}");
        *n += 1;
        c.label.insert(id.into(), l.clone());
        c.order.push((l, id.into(), path));
        queue.push_back(id.into());
    };
    let empty = Map::new();
    let g = snap["g"].as_object().unwrap_or(&empty);
    let g: BTreeMap<&String, &Value> = g.iter().collect();
    for (k, v) in g {
        if let Some(s) = v.as_str() {
            visit(&mut c, s, format!("g.{k}"), &mut queue);
        }
    }
    // Statics are roots too (their pointers are part of the state).
    for s in ["SFACE#1", "SFACE#2", "TFACE"] {
        queue.push_back(s.into());
    }
    while let Some(id) = queue.pop_front() {
        let Some(r) = by_id.get(&id).copied() else { continue };
        let base = c.order.iter().find(|x| x.1 == id).map(|x| x.2.clone()).unwrap_or_default();
        for (k, s) in ptr_fields(r) {
            visit(&mut c, s, format!("{base}.{k}"), &mut queue);
        }
        if r["t"] == "WALL" && id.starts_with("WALL#") {
            let e = format!("EFACE#{}", &id[5..]);
            visit(&mut c, &e, format!("{base}.eave"), &mut queue);
        }
    }
    (c, by_id)
}

fn show(v: &Value, k: &str) -> String {
    let fl = |x: &Value| -> String {
        match x.as_u64() {
            Some(b) => format!("{:?}", f32::from_bits(b as u32)),
            None => x.to_string(),
        }
    };
    let float_field = matches!(
        k,
        "o" | "x14" | "u" | "w" | "n" | "d" | "pitch" | "h" | "ov" | "zlo" | "zhi" | "s0" | "s" | "e"
            | "s_orig" | "e_orig" | "extra" | "p" | "q" | "len" | "area" | "x" | "lo" | "hi" | "z_lo"
            | "z_hi" | "lvl_lo" | "z_top" | "llx" | "tmp_line"
    );
    if !float_field {
        return v.to_string();
    }
    match v {
        Value::Array(a) => format!("[{}]", a.iter().map(fl).collect::<Vec<_>>().join(", ")),
        x => fl(x),
    }
}

/// Compares two snapshots structurally. Returns the differences (empty
/// when equal), at most `limit` of them. Records unreachable from the
/// globals are ignored.
pub fn compare(want: &Value, got: &Value, limit: usize) -> Vec<String> {
    let (ca, ra) = canon(want);
    let (cb, rb) = canon(got);
    let mut out = Vec::new();
    let mut push = |s: String| {
        if out.len() < limit {
            out.push(s);
        }
    };
    let empty = Map::new();
    let (ga, gb) = (
        want["g"].as_object().unwrap_or(&empty),
        got["g"].as_object().unwrap_or(&empty),
    );
    let lab = |c: &Canon, v: &Value| -> Value {
        match v.as_str() {
            Some(s) => Value::String(c.label.get(s).cloned().unwrap_or_else(|| format!("?{s}"))),
            None => v.clone(),
        }
    };
    for (k, va) in ga {
        // A probe of the dump harness, not state.
        if k == "relink_slot" {
            continue;
        }
        let vb = gb.get(k).unwrap_or(&Value::Null);
        let (x, y) = (lab(&ca, va), lab(&cb, vb));
        if x != y {
            push(format!("g.{k}: want {} got {}", show(&x, k), show(&y, k)));
        }
    }
    let lb: HashMap<&String, &String> = cb.order.iter().map(|(l, id, _)| (l, id)).collect();
    for (l, ida, path) in &ca.order {
        let Some(a) = ra.get(ida) else { continue };
        let Some(idb) = lb.get(l) else {
            push(format!("{l} ({path}): missing"));
            continue;
        };
        let Some(b) = rb.get(*idb) else { continue };
        if a["t"] != b["t"] {
            push(format!("{l} ({path}): type {} vs {}", a["t"], b["t"]));
            continue;
        }
        for (k, va) in a.iter() {
            if k == "id" || k == "t" || k == "bt" {
                continue;
            }
            let vb = b.get(k).unwrap_or(&Value::Null);
            let (x, y) = (lab(&ca, va), lab(&cb, vb));
            if x != y {
                push(format!("{l} ({path}).{k}: want {} got {}", show(&x, k), show(&y, k)));
            }
        }
    }
    if ca.order.len() < cb.order.len() {
        push(format!(
            "extra reachable records: {} vs {}",
            cb.order.len(),
            ca.order.len()
        ));
    }
    out
}
