//! The records the stages work on.

use serde::{Deserialize, Serialize};

use super::plane::{IDENTITY, Mat, Plane};
use super::vec::V3;

/// A surface header: what a record of the INT or W3D file says about a
/// model, face, hole or wall, plus its plane.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Srf {
    /// Record letter ('I', 'G', 'O', 'D', 'L', 'T', 'W', ...).
    pub kind: i32,
    pub model: i32,
    /// Face or wall number: the first number written in a SOL face header.
    pub face: i32,
    /// Sub index: the second number written in a SOL face header.
    pub sub: i32,
    /// Eave flag: 1 when the face rises from an eave.
    pub flag5: i32,
    pub flag4: i32,
    /// Pitch in degrees.
    pub angle: f64,
    pub zmin: f64,
    pub plane: Plane,
}

impl Default for Srf {
    fn default() -> Self {
        Srf {
            kind: b'?' as i32,
            model: 0,
            face: 0,
            sub: 0,
            flag5: 0,
            flag4: 0,
            angle: 0.0,
            zmin: 0.0,
            plane: Plane::default(),
        }
    }
}

/// A polygon edge.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Edge {
    pub p1: V3,
    pub p2: V3,
    /// Index of the output line this edge became.
    pub line: i32,
    /// Where the edge comes from; `info.kind` is its type: 'd' face edge,
    /// 'W' face edge on a wall, 'w' ground plan or wall edge, 'L'/'T' hole
    /// edge, 's' edge of a cutter made by another face.
    pub info: Srf,
}

impl Edge {
    pub fn new(p1: V3, p2: V3, kind: u8) -> Edge {
        Edge {
            p1,
            p2,
            line: 0,
            info: Srf {
                kind: kind as i32,
                ..Srf::default()
            },
        }
    }

    pub fn dir(&self) -> V3 {
        self.p2.sub(self.p1)
    }

    pub fn len(&self) -> f64 {
        self.dir().length()
    }

    pub fn mid(&self) -> V3 {
        self.p1.add(self.dir().scale(0.5))
    }

    pub fn kind(&self) -> u8 {
        self.info.kind as u8
    }
}

/// A polygon: its edges in order, a sign (1, or -1 for a cutter), its plane
/// and a 4x4 frame used by the booleans.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Poly {
    pub edges: Vec<Edge>,
    pub sign: f64,
    pub plane: Plane,
    #[serde(with = "mat16")]
    pub m: Mat,
}

impl Default for Poly {
    fn default() -> Self {
        Poly {
            edges: Vec::new(),
            sign: 1.0,
            plane: Plane::default(),
            m: IDENTITY,
        }
    }
}

impl Poly {
    pub fn len(&self) -> usize {
        self.edges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }

    /// Edge `i`, counted modulo the edge count (negative allowed).
    pub fn at(&self, i: isize) -> &Edge {
        let n = self.edges.len() as isize;
        &self.edges[i.rem_euclid(n) as usize]
    }

    /// A polygon with the same attributes and no edges.
    pub fn empty_like(&self) -> Poly {
        Poly {
            edges: Vec::new(),
            ..self.clone()
        }
    }
}

/// A model, face, hole or wall with its polygon lists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Face {
    pub rec: Srf,
    /// The face clipped to the prism over its own model.
    pub own: Vec<Poly>,
    /// The face's current pieces.
    pub pieces: Vec<Poly>,
    /// Parts hidden by other faces or holes.
    pub cutters: Vec<Poly>,
    /// The header of the record that made each cutter.
    pub cut_src: Vec<Srf>,
}

impl Face {
    pub fn new(rec: Srf, poly: Poly) -> Face {
        Face {
            rec,
            own: Vec::new(),
            pieces: vec![poly],
            cutters: Vec::new(),
            cut_src: Vec::new(),
        }
    }

    pub fn kind(&self) -> u8 {
        self.rec.kind as u8
    }
}

/// An output line with the faces on its two sides.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub p1: V3,
    pub p2: V3,
    /// Type letter of the line: the creating face's kind, or 'T'/'L'.
    pub kind: i32,
    /// Number of references from face pieces (and found faces).
    pub n_plus: i32,
    pub kind_plus: i32,
    /// Number of references from cutters.
    pub n_minus: i32,
    pub kind_minus: i32,
    /// The line code written to the SOL.
    pub code: i32,
    /// +1, or -1 to write the length negative.
    pub sign: i32,
    pub x64: i32,
    pub a_set: i32,
    pub a: Srf,
    pub b_set: i32,
    pub b: Srf,
    /// Index of the creating edge in its polygon.
    pub idx: i32,
    /// Set when the line's face continues above a face found on the line.
    pub x200: i32,
}

/// Everything the stages read and write.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct State {
    /// Ground plans of 'G' and 'I' models.
    pub models: Vec<Face>,
    /// 'O' models (read, not used further).
    pub omodels: Vec<Face>,
    pub faces: Vec<Face>,
    pub holes: Vec<Face>,
    pub walls: Vec<Face>,
    pub lines: Vec<Line>,
    pub n_models: i32,
    pub n_faces: i32,
    pub n_holes: i32,
}

mod mat16 {
    use super::Mat;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(m: &Mat, s: S) -> Result<S::Ok, S::Error> {
        let v: Vec<f64> = m.iter().flatten().copied().collect();
        v.serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Mat, D::Error> {
        let v = Vec::<f64>::deserialize(d)?;
        let mut m = [[0.0; 4]; 4];
        for (k, x) in v.into_iter().take(16).enumerate() {
            m[k / 4][k % 4] = x;
        }
        Ok(m)
    }
}
