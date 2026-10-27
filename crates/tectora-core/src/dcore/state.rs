//! The core's state: record arenas and the run-wide globals.
//!
//! The original keeps heap records linked by raw pointers plus a block of
//! globals. Here every record kind has an arena (`Vec`) and records refer
//! to each other by typed indices (`Option<FaceId>` for a nullable
//! pointer). Records are never removed from an arena: where the original
//! unlinks (and possibly frees) a record, the port only unlinks it, so
//! indices stay valid. List order, prepend/append behaviour and every
//! field the original stores are kept, including fields whose meaning is
//! only partly known (named after what they hold when they are read).
//!
//! Float fields are `f32` (the original's `float`); see `num` for the
//! arithmetic rules.

use super::num::{Tol, V3, ZERO3};

macro_rules! id_type {
    ($($(#[$m:meta])* $name:ident;)*) => {$(
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub struct $name(pub u32);
        impl $name {
            #[inline]
            pub fn ix(self) -> usize {
                self.0 as usize
            }
        }
    )*};
}

id_type! {
    /// A model (one roof body: the walls of one model number in the RD2).
    ModelId;
    /// A wall of a model's wall ring.
    WallId;
    /// A face (the hidden wall face, a roof face, an eave face, a model's
    /// ground face, the static hole faces).
    FaceId;
    /// An edge segment (face outline, hole outline, cut result).
    SegId;
    /// A list node owning a segment chain: a face's piece list or a hole.
    NodeId;
    /// A plane-plane cut record.
    CutId;
    /// A height level interval.
    LevelId;
    /// An entry of the edge (line) table written to SOL.
    EdgeId;
}

/// The static pseudo face that hole segments of a hole with height 0 point to.
pub const SFACE1: FaceId = FaceId(0);
/// The static pseudo face for holes with a nonzero height.
pub const SFACE2: FaceId = FaceId(1);
/// The static temporary face.
pub const TFACE: FaceId = FaceId(2);

/// Indices into the 18 counters (per model statistics, saved into and
/// restored from `Model::counters`).
pub mod ctr {
    /// Wall records read for the model.
    pub const WALLS_READ: usize = 0;
    /// Hidden wall faces left open / closed by the closing pass.
    pub const W_OPEN: usize = 1;
    pub const W_CLOSED: usize = 2;
    /// D (roof) face records read; D faces open / closed.
    pub const D_READ: usize = 3;
    pub const D_OPEN: usize = 4;
    pub const D_CLOSED: usize = 5;
    /// K (pitch break) face records read; K faces open / closed.
    pub const K_READ: usize = 6;
    pub const K_OPEN: usize = 7;
    pub const K_CLOSED: usize = 8;
    /// Walls in the ring (decremented when the ring merge drops a wall).
    pub const RING_WALLS: usize = 9;
    /// Faces created.
    pub const FACES: usize = 12;
    /// Faces open / closed (all types).
    pub const OPEN: usize = 13;
    pub const CLOSED: usize = 14;
    /// Segments created.
    pub const SEGS: usize = 15;
    /// Line count bookkeeping of the post-processing (merged lines subtracted).
    pub const LINES: usize = 16;
}

/// A model record.
#[derive(Debug, Clone, PartialEq)]
pub struct Model {
    /// Model type letter: 'D' (from W walls), 'G' (from G/T walls), 'g' (from g/t), 'w'.
    pub typ: u8,
    /// Model number from the RD2.
    pub no: i32,
    /// Word after the number (never written by the core; 0).
    pub x08: u32,
    /// Saved counters of this model (see [`ctr`]).
    pub counters: [i32; 18],
    /// First wall of the ring.
    pub ring: Option<WallId>,
    /// Two words set to 0 by the constructor.
    pub x58: u32,
    pub x5c: u32,
    /// Next model (the list is prepended while reading: last model first).
    pub next: Option<ModelId>,
    /// The ground ('G') face collecting the model's outline (dormer pass, INT).
    pub gface: FaceId,
    /// A second 'G' face created by the constructor (before `gface`).
    pub gface2: FaceId,
}

/// A wall record.
#[derive(Debug, Clone, PartialEq)]
pub struct Wall {
    /// Wall type letter as read: 'W', 'G', 'T', 'g', 't', 'w'.
    pub typ: u8,
    /// Wall number.
    pub no: i32,
    /// Seventh number of the wall record (0 under flag 0x200).
    pub extra: f32,
    /// Ring sort mark.
    pub visited: u8,
    /// 1 when the wall has zero length.
    pub zero: u8,
    /// Start point of a zero-length wall.
    pub s0: V3,
    /// Start and end point (the ring merge extends `e`).
    pub s: V3,
    pub e: V3,
    /// Start and end point as read (merge extends `e_orig` too).
    pub s_orig: V3,
    pub e_orig: V3,
    /// Wall height: the height of its first roof face, later the eave height.
    pub h: f32,
    /// First face: the hidden wall face ('W', sub -1), then the roof faces.
    pub faces: Option<FaceId>,
    /// Last face created for the wall.
    pub last_face: Option<FaceId>,
    /// Current face cursor of the sweep (starts at `faces`).
    pub cur: Option<FaceId>,
    /// Faces of the neighbouring walls the sweep links to.
    pub left: Option<FaceId>,
    pub right: Option<FaceId>,
    /// Active face of the sweep.
    pub act: Option<FaceId>,
    /// The embedded eave face (part of the wall record in the original).
    pub eave: FaceId,
    /// Ring links.
    pub prev: Option<WallId>,
    pub next: Option<WallId>,
}

/// A face record.
#[derive(Debug, Clone, PartialEq)]
pub struct Face {
    /// Sub number: -1 for hidden wall faces, the RD2 number for roof faces,
    /// 0x80000000 for eave faces.
    pub sub: i32,
    /// Face type letter: 'W' hidden wall face, 'D' roof, 'K' pitch break,
    /// 'G' ground, 'H' hole pseudo face.
    pub typ: u8,
    /// Frame origin.
    pub o: V3,
    /// Three floats after the origin (not written by the S1 code).
    pub x14: V3,
    /// Frame axes: `u` up the slope, `w` along the wall.
    pub u: V3,
    pub w: V3,
    /// Plane normal and offset (`n . x = d`).
    pub n: V3,
    pub d: f32,
    /// Pitch in radians (pi/2 for vertical faces).
    pub pitch: f32,
    /// Height of the face's lower edge (eave height).
    pub h: f32,
    /// Overhang.
    pub ov: f32,
    /// Validity range of the face in z.
    pub zlo: f32,
    pub zhi: f32,
    /// Third integer of the roof record (a flag; -1 marks an implied gable).
    pub b: i32,
    /// Word set to 0 by the constructor (eave handling mark).
    pub eave_done: i32,
    /// Head node of the face's piece list.
    pub head: Option<NodeId>,
    /// 1 when the outline is closed.
    pub closed: u8,
    /// Outline segments.
    pub segs: Option<SegId>,
    /// Holes in the face.
    pub holes: Option<NodeId>,
    /// Face list links within a wall.
    pub prev: Option<FaceId>,
    pub next: Option<FaceId>,
    /// Wall the face belongs to.
    pub owner: Option<WallId>,
    /// A face this one stands for after merging.
    pub alias: Option<FaceId>,
}

/// A segment record.
#[derive(Debug, Clone, PartialEq)]
pub struct Seg {
    /// First byte (never written by the constructor; 0).
    pub org: u8,
    /// Kind letter: 'L' hole outline, 'T', 'G', ... (set by the creator).
    pub kind: u8,
    /// Line number in the edge table.
    pub line: i32,
    /// Line code (see [`Codes`]).
    pub code: i32,
    /// Second code word.
    pub code2: i32,
    /// Copy of the flags word when the segment was created.
    pub fl: u32,
    /// End points.
    pub p: V3,
    pub q: V3,
    /// Length.
    pub len: f32,
    /// The two faces the segment separates.
    pub fa: Option<FaceId>,
    pub fb: Option<FaceId>,
    pub prev: Option<SegId>,
    pub next: Option<SegId>,
}

/// A list node: a face piece (the head node of a face) or a hole.
#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Number (hole number from the RD2, 0 for face head nodes).
    pub no: i32,
    /// Holes: the bits of the hole height (float); not written for face nodes.
    pub x04: u32,
    pub x08: u32,
    pub x0c: u32,
    pub b10: u8,
    pub x14: u32,
    /// Area of the piece.
    pub area: f32,
    pub segs: Option<SegId>,
    pub next: Option<NodeId>,
}

