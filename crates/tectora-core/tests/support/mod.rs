//! Shared helpers of the analytic and invariant tests: building roofs,
//! collecting mismatches and the structural checks every roof must pass.
#![allow(dead_code)]

use std::collections::BTreeMap;
use std::fmt::Write as _;

use tectora_core::geom::Point2;
use tectora_core::openings::generate_project;
use tectora_core::roof::RoofGeometry;
use tectora_core::{EdgeDef, Project, RoofPart};

// ---------------------------------------------------------------- building

pub fn edge(x: f64, y: f64, pitch_deg: f64, eave_height: f64, overhang: f64) -> EdgeDef {
    EdgeDef {
        start: Point2::new(x, y),
        pitch_deg,
        eave_height,
        overhang,
        half_hip: None,
        breaks: Vec::new(),
        priority: 0,
        direction_rad: None,
        clad: false,
        source: None,
        material: tectora_core::model::DEFAULT_MATERIAL,
    }
}

pub fn project(edges: Vec<EdgeDef>) -> Project {
    let mut p = Project::new("analytic");
    p.parts.push(RoofPart {
        name: "A".into(),
        edges,
    });
    p
}

/// Rectangle `w × d`, edges bottom, right, top, left (counter-clockwise),
/// each edge given as `(pitch, eave height, overhang)`.
pub fn rect(w: f64, d: f64, defs: [(f64, f64, f64); 4]) -> Project {
    let corners = [(0.0, 0.0), (w, 0.0), (w, d), (0.0, d)];
    project(
        corners
            .iter()
            .zip(defs)
            .map(|(&(x, y), (p, h, o))| edge(x, y, p, h, o))
            .collect(),
    )
}

/// A polygon with one definition on every edge.
pub fn uniform(pts: &[(f64, f64)], pitch: f64, eave: f64, ov: f64) -> Project {
    project(
        pts.iter()
            .map(|&(x, y)| edge(x, y, pitch, eave, ov))
            .collect(),
    )
}

pub fn tan(deg: f64) -> f64 {
    deg.to_radians().tan()
}

pub fn cos(deg: f64) -> f64 {
    deg.to_radians().cos()
}

/// Length of a line of plan length `l` rising `h`.
pub fn slant(l: f64, h: f64) -> f64 {
    l.hypot(h)
}

// ---------------------------------------------------------------- results

pub struct Roof {
    pub geos: Vec<RoofGeometry>,
    pub lines: BTreeMap<String, f64>,
    pub area: f64,
}

impl Roof {
    pub fn line(&self, code: &str) -> f64 {
        self.lines.get(code).copied().unwrap_or(0.0)
    }

    pub fn faces(&self) -> impl Iterator<Item = &tectora_core::roof::RoofFace> {
        self.geos.iter().flat_map(|g| &g.faces)
    }

    pub fn max_z(&self) -> f64 {
        self.faces()
            .flat_map(|f| &f.polygon)
            .map(|v| v[2])
            .fold(f64::MIN, f64::max)
    }
}

pub fn calc(p: &Project) -> Roof {
    let geos = generate_project(p);
    let mut lines: BTreeMap<String, f64> = BTreeMap::new();
    for l in geos.iter().flat_map(|g| &g.lines) {
        *lines.entry(l.dacad_code().to_string()).or_default() += l.length();
    }
    for (code, len) in geos.iter().flat_map(|g| &g.minus_lines) {
        *lines.entry(code.clone()).or_default() -= len;
    }
    lines.retain(|_, v| v.abs() > 1e-9);
    let area = geos
        .iter()
        .map(|g| g.faces.iter().map(|f| f.area).sum::<f64>() + g.clad_area)
        .sum();
    Roof { geos, lines, area }
}

/// Collects mismatches of one grid so a failing test reports all of them.
#[derive(Default)]
pub struct Report {
    pub cases: usize,
    pub failures: Vec<String>,
}

impl Report {
    pub fn case(&mut self) -> Case<'_> {
        self.cases += 1;
        Case {
            report: self,
            errors: String::new(),
        }
    }

    pub fn finish(self, name: &str) {
        eprintln!(
            "{name}: {} cases, {} failing",
            self.cases,
            self.failures.len()
        );
        assert!(
            self.failures.is_empty(),
            "{name}: {} of {} cases failed:\n{}",
            self.failures.len(),
            self.cases,
            self.failures
                .iter()
                .take(12)
                .cloned()
                .collect::<Vec<_>>()
                .join("\n")
        );
    }
}

pub struct Case<'a> {
    report: &'a mut Report,
    errors: String,
}

/// Lengths: 1 mm plus float32 noise. The engine works in float32 like
/// DACAD's core (about 1e-7 relative per operation).
pub fn tol(want: f64) -> f64 {
    1e-3 + 2e-5 * want.abs()
}

