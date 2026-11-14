//! DACAD's "Modellverschnitt": the stage that joins the roofs of several
//! models after each was built on its own.
//!
//! Input is the INT text (ground plans, roof faces, holes) and optionally
//! the W3D text (walls); output is the SOL text: the roof lines with their
//! codes and every face as a list of signed line references.
//!
//! This is a faithful re-implementation, quirks included: coordinates are
//! read as float32, every tolerance derives from the float32 unit 0.01,
//! point equality is DACAD's origin-based test, all sums keep DACAD's
//! evaluation order, and list orders and first-match rules are the same.
//! Do not tidy any of it up: the line codes hinge on knife-edge cases.

mod boolean;
mod classify;
mod edges;
mod io;
mod plane;
mod poly;
mod split;
mod stages;
mod tables;
mod types;
mod vec;

pub use io::ReadError;
pub use types::State;
pub use vec::V3;

/// The tolerances, all derived from the unit value.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tol {
    /// 0.01 of the unit: equality, coplanarity, splitting.
    pub t01: f64,
    /// 0.05 of the unit: above/below, gaps of the booleans.
    pub t05: f64,
    /// 0.025 of the unit: collinear edges.
    pub t025: f64,
    /// 0.07 of the unit: matching edges to lines.
    pub t07: f64,
    /// Snap grid factor of INT vertices.
    pub grid_int: f64,
    /// Snap grid factor of W3D vertices and of the booleans.
    pub grid_w3d: f64,
}

impl Tol {
    /// The tolerances for the unit value `r` (DACAD's length precision,
    /// a float32 0.01 by default).
    pub fn new(r: f64) -> Tol {
        let p = r * 100.0;
        Tol {
            t01: p * 0.01,
            t05: p * 0.05,
            t025: p * 0.025,
            t07: p * 0.07,
            grid_int: 1.0 / r,
            grid_w3d: 1000.0 / p,
        }
    }
}

impl Default for Tol {
    fn default() -> Self {
        Tol::new(0.01f32 as f64)
    }
}

/// A line of the result.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OutLine {
    pub p1: V3,
    pub p2: V3,
    /// DACAD line code (1 eave, 2 ridge, 3 hip, 4 valley, ..., 17 none, 18 wall).
    pub code: i32,
    /// +1, or -1 when DACAD writes the length negative.
    pub sign: i32,
}

/// What a face polygon of the result is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceKind {
    /// A roof or wall polygon.
    Surface,
    /// A cut-out, by the kind of what cut it: 1 chimney, 2 opening,
    /// 3 ground plan / wall, 4 roof face, 0 other.
    Cut(i32),
}

/// A face polygon of the result.
#[derive(Debug, Clone, PartialEq)]
pub struct OutFace {
    /// Face or wall number.
    pub face: i32,
    pub sub: i32,
    pub kind: FaceKind,
    /// Area; negative for cut-outs.
    pub area: f64,
    /// Line references: index into the lines (0-based) and +1 or -1 for the
    /// direction the polygon runs along the line.
    pub edges: Vec<(usize, i32)>,
}

/// The result of the stage.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub lines: Vec<OutLine>,
    pub faces: Vec<OutFace>,
    /// The full final state, for inspection and for writing the SOL.
    pub state: State,
    /// The pairs of models (RD2 model numbers, smaller first) where a face
    /// of one hides part of a face of the other: the roofs this stage
    /// joins. Read after the first stage, before the cutters are merged
    /// and subtracted; an observation only, the stages are unchanged.
    pub joins: Vec<(i32, i32)>,
}

/// Reads INT (and W3D) text into the stage's state.
pub fn parse(int: &str, w3d: Option<&str>, tol: &Tol) -> Result<State, ReadError> {
    io::read(int, w3d, tol)
}

/// Runs all stages on a state read by [`parse`].
pub fn run_state(st: &mut State, tol: &Tol) {
    stages::run(st, tol, usize::MAX);
}

/// Runs the stages `from..to` (0 = own/cutters, 1 = merge cutters,
/// 2 = subtract, 3 = tidy, 4 = split edges, 5 = edge records,
/// 6 = second faces, 7 = classify).
pub fn run_stages(st: &mut State, tol: &Tol, from: usize, to: usize) {
    stages::run_range(st, tol, from, to);
}