impl Node {
    /// The hole height stored in `x04`.
    pub fn hole_h(&self) -> f32 {
        f32::from_bits(self.x04)
    }
}

/// A plane cut record (sweep).
#[derive(Debug, Clone, PartialEq)]
pub struct Cut {
    pub a: Option<FaceId>,
    pub ea: i32,
    pub b: Option<FaceId>,
    pub eb: i32,
    pub p: V3,
    pub d: V3,
    pub x: V3,
    pub par: i32,
    pub prev: Option<CutId>,
    pub next: Option<CutId>,
}

/// A height interval `lo..hi` of the level list.
#[derive(Debug, Clone, PartialEq)]
pub struct Level {
    pub lo: f32,
    pub hi: f32,
    pub next: Option<LevelId>,
}

/// An edge table entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Edge {
    pub no: i32,
    pub x04: u32,
    pub cnt: i32,
    pub xc: i32,
    pub x10: u32,
    pub x14: u32,
    pub x18: u32,
    /// Representative segment (its line number, end points, code and length are written).
    pub seg: Option<SegId>,
    pub next: Option<EdgeId>,
}

/// The line code table (the values depend on flag bit 0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Codes {
    pub none: i32,
    pub t: i32,
    pub f: i32,
    pub g: i32,
    pub k: i32,
    pub p: i32,
    pub vl: i32,
    pub vr: i32,
    pub os: i32,
    pub c14: i32,
    pub wv: i32,
    pub wh: i32,
    pub ws: i32,
    pub c: i32,
    pub c13: i32,
    pub s: i32,
    pub c17: i32,
    pub wall: i32,
}