impl Case<'_> {
    pub fn eq(&mut self, what: &str, got: f64, want: f64) {
        self.eq_tol(what, got, want, tol(want));
    }

    /// `got` within `tol` of `want`; NaN never is.
    pub fn eq_tol(&mut self, what: &str, got: f64, want: f64, tol: f64) {
        let off = (got - want).abs();
        if off.is_nan() || off > tol {
            let _ = write!(self.errors, " {what}={got:.5} want {want:.5};");
        }
    }

    pub fn ok(&mut self, what: &str, cond: bool) {
        if !cond {
            let _ = write!(self.errors, " {what};");
        }
    }

    /// Lines of a code the roof must not have (pieces under a millimetre
    /// are DACAD's own rounding remainders).
    pub fn none(&mut self, roof: &Roof, codes: &[&str]) {
        for c in codes {
            let l = roof.line(c);
            if l.abs() > 1e-3 {
                let _ = write!(self.errors, " unexpected {c}={l:.5};");
            }
        }
    }

    pub fn only(&mut self, roof: &Roof, codes: &[&str]) {
        for (c, l) in &roof.lines {
            if !codes.contains(&c.as_str()) && l.abs() > 1e-3 {
                let _ = write!(self.errors, " unexpected {c}={l:.5};");
            }
        }
    }

    /// No mismatch so far.
    pub fn clean(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn done(self, label: String) {
        if let Some(e) = self.errors.strip_prefix(' ') {
            self.report.failures.push(format!("{label}: {e}"));
        }
    }
}

// ---------------------------------------------------------------- structure

pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn norm(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Area vector of a 3D polygon (Newell): its length is the area.
pub fn area_vector(poly: &[[f64; 3]]) -> [f64; 3] {
    let n = poly.len();
    let mut s = [0.0; 3];
    for i in 0..n {
        let c = cross(poly[i], poly[(i + 1) % n]);
        s = [s[0] + c[0], s[1] + c[1], s[2] + c[2]];
    }
    [s[0] / 2.0, s[1] / 2.0, s[2] / 2.0]
}

/// Distance of `p` from segment `ab` in 3D and the position along it.
pub fn seg_dist(p: [f64; 3], a: [f64; 3], b: [f64; 3]) -> (f64, f64) {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1] + ab[2] * ab[2];
    if l2 == 0.0 {
        return (norm(sub(p, a)), 0.0);
    }
    let ap = sub(p, a);
    let t = ((ap[0] * ab[0] + ap[1] * ab[1] + ap[2] * ab[2]) / l2).clamp(0.0, 1.0);
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    (norm(sub(p, q)), t)
}

/// Line codes that separate two roof faces; all others bound one face.
pub const INNER: [&str; 4] = ["F", "G", "K", "c"];

/// How closely a roof must follow its own geometry.
#[derive(Clone, Copy, PartialEq)]
pub enum Fit {
    /// Closed-form roofs: everything to a millimetre.
    Exact,
    /// Random roofs: DACAD's core merges points within 2 cm (POINT_EPS),
    /// takes events within 1 cm of height as one sweep level, and takes a
    /// face's area from its own edge list rather than from the merged
    /// corners. Lines may then end up to 2.5 cm from a face corner and a
    /// face's area may differ from its polygon by a sliver along its
    /// outline. The polygons themselves stay exact (checked elsewhere).
    Dacad,
}

/// Structural checks every roof without openings must pass:
///
/// 1. every face is planar and sloped at the pitch it reports,
/// 2. its area equals the area of its 3D polygon,
/// 3. the faces' plan areas add up to `plan` (they tile the eave outline
///    without gaps or overlaps, given that every face lies inside it),
/// 4. every face side lies on a roof line and every roof line on a face
///    side,
/// 5. without gables, the face sides add up to twice the inner lines
///    (ridge, hip, valley, break: two faces each) plus the outer ones
///    (eave, verge, wall: one face each). A gable rising above a lower
///    face makes a valley against the gable wall, which has no face.
pub fn check_structure(c: &mut Case<'_>, roof: &Roof, plan: f64) {
    check_structure_fit(c, roof, plan, Fit::Exact);
}