/// The SOL text of a state.
pub fn write_sol(st: &State, tol: &Tol) -> String {
    io::write(st, tol)
}

/// Parses and runs everything.
pub fn run(int: &str, w3d: Option<&str>) -> Result<Output, ReadError> {
    let tol = Tol::default();
    let mut st = parse(int, w3d, &tol)?;
    // The same stages as `run_state`, with a look at the cutters between
    // the first and the second.
    stages::run_range(&mut st, &tol, 0, 1);
    let joins = cutter_models(&st);
    stages::run_range(&mut st, &tol, 1, usize::MAX);
    if st.overflow {
        return Err(ReadError::TooComplex);
    }
    let mut out = output(st, &tol);
    out.joins = joins;
    Ok(out)
}

/// The model pairs of [`Output::joins`]: each face's cutters made by a
/// face of another model (holes and chimneys are no faces).
fn cutter_models(st: &State) -> Vec<(i32, i32)> {
    let models: Vec<i32> = st.faces.iter().map(|f| f.rec.model).collect();
    let mut pairs = Vec::new();
    for f in &st.faces {
        for s in f
            .cut_src
            .iter()
            .filter(|s| !b"TL".contains(&(s.kind as u8)))
        {
            let (a, b) = (f.rec.model, s.model);
            if a != b && models.contains(&b) && !pairs.contains(&(a.min(b), a.max(b))) {
                pairs.push((a.min(b), a.max(b)));
            }
        }
    }
    pairs.sort_unstable();
    pairs
}

/// The first difference between two SOL texts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SolDiff {
    /// Index of the token.
    pub token: usize,
    /// The tokens there; `None` past the end of the shorter text.
    pub a: Option<String>,
    pub b: Option<String>,
}

/// Compares two SOL texts token by token, numbers to within `eps`.
/// Returns the token count.
pub fn compare_sol(a: &str, b: &str, eps: f64) -> Result<usize, SolDiff> {
    let (ta, tb): (Vec<&str>, Vec<&str>) = (
        a.split_whitespace().collect(),
        b.split_whitespace().collect(),
    );
    for i in 0..ta.len().max(tb.len()) {
        let (x, y) = (ta.get(i), tb.get(i));
        let same = match (x, y) {
            (Some(x), Some(y)) => {
                x == y
                    || matches!((x.parse::<f64>(), y.parse::<f64>()), (Ok(p), Ok(q)) if (p - q).abs() <= eps)
            }
            _ => false,
        };
        if !same {
            return Err(SolDiff {
                token: i,
                a: x.map(|s| s.to_string()),
                b: y.map(|s| s.to_string()),
            });
        }
    }
    Ok(ta.len())
}

fn output(state: State, tol: &Tol) -> Output {
    let lines = state
        .lines
        .iter()
        .map(|l| OutLine {
            p1: l.p1,
            p2: l.p2,
            code: l.code,
            sign: l.sign,
        })
        .collect();
    let mut faces = Vec::new();
    let refs = |q: &types::Poly| {
        q.edges
            .iter()
            .map(|e| {
                let s = match state.lines.get(e.line as usize) {
                    Some(l) if vec::veq(e.p1, l.p1, tol.t01) => 1,
                    _ => -1,
                };
                (e.line as usize, s)
            })
            .collect::<Vec<_>>()
    };
    let mut push = |f: &types::Face, kind: FaceKind, area: f64, q: &types::Poly| {
        faces.push(OutFace {
            face: f.rec.face,
            sub: f.rec.sub,
            kind,
            area,
            edges: refs(q),
        })
    };
    for f in &state.faces {
        for q in &f.pieces {
            push(f, FaceKind::Surface, poly::area(q), q);
        }
        for (q, src) in f.cutters.iter().zip(&f.cut_src) {
            push(f, FaceKind::Cut(io::cut_kind(src.kind)), -poly::area(q), q);
        }
    }
    for f in &state.walls {
        for q in &f.pieces {
            push(f, FaceKind::Surface, poly::area(q), q);
        }
        for q in &f.cutters {
            push(f, FaceKind::Cut(3), -poly::area(q), q);
        }
    }
    Output {
        lines,
        faces,
        state,
        joins: Vec::new(),
    }
}