impl Codes {
    /// The table for a flags word.
    pub fn for_flags(flags: u32) -> Codes {
        let v: [i32; 18] = if flags & 1 == 0 {
            [0, 1, 2, 3, 4, 5, 6, 7, 8, 14, 9, 10, 11, 12, 13, 15, 17, 18]
        } else {
            [
                1, 14, 3, 2, 5, 13, 4, 12, 6, 31, 36, 44, 38, 15, 31, 46, 9, 23,
            ]
        };
        Codes::from_array(v)
    }

    pub fn from_array(v: [i32; 18]) -> Codes {
        Codes {
            none: v[0],
            t: v[1],
            f: v[2],
            g: v[3],
            k: v[4],
            p: v[5],
            vl: v[6],
            vr: v[7],
            os: v[8],
            c14: v[9],
            wv: v[10],
            wh: v[11],
            ws: v[12],
            c: v[13],
            c13: v[14],
            s: v[15],
            c17: v[16],
            wall: v[17],
        }
    }

    pub fn to_array(self) -> [i32; 18] {
        [
            self.none, self.t, self.f, self.g, self.k, self.p, self.vl, self.vr, self.os, self.c14,
            self.wv, self.wh, self.ws, self.c, self.c13, self.s, self.c17, self.wall,
        ]
    }
}