pub fn check_structure_fit(c: &mut Case<'_>, roof: &Roof, plan: f64, fit: Fit) {
    let (near, sliver) = match fit {
        Fit::Exact => (2e-3, 0.0),
        Fit::Dacad => (0.025, 0.02),
    };
    // A sound roof gets no plausibility warning (tectora_core::check).
    for g in &roof.geos {
        c.ok(
            &format!("plausibility warnings {:?}", g.warnings),
            g.warnings.is_empty(),
        );
    }
    let mut plan_sum = 0.0;
    let mut sides = 0.0;
    let mut segs: Vec<([f64; 3], [f64; 3])> = Vec::new();
    let lines: Vec<_> = roof.geos.iter().flat_map(|g| &g.lines).collect();
    let along = |a: [f64; 3], b: [f64; 3], p: [f64; 3], q: [f64; 3]| {
        // Segment pq runs along segment ab: its midpoint is on ab and the
        // two are parallel.
        let len = norm(sub(q, p));
        let m = [
            (p[0] + q[0]) / 2.0,
            (p[1] + q[1]) / 2.0,
            (p[2] + q[2]) / 2.0,
        ];
        len > 0.0
            && seg_dist(m, a, b).0 < near
            && norm(cross(sub(b, a), sub(q, p))) <= near * norm(sub(b, a))
    };
    for f in roof.faces() {
        let av = area_vector(&f.polygon);
        let a3 = norm(av);
        let n = f.polygon.len();
        let perim: f64 = (0..n)
            .map(|i| norm(sub(f.polygon[(i + 1) % n], f.polygon[i])))
            .sum();
        c.eq_tol(
            &format!("face {} area vs polygon", f.edge),
            f.area + f.deducted,
            a3,
            1e-4 * a3 + 5e-4 + sliver * perim,
        );
        // Faces come in either orientation, like DACAD's SO2 faces.
        plan_sum += av[2].abs();
        if a3 > 1e-6 {
            let pitch = (av[2].abs() / a3).clamp(-1.0, 1.0).acos().to_degrees();
            c.eq_tol("face pitch", pitch, f.pitch_deg, 0.01);
            let nrm = [av[0] / a3, av[1] / a3, av[2] / a3];
            let o = f.polygon[0];
            let off = f
                .polygon
                .iter()
                .map(|v| {
                    let d = sub(*v, o);
                    (d[0] * nrm[0] + d[1] * nrm[1] + d[2] * nrm[2]).abs()
                })
                .fold(0.0, f64::max);
            c.ok(
                &format!("face {} not planar ({off:.2e} m)", f.edge),
                off < 1e-3,
            );
        }
        for i in 0..n {
            let (a, b) = (f.polygon[i], f.polygon[(i + 1) % n]);
            let len = norm(sub(b, a));
            if len < 1e-4 {
                continue;
            }
            sides += len;
            segs.push((a, b));
            let covered = lines.iter().any(|l| along(l.a, l.b, a, b));
            c.ok(
                &format!("face {} side {a:?}-{b:?} has no line", f.edge),
                len < near || covered,
            );
        }
    }
    for l in &lines {
        let len = l.length();
        let covered = len < near
            || segs
                .iter()
                .any(|&(a, b)| along(a, b, l.a, l.b) || along(l.a, l.b, a, b));
        c.ok(
            &format!("{} line {:?}-{:?} bounds no face", l.dacad_code(), l.a, l.b),
            covered,
        );
    }
    // Merged corners leave slivers of overlap or gap along the face sides.
    let tiling = match fit {
        Fit::Exact => 1e-4 * plan.max(1.0),
        // Up to 1 mm on average along every side; the largest seen, each
        // byte-identical to DACAD in its core emulator, is 0.6 mm.
        Fit::Dacad => 1e-4 * plan.max(1.0) + 1e-3 * sides,
    };
    c.eq_tol("plan tiling", plan_sum, plan, tiling);
    let gables = roof.geos.iter().any(|g| !g.gables.is_empty());
    if !gables {
        let want_sides: f64 = roof
            .lines
            .iter()
            .map(|(code, l)| {
                if INNER.contains(&code.as_str()) {
                    2.0 * l
                } else {
                    *l
                }
            })
            .sum();
        let t = match fit {
            Fit::Exact => 2e-3 * (1.0 + want_sides / 100.0),
            Fit::Dacad => 0.1 + 2e-3 * want_sides / 10.0,
        };
        c.eq_tol("face sides vs lines", sides, want_sides, t);
    }
}

pub fn no_error(c: &mut Case<'_>, roof: &Roof) {
    for g in &roof.geos {
        c.ok(
            &format!("calculation error {:?}", g.error),
            g.error.is_none(),
        );
    }
}

// ---------------------------------------------------------------- helpers

/// Pushes a counter-clockwise polygon out by `d` with mitred corners.
pub fn offset(pts: &[(f64, f64)], d: f64) -> Vec<Point2> {
    let n = pts.len();
    let p = |i: usize| Point2::new(pts[i % n].0, pts[i % n].1);
    let line = |i: usize| {
        let (a, b) = (p(i), p(i + 1));
        let len = a.distance(b);
        let (nx, ny) = ((b.y - a.y) / len, -(b.x - a.x) / len);
        (
            Point2::new(a.x + nx * d, a.y + ny * d),
            (b.x - a.x, b.y - a.y),
        )
    };
    (0..n)
        .map(|i| {
            let (p0, d0) = line(i + n - 1);
            let (p1, d1) = line(i);
            let den = d0.0 * d1.1 - d0.1 * d1.0;
            let t = ((p1.x - p0.x) * d1.1 - (p1.y - p0.y) * d1.0) / den;
            Point2::new(p0.x + d0.0 * t, p0.y + d0.1 * t)
        })
        .collect()
}