/// The run-wide globals.
#[derive(Debug, Clone, PartialEq)]
pub struct Globals {
    /// Flags word of the run (0x20 wall faces in SOL, 0x8 skip Modellverschnitt, ...).
    pub flags: u32,
    /// Model state: 1 once the model was processed successfully.
    pub ok: u8,
    /// Second sweep pass active.
    pub pass2: u8,
    pub cur_model: Option<ModelId>,
    /// Ring of the current model.
    pub ring: Option<WallId>,
    /// Model list head.
    pub models: Option<ModelId>,
    /// Hole list head (prepended while reading).
    pub holes: Option<NodeId>,
    pub cuts: Option<CutId>,
    pub cut_tail: Option<CutId>,
    pub cut_cur: Option<CutId>,
    pub levels: Option<LevelId>,
    pub level_cur: Option<LevelId>,
    pub edges: Option<EdgeId>,
    /// Number of lines in the edge table.
    pub n_lines: i32,
    /// Current sweep slab.
    pub z_lo: f32,
    pub z_hi: f32,
    pub lvl_lo: f32,
    pub z_top: f32,
    /// Wall the ring sort currently looks at (used by the fallback SOL).
    pub chain_wall: Option<WallId>,
    /// Statistics.
    pub n_t_models: i32,
    pub n_g_models: i32,
    pub n_holes: i32,
    pub n_pieces: i32,
    pub n_pieces2: i32,
    /// Reader state.
    pub last_model_no: i32,
    pub last_wall: Option<WallId>,
    pub last_face: Option<FaceId>,
    pub last_hidden: Option<FaceId>,
    pub hole_chain: Option<SegId>,
    /// Cursors the original keeps in globals (their final values are visible state).
    pub cur_wall: Option<WallId>,
    pub cur_face: Option<FaceId>,
    pub prev_wall: Option<WallId>,
    pub next_wall: Option<WallId>,
    pub face_a: Option<FaceId>,
    pub face_b: Option<FaceId>,
    pub seg_cur: Option<SegId>,
    pub seg_cur2: Option<SegId>,
    pub hole_cur: Option<NodeId>,
    /// A line (point, direction) scratch of the sweep.
    pub tmp_line: [f32; 6],
    /// Last line-line intersection point.
    pub llx: V3,
    pub x05d0: u32,
    pub x02ec: u32,
    pub scratch100: u32,
    pub scratch0fc: u32,
    pub codes: Codes,
    pub tol: Tol,
    pub counters: [i32; 18],
}

impl Default for Globals {
    fn default() -> Self {
        Globals {
            flags: 0,
            ok: 0,
            pass2: 0,
            cur_model: None,
            ring: None,
            models: None,
            holes: None,
            cuts: None,
            cut_tail: None,
            cut_cur: None,
            levels: None,
            level_cur: None,
            edges: None,
            n_lines: 0,
            z_lo: 0.0,
            z_hi: 0.0,
            lvl_lo: 0.0,
            z_top: 0.0,
            chain_wall: None,
            n_t_models: 0,
            n_g_models: 0,
            n_holes: 0,
            n_pieces: 0,
            n_pieces2: 0,
            last_model_no: 0,
            last_wall: None,
            last_face: None,
            last_hidden: None,
            hole_chain: None,
            cur_wall: None,
            cur_face: None,
            prev_wall: None,
            next_wall: None,
            face_a: None,
            face_b: None,
            seg_cur: None,
            seg_cur2: None,
            hole_cur: None,
            tmp_line: [0.0; 6],
            llx: ZERO3,
            x05d0: 0,
            x02ec: 0,
            scratch100: 0,
            scratch0fc: 0,
            codes: Codes::for_flags(0),
            tol: Tol::default(),
            counters: [0; 18],
        }
    }
}

/// The whole core state.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Core {
    pub models: Vec<Model>,
    pub walls: Vec<Wall>,
    /// Faces; the first three are the static faces [`SFACE1`], [`SFACE2`], [`TFACE`].
    pub faces: Vec<Face>,
    pub segs: Vec<Seg>,
    pub nodes: Vec<Node>,
    pub cuts: Vec<Cut>,
    pub levels: Vec<Level>,
    pub edges: Vec<Edge>,
    pub g: Globals,
}

impl Face {
    /// A face with every field zero.
    pub fn blank() -> Face {
        Face {
            sub: 0,
            typ: 0,
            o: ZERO3,
            x14: ZERO3,
            u: ZERO3,
            w: ZERO3,
            n: ZERO3,
            d: 0.0,
            pitch: 0.0,
            h: 0.0,
            ov: 0.0,
            zlo: 0.0,
            zhi: 0.0,
            b: 0,
            eave_done: 0,
            head: None,
            closed: 0,
            segs: None,
            holes: None,
            prev: None,
            next: None,
            owner: None,
            alias: None,
        }
    }
}

impl Node {
    pub fn blank() -> Node {
        Node {
            no: 0,
            x04: 0,
            x08: 0,
            x0c: 0,
            b10: 0,
            x14: 0,
            area: 0.0,
            segs: None,
            next: None,
        }
    }
}

/// Arguments of the face constructor.
#[derive(Debug, Clone, Copy)]
pub struct NewFace {
    pub typ: u8,
    pub sub: i32,
    pub o: V3,
    pub u: V3,
    pub w: V3,
    pub pitch: f32,
    pub h: f32,
    pub ov: f32,
    pub b: i32,
    pub owner: Option<WallId>,
    pub prev: Option<FaceId>,
    pub next: Option<FaceId>,
}

impl Core {
    /// A fresh state: the static faces and the initial globals of a run
    /// (the run initialisation, before the per-model reset).
    pub fn new(flags: u32) -> Core {
        let mut c = Core::default();
        for _ in 0..3 {
            c.faces.push(Face::blank());
        }
        c.g.flags = flags;
        c.init_run();
        c
    }

    /// Run initialisation: code table, statistics, static hole faces, list heads.
    pub fn init_run(&mut self) {
        let g = &mut self.g;
        g.codes = Codes::for_flags(g.flags);
        g.n_lines = 0;
        g.n_t_models = 0;
        g.n_g_models = 0;
        g.n_holes = 0;
        g.n_pieces = 0;
        g.counters = [0; 18];
        for (i, h) in [(SFACE1, 0.0f32), (SFACE2, 1.0f32)] {
            let f = &mut self.faces[i.ix()];
            f.sub = i.0 as i32 + 1;
            f.typ = b'H';
            f.h = h;
            f.owner = None;
        }
        g.models = None;
        g.holes = None;
        g.edges = None;
        g.last_model_no = 0xffff;
    }

    /// Per-model reset of the reader/sweep globals.
    pub fn reset_model(&mut self) {
        let g = &mut self.g;
        g.counters = [0; 18];
        g.ring = None;
        g.last_wall = None;
        g.last_hidden = None;
        g.last_face = None;
        g.cuts = None;
        g.cut_tail = None;
        g.x05d0 = 0;
        g.x02ec = 0;
        g.ok = 0;
    }

    pub fn face(&self, f: FaceId) -> &Face {
        &self.faces[f.ix()]
    }
    pub fn face_mut(&mut self, f: FaceId) -> &mut Face {
        &mut self.faces[f.ix()]
    }
    pub fn wall(&self, w: WallId) -> &Wall {
        &self.walls[w.ix()]
    }
    pub fn wall_mut(&mut self, w: WallId) -> &mut Wall {
        &mut self.walls[w.ix()]
    }
    pub fn seg(&self, s: SegId) -> &Seg {
        &self.segs[s.ix()]
    }
    pub fn seg_mut(&mut self, s: SegId) -> &mut Seg {
        &mut self.segs[s.ix()]
    }
    pub fn node(&self, n: NodeId) -> &Node {
        &self.nodes[n.ix()]
    }
    pub fn node_mut(&mut self, n: NodeId) -> &mut Node {
        &mut self.nodes[n.ix()]
    }
    pub fn model(&self, m: ModelId) -> &Model {
        &self.models[m.ix()]
    }
    pub fn model_mut(&mut self, m: ModelId) -> &mut Model {
        &mut self.models[m.ix()]
    }

    /// Node constructor (`no`, area, segment chain, next).
    pub fn new_node(
        &mut self,
        no: i32,
        area: f32,
        segs: Option<SegId>,
        next: Option<NodeId>,
    ) -> NodeId {
        let id = NodeId(self.nodes.len() as u32);
        self.nodes.push(Node {
            no,
            area,
            segs,
            next,
            ..Node::blank()
        });
        id
    }

    /// Face constructor: creates the head node, registers the face as its
    /// owner's last face and counts it.
    pub fn new_face(&mut self, a: NewFace) -> FaceId {
        let id = FaceId(self.faces.len() as u32);
        self.faces.push(Face::blank());
        let head = self.new_node(0, 0.0, None, None);
        let f = &mut self.faces[id.ix()];
        f.typ = a.typ;
        f.sub = a.sub;
        f.o = a.o;
        f.u = a.u;
        f.w = a.w;
        f.pitch = a.pitch;
        f.h = a.h;
        f.ov = a.ov;
        f.b = a.b;
        f.eave_done = 0;
        f.head = Some(head);
        f.closed = 0;
        f.segs = None;
        f.holes = None;
        f.owner = a.owner;
        f.alias = None;
        f.prev = a.prev;
        f.next = a.next;
        if let Some(w) = a.owner {
            self.walls[w.ix()].last_face = Some(id);
        }
        self.g.counters[ctr::FACES] += 1;
        id
    }

    /// Segment constructor.
    #[allow(clippy::too_many_arguments)]
    pub fn new_seg(
        &mut self,
        kind: u8,
        line: i32,
        p: V3,
        q: V3,
        prev: Option<SegId>,
        next: Option<SegId>,
        fa: Option<FaceId>,
        fb: Option<FaceId>,
    ) -> SegId {
        let id = SegId(self.segs.len() as u32);
        self.segs.push(Seg {
            org: 0,
            kind,
            line,
            code: self.g.codes.none,
            code2: 0,
            fl: self.g.flags,
            p,
            q,
            len: 0.0,
            fa,
            fb,
            prev,
            next,
        });
        self.g.counters[ctr::SEGS] += 1;
        id
    }

    /// Wall constructor: links nothing but its own fields; counts the ring wall.
    pub fn new_wall(
        &mut self,
        typ: u8,
        no: i32,
        extra: f32,
        faces: FaceId,
        prev: Option<WallId>,
        next: Option<WallId>,
    ) -> WallId {
        let id = WallId(self.walls.len() as u32);
        // The embedded eave face lives in the face arena.
        let eave = FaceId(self.faces.len() as u32);
        self.faces.push(Face::blank());
        self.walls.push(Wall {
            typ,
            no,
            extra,
            visited: 0,
            zero: 0,
            s0: ZERO3,
            s: ZERO3,
            e: ZERO3,
            s_orig: ZERO3,
            e_orig: ZERO3,
            h: 0.0,
            faces: Some(faces),
            last_face: None,
            cur: Some(faces),
            left: None,
            right: None,
            act: None,
            eave,
            prev,
            next,
        });
        self.g.counters[ctr::RING_WALLS] += 1;
        id
    }

    /// Model constructor: creates the two ground faces (the second one first).
    pub fn new_model(&mut self, typ: u8, no: i32, ring: WallId, next: Option<ModelId>) -> ModelId {
        let blank = NewFace {
            typ: b'G',
            sub: 0,
            o: ZERO3,
            u: ZERO3,
            w: ZERO3,
            pitch: 0.0,
            h: 0.0,
            ov: 0.0,
            b: 0,
            owner: None,
            prev: None,
            next: None,
        };
        let gface2 = self.new_face(blank);
        let gface = self.new_face(blank);
        let id = ModelId(self.models.len() as u32);
        self.models.push(Model {
            typ,
            no,
            x08: 0,
            counters: [0; 18],
            ring: Some(ring),
            x58: 0,
            x5c: 0,
            next,
            gface,
            gface2,
        });
        id
    }

    /// Saves the counters into the head model (the reader does this when a
    /// new model starts and at the end of the file).
    pub fn save_counters_to_head(&mut self) {
        if let Some(m) = self.g.models {
            self.models[m.ix()].counters = self.g.counters;
        }
    }
}
